//! Scripted peers exercise wire behavior; production builds have no mock data path.
use hmac::Mac;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream},
    net::TcpListener,
    sync::watch,
    time::timeout,
};
use tor_relay_monitor::{
    auth::{self, CLIENT_KEY, SERVER_KEY},
    collector,
    config::Config,
    model::State,
    protocol::{Command, Connection, EVENT_CAPACITY, MAX_MESSAGE},
};

fn config(cookie: &std::path::Path) -> Config {
    Config {
        cookie: Some(cookie.to_path_buf()),
        timeout_seconds: 1,
        reconnect_seconds: 1,
        poll_seconds: 5,
        ..Config::default()
    }
}
fn cookie_file() -> tempfile::NamedTempFile {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&[42; 32]).unwrap();
    file
}

async fn handshake<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    peer: &mut BufReader<S>,
    bad_hash: bool,
) {
    let mut line = String::new();
    peer.read_line(&mut line).await.unwrap();
    assert_eq!(line, "PROTOCOLINFO 1\r\n");
    peer.write_all(b"250-PROTOCOLINFO 1\r\n250-AUTH METHODS=COOKIE,SAFECOOKIE COOKIEFILE=\"unused\"\r\n250-VERSION Tor=\"0.4.8-test\"\r\n250 OK\r\n").await.unwrap();
    line.clear();
    peer.read_line(&mut line).await.unwrap();
    let nonce = hex::decode(
        line.trim()
            .strip_prefix("AUTHCHALLENGE SAFECOOKIE ")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(nonce.len(), 32);
    let server = [7; 32];
    let hash = if bad_hash {
        vec![0; 32]
    } else {
        auth::proof(SERVER_KEY, &[42; 32], &nonce, &server)
            .unwrap()
            .finalize()
            .into_bytes()
            .to_vec()
    };
    peer.write_all(
        format!(
            "250 AUTHCHALLENGE SERVERNONCE={} SERVERHASH={} FUTURE=1\r\n",
            hex::encode(server),
            hex::encode(hash)
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    line.clear();
    if bad_hash {
        // Closing without AUTHENTICATE is mandatory after a bogus server proof.
        let read = timeout(Duration::from_secs(2), peer.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(read, 0);
        return;
    }
    peer.read_line(&mut line).await.unwrap();
    let expected = hex::encode(
        auth::proof(CLIENT_KEY, &[42; 32], &nonce, &server)
            .unwrap()
            .finalize()
            .into_bytes(),
    );
    assert_eq!(line, format!("AUTHENTICATE {expected}\r\n"));
    peer.write_all(b"250 OK\r\n").await.unwrap();
}

#[tokio::test]
async fn safecookie_handshake_and_rejected_server() {
    for bad_hash in [false, true] {
        let cookie = cookie_file();
        let (client, peer) = tokio::io::duplex(8192);
        let task = tokio::spawn(async move {
            handshake(&mut BufReader::new(peer), bad_hash).await;
        });
        let mut connection = Connection::new(Box::new(client), Duration::from_secs(1));
        let result = auth::authenticate(&mut connection, &config(cookie.path())).await;
        assert_eq!(result.is_err(), bad_hash);
        drop(connection);
        task.await.unwrap();
    }
}

#[tokio::test]
async fn rejects_short_cookie_and_unsafe_authentication() {
    for methods in ["NULL", "COOKIE", "HASHEDPASSWORD", "SAFECOOKIE"] {
        let cookie = tempfile::NamedTempFile::new().unwrap(); // zero bytes
        let (client, peer) = tokio::io::duplex(8192);
        let task = tokio::spawn(async move {
            let mut peer = BufReader::new(peer);
            let mut line = String::new();
            peer.read_line(&mut line).await.unwrap();
            peer.write_all(
                format!("250-AUTH METHODS={methods} COOKIEFILE=\"unused\"\r\n250 OK\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
            line.clear();
            assert_eq!(peer.read_line(&mut line).await.unwrap(), 0);
        });
        let mut connection = Connection::new(Box::new(client), Duration::from_secs(1));
        assert!(
            auth::authenticate(&mut connection, &config(cookie.path()))
                .await
                .is_err()
        );
        drop(connection);
        task.await.unwrap();
    }
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_world_readable_symlink_and_fifo_cookies() {
    use std::os::unix::{
        ffi::OsStrExt,
        fs::{PermissionsExt, symlink},
    };
    let directory = tempfile::tempdir().unwrap();
    let readable = directory.path().join("readable");
    std::fs::write(&readable, [42; 32]).unwrap();
    std::fs::set_permissions(&readable, std::fs::Permissions::from_mode(0o644)).unwrap();
    let link = directory.path().join("symlink");
    symlink(&readable, &link).unwrap();
    let fifo = directory.path().join("fifo");
    let fifo_c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // SAFETY: valid, NUL-terminated owned path; used only to create a private test FIFO.
    assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
    for path in [readable, link, fifo] {
        let (client, peer) = tokio::io::duplex(8192);
        let task = tokio::spawn(async move {
            let mut peer = BufReader::new(peer);
            let mut request = String::new();
            peer.read_line(&mut request).await.unwrap();
            assert_eq!(request, "PROTOCOLINFO 1\r\n");
            peer.write_all(b"250-AUTH METHODS=SAFECOOKIE\r\n250 OK\r\n")
                .await
                .unwrap();
            request.clear();
            assert_eq!(peer.read_line(&mut request).await.unwrap(), 0);
        });
        let mut connection = Connection::new(Box::new(client), Duration::from_secs(1));
        assert!(
            timeout(
                Duration::from_millis(250),
                auth::authenticate(&mut connection, &config(&path))
            )
            .await
            .unwrap()
            .is_err()
        );
        drop(connection);
        task.await.unwrap();
    }
}

#[tokio::test]
async fn events_interleaved_with_commands_and_unknown_extensions() {
    let (client, peer) = tokio::io::duplex(8192);
    let task = tokio::spawn(async move {
        let mut peer = BufReader::new(peer);
        let mut line = String::new();
        peer.read_line(&mut line).await.unwrap();
        assert_eq!(line, "GETINFO version\r\n");
        peer.write_all(b"650 BW 100 200 EXTRA=1\r\n250-version=0.4.9\r\n650-FUTURE_EVENT 1\r\n650 EXTRA=2\r\n250 OK\r\n").await.unwrap();
    });
    let mut connection = Connection::new(Box::new(client), Duration::from_secs(1));
    assert_eq!(
        connection
            .command(Command::GetInfo("version"))
            .await
            .unwrap()
            .value("version")
            .as_deref(),
        Some("0.4.9")
    );
    assert_eq!(connection.bandwidth.borrow().read, 100);
    assert!(connection.events.recv().await.unwrap().lines[0].starts_with("FUTURE_EVENT"));
    assert!(
        connection
            .command(Command::GetInfo("version"))
            .await
            .is_err()
    );
    task.await.unwrap();
}

#[tokio::test]
async fn command_deadline_and_incomplete_response() {
    let (client, mut peer) = tokio::io::duplex(8192);
    let task = tokio::spawn(async move {
        peer.write_all(b"250-partial\r\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    let mut c = Connection::new(Box::new(client), Duration::from_millis(20));
    assert!(
        c.command(Command::GetInfo("version"))
            .await
            .unwrap_err()
            .to_string()
            .contains("timed out")
    );
    drop(c);
    task.await.unwrap();
}

#[tokio::test]
async fn event_flood_is_bounded_and_bandwidth_bytes_are_preserved() {
    let started = std::time::Instant::now();
    let (client, peer): (DuplexStream, DuplexStream) = tokio::io::duplex(65536);
    let task = tokio::spawn(async move {
        let mut peer = BufReader::new(peer);
        let mut line = String::new();
        peer.read_line(&mut line).await.unwrap();
        for _ in 0..10_000 {
            peer.write_all(b"650 NOTICE bounded event\r\n650 BW 10 20\r\n")
                .await
                .unwrap();
        }
        peer.write_all(b"650 BW malformed 2\r\n250 version=real-test-peer\r\n")
            .await
            .unwrap();
    });
    let mut connection = Connection::new(Box::new(client), Duration::from_secs(10));
    assert!(
        connection
            .command(Command::GetInfo("version"))
            .await
            .is_ok()
    );
    let bw = *connection.bandwidth.borrow();
    assert_eq!(bw.samples, 10_000);
    assert_eq!(bw.read, 100_000);
    assert_eq!(bw.written, 200_000);
    assert_eq!(connection.events.len(), EVENT_CAPACITY);
    assert_eq!(
        connection.dropped.load(Ordering::Relaxed),
        10_000 - EVENT_CAPACITY as u64
    );
    assert_eq!(connection.malformed.load(Ordering::Relaxed), 1);
    task.await.unwrap();
    eprintln!(
        "20,000 valid events: {:?}; BW bytes retained; event queue bounded to {EVENT_CAPACITY}",
        started.elapsed()
    );
}

#[tokio::test]
async fn message_body_limit_is_enforced() {
    let line = "a".repeat(1024);
    let wire = format!(
        "250+test=\r\n{}",
        format!("{line}\r\n").repeat(MAX_MESSAGE / 1024 + 1)
    );
    assert!(
        tor_relay_monitor::protocol::Decoder::new(wire.as_bytes())
            .next()
            .await
            .is_err()
    );
}

async fn snapshot_peer(stream: tokio::net::TcpStream, visits: Arc<AtomicUsize>) {
    let mut peer = BufReader::new(stream);
    handshake(&mut peer, false).await;
    visits.fetch_add(1, Ordering::SeqCst);
    let mut line = String::new();
    loop {
        line.clear();
        if peer.read_line(&mut line).await.unwrap_or(0) == 0 {
            return;
        }
        let request = line.trim();
        let reply = if request == "GETINFO events/names" {
            "250 events/names=BW NOTICE WARN ERR\r\n".to_string()
        } else if request.starts_with("SETEVENTS ") {
            "250 OK\r\n650 WARN scripted test warning\r\n650 BW 1234 5678\r\n".to_string()
        } else if request == "GETINFO version" {
            "250 version=test-instance\r\n".to_string()
        } else if request == "GETINFO orconn-status" {
            "250+orconn-status=\r\n$ABC=known CONNECTED\r\n.\r\n250 OK\r\n".to_string()
        } else if request == "GETCONF Nickname" {
            "250 Nickname=TestRelay\r\n".to_string()
        } else {
            "552 Unavailable\r\n".to_string()
        };
        if peer.write_all(reply.as_bytes()).await.is_err() {
            return;
        }
        if request == "GETCONF AccountingStart" {
            tokio::time::sleep(Duration::from_millis(400)).await;
            return; // a restart/disconnect after the full initial snapshot
        }
    }
}

#[tokio::test]
async fn reconnects_and_cancels_without_waiting_for_retry() {
    let cookie = cookie_file();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = config(cookie.path());
    config.tcp = Some(listener.local_addr().unwrap());
    let visits = Arc::new(AtomicUsize::new(0));
    let count = visits.clone();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            snapshot_peer(stream, count.clone()).await;
        }
    });
    let (sender, mut snapshots) = watch::channel(Arc::new(State::default()));
    let (stop, stopping) = watch::channel(false);
    let collector = tokio::spawn(collector::run(config, sender, stopping));
    timeout(Duration::from_secs(8), async {
        loop {
            snapshots.changed().await.unwrap();
            let s = snapshots.borrow().clone();
            if visits.load(Ordering::SeqCst) >= 2
                && s.connected
                && s.metric("Nickname") == "TestRelay"
                && s.session_read >= 2468
                && s.session_written >= 11356
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    stop.send(true).unwrap();
    timeout(Duration::from_millis(250), collector)
        .await
        .unwrap()
        .unwrap();
    server.await.unwrap();
}

#[tokio::test]
#[ignore = "Requires an explicitly selected, isolated local Tor with SAFECOOKIE"]
async fn real_local_tor() {
    let mut config = Config::default();
    if let Ok(address) = std::env::var("TRM_TEST_TCP") {
        config.tcp = Some(address.parse().unwrap());
    } else {
        config.socket = Some(
            std::env::var_os("TRM_TEST_SOCKET")
                .expect("Set TRM_TEST_SOCKET or TRM_TEST_TCP")
                .into(),
        );
    }
    config.cookie = std::env::var_os("TRM_TEST_COOKIE").map(Into::into);
    let state = collector::diagnose(&config).await.unwrap();
    assert!(state.connected);
    assert_ne!(state.metric("version"), "N/A");
    assert!(state.number("uptime").is_some());
    assert!(state.rates().is_some());
    #[cfg(target_os = "linux")]
    if std::env::var_os("TRM_TEST_RESOURCES").is_some() {
        assert!(state.resources.rss.is_some_and(|bytes| bytes > 0));
        assert!(state.resources.cpu.is_some());
        // Tor's default process protection can hide /proc/PID/fd from a
        // non-root test runner, even when other resource files are readable.
        assert!(state.resources.descriptors.is_some() || !state.resources.reason.is_empty());
        assert!(state.resources.descriptor_limit.is_some());
    }
    for key in ["address/v4", "address/v6", "status/version/current"] {
        assert!(state.metrics.contains_key(key));
    }
    if std::env::var_os("TRM_TEST_ACCOUNTING").is_some() {
        assert_eq!(state.number("accounting/enabled"), Some(1));
        assert!(state.pair("accounting/bytes").is_some());
        assert!(state.pair("accounting/bytes-left").is_some());
        assert!(
            state
                .accounting_reset(std::time::SystemTime::now())
                .is_some()
        );
    }
}
