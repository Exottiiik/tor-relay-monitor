//! SAFECOOKIE only: authenticate the peer before sending a controller proof.
use crate::{
    config::Config,
    protocol::{Command, Connection},
};
use anyhow::{Context, Result, bail};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::path::Path;
use tokio::io::AsyncReadExt;
use zeroize::Zeroizing;

pub const SERVER_KEY: &[u8] = b"Tor safe cookie authentication server-to-controller hash";
pub const CLIENT_KEY: &[u8] = b"Tor safe cookie authentication controller-to-server hash";

/// Decode Tor's C-style quoted strings, including octal escapes used in paths.
pub fn unquote(input: &str) -> Result<(String, usize)> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'"') {
        bail!("Expected a quoted control string");
    }
    let mut output = Vec::new();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                return Ok((
                    String::from_utf8(output).context("Non-UTF-8 control path")?,
                    i + 1,
                ));
            }
            b'\\' => {
                i += 1;
                match bytes.get(i).copied().context("Incomplete control escape")? {
                    b'n' => output.push(b'\n'),
                    b'r' => output.push(b'\r'),
                    b't' => output.push(b'\t'),
                    b'\\' => output.push(b'\\'),
                    b'"' => output.push(b'"'),
                    b'0'..=b'7' => {
                        let mut value = 0u16;
                        let mut count = 0;
                        while count < 3 && i < bytes.len() && (b'0'..=b'7').contains(&bytes[i]) {
                            value = value * 8 + u16::from(bytes[i] - b'0');
                            i += 1;
                            count += 1;
                        }
                        if value > 255 {
                            bail!("Invalid octal control escape");
                        }
                        output.push(value as u8);
                        i -= 1;
                    }
                    _ => bail!("Unknown control string escape"),
                }
            }
            byte => output.push(byte),
        }
        i += 1;
    }
    bail!("Unterminated control string")
}

pub fn proof(key: &[u8], cookie: &[u8], client: &[u8], server: &[u8]) -> Result<Hmac<Sha256>> {
    if cookie.len() != 32 || client.len() != 32 || server.len() != 32 {
        bail!("SAFECOOKIE requires 32-byte inputs");
    }
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key).context("Cannot initialize SAFECOOKIE HMAC")?;
    mac.update(cookie);
    mac.update(client);
    mac.update(server);
    Ok(mac)
}

async fn read_cookie(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    let mut options = tokio::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    let file = options.open(path).await.context("Cannot open Tor cookie; check control group permissions or --cookie (symlinks are refused on Unix)")?;
    let metadata = file.metadata().await?;
    if !metadata.is_file() || metadata.len() != 32 {
        bail!("Tor authentication cookie must be a regular file of exactly 32 bytes");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o004 != 0 {
            bail!("Refusing a world-readable Tor authentication cookie");
        }
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(33));
    file.take(33).read_to_end(&mut bytes).await?;
    if bytes.len() != 32 {
        bail!("Tor authentication cookie changed size while reading");
    }
    Ok(bytes)
}

pub async fn authenticate(connection: &mut Connection, config: &Config) -> Result<()> {
    let response = connection.command(Command::ProtocolInfo).await?.success()?;
    let auth = response
        .lines
        .iter()
        .find(|l| l.starts_with("AUTH "))
        .context("Tor did not advertise authentication methods")?;
    let methods = auth
        .split_whitespace()
        .find_map(|v| v.strip_prefix("METHODS="))
        .unwrap_or("");
    if !methods.split(',').any(|v| v == "SAFECOOKIE") {
        bail!(
            "Tor must offer SAFECOOKIE; NULL, COOKIE and password authentication are not enabled by this monitor"
        );
    }
    let cookie_path = match &config.cookie {
        Some(path) => path.clone(),
        None => {
            let start = auth
                .find(" COOKIEFILE=")
                .context("Tor did not advertise a cookie file; use --cookie")?
                + 12;
            unquote(&auth[start..])?.0.into()
        }
    };
    let cookie = read_cookie(&cookie_path).await?;
    let mut nonce = Zeroizing::new([0u8; 32]);
    getrandom::fill(nonce.as_mut()).context("OS random generator unavailable")?;
    let nonce_hex = Zeroizing::new(hex::encode(nonce.as_ref()));
    let challenge = connection
        .command(Command::Challenge(&nonce_hex))
        .await?
        .success()?;
    let line = challenge
        .lines
        .first()
        .context("Missing SAFECOOKIE challenge")?;
    if !line.starts_with("AUTHCHALLENGE ") {
        bail!("Invalid SAFECOOKIE challenge");
    }
    let field = |name: &str| -> Result<Vec<u8>> {
        let value = line
            .split_whitespace()
            .find_map(|f| f.strip_prefix(name))
            .context("Missing SAFECOOKIE challenge field")?;
        if value.len() != 64 {
            bail!("Invalid SAFECOOKIE challenge field size");
        }
        hex::decode(value).context("Invalid SAFECOOKIE challenge encoding")
    };
    let server_nonce = field("SERVERNONCE=")?;
    let server_hash = field("SERVERHASH=")?;
    proof(SERVER_KEY, &cookie, nonce.as_ref(), &server_nonce)?
        .verify_slice(&server_hash)
        .map_err(|_| {
            anyhow::anyhow!("SAFECOOKIE server verification failed; no controller proof was sent")
        })?;
    let client_proof = Zeroizing::new(
        proof(CLIENT_KEY, &cookie, nonce.as_ref(), &server_nonce)?
            .finalize()
            .into_bytes()
            .to_vec(),
    );
    let encoded = Zeroizing::new(hex::encode(&client_proof[..]));
    connection
        .command(Command::Authenticate(&encoded))
        .await?
        .success()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_paths() {
        assert_eq!(
            unquote(r#""/run/a\040b/\"cookie" EXTRA=1"#).unwrap().0,
            "/run/a b/\"cookie"
        );
        assert_eq!(
            unquote(r#""C:\\Tor\\cookie""#).unwrap().0,
            "C:\\Tor\\cookie"
        );
        for invalid in ["no quote", "\"unterminated", "\"\\777\"", "\"\\x\""] {
            assert!(unquote(invalid).is_err());
        }
    }
    #[test]
    fn safecookie_known_answer_and_tampering() {
        // Independently generated using Python hmac/sha256, repeated in integration peers.
        let mac = proof(SERVER_KEY, &[0; 32], &[1; 32], &[2; 32]).unwrap();
        assert_eq!(
            hex::encode(mac.clone().finalize().into_bytes()),
            "169a3aeb27e1e868ead7d64385a4d0bfe4af56f453bb22edc585234edfefd8e5"
        );
        assert!(mac.verify_slice(&[0; 32]).is_err());
        assert!(proof(CLIENT_KEY, &[0; 31], &[1; 32], &[2; 32]).is_err());
    }
}
