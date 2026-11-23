//! Optional TOML configuration and a deliberately small command line.
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{net::SocketAddr, path::PathBuf, time::Duration};

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub socket: Option<PathBuf>,
    pub tcp: Option<SocketAddr>,
    pub cookie: Option<PathBuf>,
    pub timeout_seconds: u64,
    pub reconnect_seconds: u64,
    pub poll_seconds: u64,
    pub no_color: bool,
    pub ascii: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            socket: None,
            tcp: None,
            cookie: None,
            timeout_seconds: 5,
            reconnect_seconds: 2,
            poll_seconds: 15,
            no_color: false,
            ascii: false,
        }
    }
}

pub struct Options {
    pub config: Config,
    pub diagnose: bool,
}

impl Config {
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_seconds)
    }

    pub fn validate(&self) -> Result<()> {
        if self.socket.is_some() && self.tcp.is_some() {
            bail!("Choose either --socket or --tcp");
        }
        if let Some(address) = self.tcp
            && (!address.ip().is_loopback() || address.port() == 0)
        {
            bail!("Control TCP address must be a numeric loopback address with a nonzero port");
        }
        if !(1..=60).contains(&self.timeout_seconds)
            || !(1..=60).contains(&self.reconnect_seconds)
            || !(5..=3600).contains(&self.poll_seconds)
        {
            bail!("Timeout/reconnect must be 1..60 seconds; poll must be 5..3600 seconds");
        }
        Ok(())
    }

    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Options>> {
        let program = std::env::args_os()
            .next()
            .and_then(|name| {
                std::path::Path::new(&name)
                    .file_stem()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .filter(|name| matches!(name.as_str(), "trm" | "tor-relay-monitor"))
            .unwrap_or_else(|| "trm".into());
        let args: Vec<String> = args.into_iter().collect();
        if args.iter().any(|a| a == "--help" || a == "-h") {
            println!(
                "{program} - Tor Relay Monitor\nUsage: {program} [--socket PATH | --tcp 127.0.0.1:9051]\n  --config PATH       Optional TOML file (no implicit config file)\n  --cookie PATH       Override cookie path advertised by Tor\n  --timeout SECONDS   Connection/command deadline (default 5)\n  --diagnose          Authenticate and print real metrics, then exit\n  --no-color          Disable colors (also honors NO_COLOR)\n  --ascii             Use ASCII borders and bandwidth bars\n  --version           Print application version\n\nKeys: q quit, 1 dashboard, 2 logs, 3 connections, h/? help, arrows/PgUp/PgDn scroll\n      p pause/resume; [ ] graph height, - + time window (30s..24h, saved)\n      / search on logs/OR; e severity, d repeats (logs); s sort, r reverse (OR)"
            );
            return Ok(None);
        }
        if args.iter().any(|a| a == "--version") {
            println!("{program} {}", env!("CARGO_PKG_VERSION"));
            return Ok(None);
        }
        let mut config = Config::default();
        let mut path = None;
        let mut scan = args.iter();
        while let Some(arg) = scan.next() {
            if arg == "--config" {
                path = Some(scan.next().context("--config requires a path")?);
            }
        }
        if let Some(path) = path {
            let contents =
                std::fs::read_to_string(path).context("Cannot read configuration file")?;
            config = toml::from_str(&contents).context("Invalid configuration file")?;
        }
        let mut diagnose = false;
        let mut scan = args.into_iter();
        while let Some(arg) = scan.next() {
            match arg.as_str() {
                "--config" => {
                    scan.next().context("--config requires a path")?;
                }
                "--socket" => {
                    config.socket = Some(scan.next().context("--socket requires a path")?.into());
                    config.tcp = None;
                }
                "--tcp" => {
                    config.tcp = Some(
                        scan.next()
                            .context("--tcp requires an address")?
                            .parse()
                            .context("Use a numeric IP:port")?,
                    );
                    config.socket = None;
                }
                "--cookie" => {
                    config.cookie = Some(scan.next().context("--cookie requires a path")?.into())
                }
                "--timeout" => {
                    config.timeout_seconds =
                        scan.next().context("--timeout requires seconds")?.parse()?
                }
                "--diagnose" => diagnose = true,
                "--no-color" => config.no_color = true,
                "--ascii" => config.ascii = true,
                _ => bail!("Unknown argument: {arg}; see --help"),
            }
        }
        config.no_color |= std::env::var_os("NO_COLOR").is_some();
        config.validate()?;
        Ok(Some(Options { config, diagnose }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_remote_control_and_bad_intervals() {
        let mut c = Config {
            tcp: Some("192.0.2.1:9051".parse().unwrap()),
            ..Config::default()
        };
        assert!(c.validate().is_err());
        c.tcp = Some("[::1]:9051".parse().unwrap());
        assert!(c.validate().is_ok());
        c.poll_seconds = 0;
        assert!(c.validate().is_err());
    }
}
