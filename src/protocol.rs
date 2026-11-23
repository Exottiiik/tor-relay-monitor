//! Bounded Tor control framing, one reader, and allowlisted commands.
use anyhow::{Context, Result, bail};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{
        AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, ReadHalf,
        WriteHalf,
    },
    sync::{mpsc, watch},
    task::JoinHandle,
    time::timeout,
};

pub const MAX_LINE: usize = 16 * 1024;
pub const MAX_MESSAGE: usize = 2 * 1024 * 1024;
pub const EVENT_CAPACITY: usize = 256;

pub trait Transport: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Transport for T {}
pub type Stream = Box<dyn Transport>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub code: u16,
    pub lines: Vec<String>,
}
impl Message {
    pub fn value(&self, key: &str) -> Option<String> {
        self.lines.iter().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            name.eq_ignore_ascii_case(key).then(|| value.to_string())
        })
    }
    pub fn success(self) -> Result<Self> {
        // Never echo raw replies here: an authentication peer can reflect secrets.
        if self.code != 250 {
            bail!("Tor control request failed (status {})", self.code);
        }
        Ok(self)
    }
}

/// Reads without unbounded read_line allocation. A canceled read is only used when closing.
pub async fn read_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<String> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader
            .fill_buf()
            .await
            .context("Control socket read failed")?;
        if buffer.is_empty() {
            bail!("Tor disconnected during a control message");
        }
        let count = buffer
            .iter()
            .position(|b| *b == b'\n')
            .map_or(buffer.len(), |n| n + 1);
        if bytes.len() + count > MAX_LINE {
            bail!("Control line exceeds 16 KiB limit");
        }
        let done = buffer[count - 1] == b'\n';
        bytes.extend_from_slice(&buffer[..count]);
        reader.consume(count);
        if done {
            break;
        }
    }
    if !bytes.ends_with(b"\r\n") {
        bail!("Control line is missing CRLF");
    }
    bytes.truncate(bytes.len() - 2);
    String::from_utf8(bytes).context("Control line is not UTF-8")
}

/// Separates asynchronous frames even if a peer inserts them between reply lines.
pub struct Decoder<R> {
    reader: R,
    pending: Option<Message>,
    pending_bytes: usize,
}
impl<R: AsyncBufRead + Unpin> Decoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            pending: None,
            pending_bytes: 0,
        }
    }
    pub async fn next(&mut self) -> Result<Message> {
        let mut event: Option<Message> = None;
        let mut event_bytes = 0;
        loop {
            let line = read_line(&mut self.reader).await?;
            let bytes = line.as_bytes();
            if bytes.len() < 4
                || !bytes[..3].iter().all(u8::is_ascii_digit)
                || !matches!(bytes[3], b' ' | b'-' | b'+')
            {
                bail!("Malformed control reply header");
            }
            let code: u16 = line[..3].parse()?;
            if !(200..700).contains(&code) {
                bail!("Invalid control reply status");
            }
            let (slot, size) = if code == 650 {
                (&mut event, &mut event_bytes)
            } else {
                (&mut self.pending, &mut self.pending_bytes)
            };
            let message = slot.get_or_insert_with(|| Message {
                code,
                lines: Vec::new(),
            });
            if message.code != code {
                bail!("Inconsistent control continuation status");
            }
            let mut body = line[4..].to_string();
            *size += line.len() + 2;
            if *size > MAX_MESSAGE {
                bail!("Control message exceeds 2 MiB limit");
            }
            if bytes[3] == b'+' {
                loop {
                    let data = read_line(&mut self.reader).await?;
                    *size += data.len() + 2;
                    if *size > MAX_MESSAGE {
                        bail!("Control message exceeds 2 MiB limit");
                    }
                    if data == "." {
                        break;
                    }
                    if data.starts_with('.') && !data.starts_with("..") {
                        bail!("Invalid dot-stuffed data line");
                    }
                    body.push('\n');
                    body.push_str(
                        data.strip_prefix("..")
                            .map_or(data.as_str(), |_| &data[1..]),
                    );
                }
            }
            message.lines.push(body);
            if bytes[3] == b' ' {
                let result = slot.take().context("Missing control frame")?;
                *size = 0;
                return Ok(result);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BandwidthAggregate {
    pub samples: u64,
    pub read: u64,
    pub written: u64,
}

/// The only wire commands supported by the monitor.
pub enum Command<'a> {
    ProtocolInfo,
    Challenge(&'a str),
    Authenticate(&'a str),
    GetInfo(&'a str),
    GetConf(&'a str),
    Events(&'a str),
}
impl Command<'_> {
    fn encode(&self) -> Result<String> {
        let (verb, argument) = match self {
            Self::ProtocolInfo => ("PROTOCOLINFO", "1"),
            Self::Challenge(nonce) => ("AUTHCHALLENGE SAFECOOKIE", *nonce),
            Self::Authenticate(proof) => ("AUTHENTICATE", *proof),
            Self::GetInfo(key) => ("GETINFO", *key),
            Self::GetConf(key) => ("GETCONF", *key),
            Self::Events(events) => ("SETEVENTS", *events),
        };
        if argument.is_empty()
            || argument.len() > 1024
            || !argument
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b" /_-".contains(&b))
        {
            bail!("Invalid control command argument");
        }
        Ok(format!("{verb} {argument}\r\n"))
    }
}

pub struct Connection {
    writer: WriteHalf<Stream>,
    replies: mpsc::Receiver<Result<Message>>,
    pub events: mpsc::Receiver<Message>,
    pub bandwidth: watch::Receiver<BandwidthAggregate>,
    pub dropped: Arc<AtomicU64>,
    pub malformed: Arc<AtomicU64>,
    reader: JoinHandle<()>,
    deadline: Duration,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.reader.abort();
    }
}
impl Connection {
    pub fn new(stream: Stream, deadline: Duration) -> Self {
        let (reader, writer) = tokio::io::split(stream);
        let (reply_tx, replies) = mpsc::channel(1);
        let (event_tx, events) = mpsc::channel(EVENT_CAPACITY);
        let (bw_tx, bandwidth) = watch::channel(BandwidthAggregate::default());
        let dropped = Arc::new(AtomicU64::new(0));
        let malformed = Arc::new(AtomicU64::new(0));
        let reader = tokio::spawn(read_frames(
            reader,
            reply_tx,
            event_tx,
            bw_tx,
            dropped.clone(),
            malformed.clone(),
        ));
        Self {
            writer,
            replies,
            events,
            bandwidth,
            dropped,
            malformed,
            reader,
            deadline,
        }
    }
    pub async fn command(&mut self, command: Command<'_>) -> Result<Message> {
        use zeroize::Zeroizing;
        let wire = Zeroizing::new(command.encode()?);
        timeout(self.deadline, async {
            self.writer
                .write_all(wire.as_bytes())
                .await
                .context("Control write failed")?;
            self.writer.flush().await?;
            self.replies
                .recv()
                .await
                .context("Tor control reader stopped")?
        })
        .await
        .context("Tor control command timed out; connection must be reopened")?
    }
    pub async fn ended(&mut self) -> Result<()> {
        match self.replies.recv().await {
            Some(Err(e)) => Err(e),
            _ => bail!("Unexpected control response or disconnected reader"),
        }
    }
}

async fn read_frames(
    reader: ReadHalf<Stream>,
    replies: mpsc::Sender<Result<Message>>,
    events: mpsc::Sender<Message>,
    bandwidth: watch::Sender<BandwidthAggregate>,
    dropped: Arc<AtomicU64>,
    malformed: Arc<AtomicU64>,
) {
    let mut decoder = Decoder::new(BufReader::new(reader));
    loop {
        match decoder.next().await {
            Ok(message) if message.code == 650 => {
                let first = message.lines.first().map(String::as_str).unwrap_or("");
                if first.split_whitespace().next() == Some("BW") {
                    let mut fields = first.split_whitespace().skip(1);
                    let values = fields
                        .next()
                        .and_then(|v| v.parse::<u64>().ok())
                        .zip(fields.next().and_then(|v| v.parse::<u64>().ok()));
                    if let Some((read, written)) = values {
                        bandwidth.send_modify(|b| {
                            b.samples = b.samples.saturating_add(1);
                            b.read = b.read.saturating_add(read);
                            b.written = b.written.saturating_add(written);
                        });
                    } else {
                        malformed.fetch_add(1, Ordering::Relaxed);
                    }
                } else if message.lines.iter().map(String::len).sum::<usize>() > 4096
                    || events.try_send(message).is_err()
                {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
            Ok(message) => {
                if replies.send(Ok(message)).await.is_err() {
                    break;
                }
            }
            Err(error) => {
                let _ = replies.send(Err(error)).await;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn simple_multiline_data_and_events() {
        let wire = b"250-version=0.4.8\r\n650 BW 10 20 FUTURE=1\r\n250+test=\r\n..dot\r\ntext\r\n.\r\n250 OK\r\n650-NEW_EVENT x\r\n650 EXTRA=1\r\n";
        let mut d = Decoder::new(&wire[..]);
        assert_eq!(d.next().await.unwrap().lines, ["BW 10 20 FUTURE=1"]);
        let reply = d.next().await.unwrap();
        assert_eq!(reply.value("version").as_deref(), Some("0.4.8"));
        assert_eq!(reply.value("test").as_deref(), Some("\n.dot\ntext"));
        assert_eq!(d.next().await.unwrap().lines.len(), 2);
        assert!(d.next().await.is_err());
    }
    #[tokio::test]
    async fn malformed_incomplete_and_oversized_frames_fail() {
        for wire in [
            "250 OK\n",
            "x50 OK\r\n",
            "250-hello\r\n",
            "250+data=\r\nunfinished\r\n",
            "250-a\r\n251 OK\r\n",
            "250+data=\r\n.bad\r\n",
        ] {
            assert!(
                Decoder::new(wire.as_bytes()).next().await.is_err(),
                "{wire}"
            );
        }
        let wire = format!("250 {}\r\n", "a".repeat(MAX_LINE));
        assert!(Decoder::new(wire.as_bytes()).next().await.is_err());
    }
    #[test]
    fn command_injection_is_rejected() {
        assert!(Command::GetInfo("version\r\nSIGNAL HALT").encode().is_err());
        assert!(Command::GetInfo("ns/id/ABC").encode().is_ok());
    }
}
