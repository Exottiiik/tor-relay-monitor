//! One control session, bounded event consumption, periodic snapshots, reconnects.
use crate::{
    auth,
    config::Config,
    model::State,
    protocol::{BandwidthAggregate, Command, Connection, Stream},
    resources,
};
use anyhow::{Context, Result, bail};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::{
    net::TcpStream,
    sync::watch,
    time::{MissedTickBehavior, interval, sleep, timeout},
};

pub async fn connect(config: &Config) -> Result<Connection> {
    config.validate()?;
    let stream: Stream = if let Some(address) = config.tcp {
        Box::new(
            timeout(config.timeout(), TcpStream::connect(address))
                .await
                .context("Control TCP connection timed out")?
                .context("Cannot connect to local ControlPort")?,
        )
    } else {
        #[cfg(unix)]
        {
            use std::{os::unix::fs::FileTypeExt, path::PathBuf};
            use tokio::net::UnixStream;
            let path = if let Some(path) = &config.socket {
                path.clone()
            } else {
                let candidates = [
                    "/run/tor/control",
                    "/run/tor/control.sock",
                    "/var/run/tor/control",
                    "/var/run/tor/control.sock",
                ];
                candidates.iter().find(|p| std::fs::metadata(p).is_ok_and(|m| m.file_type().is_socket()))
                        .map(PathBuf::from).context("No known Tor ControlSocket found; specify --socket PATH or --tcp 127.0.0.1:PORT")?
            };
            Box::new(
                timeout(config.timeout(), UnixStream::connect(&path))
                    .await
                    .context("ControlSocket connection timed out")?
                    .with_context(|| {
                        format!(
                            "Cannot connect to {}; check socket and directory permissions",
                            path.display()
                        )
                    })?,
            )
        }
        #[cfg(not(unix))]
        {
            bail!(
                "Unix ControlSocket requires Linux/Unix; use --tcp with a local test Tor on this platform"
            );
        }
    };
    let mut connection = Connection::new(stream, config.timeout());
    timeout(
        config.timeout() * 3,
        auth::authenticate(&mut connection, config),
    )
    .await
    .context("SAFECOOKIE authentication timed out")??;
    Ok(connection)
}

async fn query(
    connection: &mut Connection,
    state: &mut State,
    key: &str,
    conf: bool,
) -> Result<Option<String>> {
    let command = if conf {
        Command::GetConf(key)
    } else {
        Command::GetInfo(key)
    };
    let reply = connection.command(command).await?;
    let (mut value, mut reason) = if reply.code == 250 {
        let value = reply.value(key);
        let reason = if value.is_some() {
            "".to_string()
        } else {
            "Tor returned no value".to_string()
        };
        (value, reason)
    } else if matches!(reply.code, 510 | 511 | 551 | 552) {
        (
            None,
            format!(
                "Tor did not provide this key (status {}); unsupported, unavailable or absent from cache",
                reply.code
            ),
        )
    } else {
        bail!("Tor metric request failed (status {})", reply.code);
    };
    if matches!(
        key,
        "process/pid"
            | "uptime"
            | "traffic/read"
            | "traffic/written"
            | "BandwidthRate"
            | "BandwidthBurst"
            | "RelayBandwidthRate"
            | "RelayBandwidthBurst"
            | "accounting/enabled"
            | "AccountingMax"
    ) && value.as_ref().is_some_and(|v| v.parse::<u64>().is_err())
    {
        value = None;
        reason = "Tor returned an invalid unsigned numeric metric".to_string();
    }
    if matches!(key, "accounting/bytes" | "accounting/bytes-left")
        && value.as_ref().is_some_and(|v| {
            let fields: Vec<_> = v.split_whitespace().collect();
            fields.len() != 2 || fields.iter().any(|field| field.parse::<u64>().is_err())
        })
    {
        value = None;
        reason = "Tor returned invalid accounting byte counters".to_string();
    }
    if matches!(key, "accounting/interval-start" | "accounting/interval-end")
        && value
            .as_ref()
            .is_some_and(|v| crate::model::parse_utc(v).is_none())
    {
        value = None;
        reason = "Tor returned an invalid UTC accounting timestamp".to_string();
    }
    if key == "fingerprint"
        && value
            .as_ref()
            .is_some_and(|v| v.len() != 40 || !v.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        value = None;
        reason = "Tor returned an invalid relay fingerprint".to_string();
    }
    if matches!(key, "address/v4" | "address/v6")
        && value
            .as_ref()
            .is_some_and(|v| match v.parse::<std::net::IpAddr>() {
                Ok(address) => {
                    address.is_unspecified() || (key == "address/v4") != address.is_ipv4()
                }
                Err(_) => true,
            })
    {
        value = None;
        reason = "Tor returned an invalid or unspecified IP address".to_string();
    }
    // Large snapshots are parsed immediately, never retained in the metric cache.
    if key == "orconn-status" {
        state.or_connections(value.as_deref());
    }
    if key.starts_with("desc/id/") {
        state.descriptor(value.as_deref());
    } else if key.starts_with("ns/id/") {
        state.consensus(value.as_deref());
    } else if key != "orconn-status" {
        state.set(key, value.clone(), &reason);
    }
    if key == "orconn-status" {
        state.set(
            "orconn-source",
            value.as_ref().map(|_| "Available".to_string()),
            &reason,
        );
    }
    Ok(value)
}

fn publish(state: &State, snapshots: Option<&watch::Sender<Arc<State>>>) {
    if let Some(sender) = snapshots {
        sender.send_replace(Arc::new(state.clone()));
    }
}

fn collect_reader_counts(connection: &Connection, state: &mut State, reported: &mut (u64, u64)) {
    let current = (
        connection.dropped.load(Ordering::Relaxed),
        connection.malformed.load(Ordering::Relaxed),
    );
    state.dropped_events = state
        .dropped_events
        .saturating_add(current.0.saturating_sub(reported.0));
    state.malformed_events = state
        .malformed_events
        .saturating_add(current.1.saturating_sub(reported.1));
    *reported = current;
}

pub async fn refresh(
    connection: &mut Connection,
    state: &mut State,
    snapshots: Option<&watch::Sender<Arc<State>>>,
) -> Result<()> {
    for key in [
        "version",
        "fingerprint",
        "process/pid",
        "uptime",
        "address/v4",
        "address/v6",
        "status/version/current",
        "traffic/read",
        "traffic/written",
        "status/bootstrap-phase",
        "status/circuit-established",
        "network-liveness",
        "accounting/hibernating",
        "accounting/enabled",
        "orconn-status",
    ] {
        let previous_pid = state.number("process/pid");
        query(connection, state, key, false).await?;
        if key == "process/pid" && previous_pid != state.number("process/pid") {
            state.resources = resources::Resources::default();
        }
        publish(state, snapshots);
    }
    if state.number("accounting/enabled") == Some(1) {
        for key in [
            "accounting/bytes",
            "accounting/bytes-left",
            "accounting/interval-start",
            "accounting/interval-end",
        ] {
            query(connection, state, key, false).await?;
        }
    } else {
        for key in [
            "accounting/bytes",
            "accounting/bytes-left",
            "accounting/interval-start",
            "accounting/interval-end",
        ] {
            state.set(key, None, "Tor accounting is disabled or unavailable");
        }
    }
    for key in [
        "Nickname",
        "BandwidthRate",
        "BandwidthBurst",
        "RelayBandwidthRate",
        "RelayBandwidthBurst",
        "ORPort",
        "AccountingMax",
        "AccountingRule",
        "AccountingStart",
    ] {
        query(connection, state, key, true).await?;
        publish(state, snapshots);
    }
    let fingerprint = state.metric("fingerprint").to_string();
    if fingerprint.len() == 40 && fingerprint.bytes().all(|c| c.is_ascii_hexdigit()) {
        for prefix in ["desc/id/", "ns/id/"] {
            query(connection, state, &format!("{prefix}{fingerprint}"), false).await?;
            publish(state, snapshots);
        }
    } else {
        state.descriptor(None);
        state.consensus(None);
    }
    Ok(())
}

async fn subscribe(connection: &mut Connection, state: &mut State) -> Result<()> {
    let names = connection
        .command(Command::GetInfo("events/names"))
        .await?
        .success()?
        .value("events/names")
        .context("Tor did not report supported events")?;
    let supported: Vec<_> = names.split_whitespace().collect();
    if !supported.contains(&"BW") {
        bail!("This Tor does not support BW events");
    }
    let wanted = ["BW", "NOTICE", "WARN", "ERR"];
    let selected: Vec<_> = wanted
        .iter()
        .filter(|e| supported.contains(e))
        .copied()
        .collect();
    for missing in wanted.iter().filter(|e| !supported.contains(e)) {
        state.log("WARN", &format!("Tor event {missing} unavailable"));
    }
    connection
        .command(Command::Events(&selected.join(" ")))
        .await?
        .success()?;
    Ok(())
}

async fn session(
    config: &Config,
    state: &mut State,
    snapshots: &watch::Sender<Arc<State>>,
) -> Result<()> {
    let mut connection = connect(config).await?;
    // Reload relay identity/process counters; monitor-run BW totals remain cumulative.
    state.metrics.clear();
    state.history.clear();
    state.resources = resources::Resources::default();
    state.or_connections(None);
    state.connected = true;
    state.status = "Connected (SAFECOOKIE)".to_string();
    state.log(
        "NOTICE",
        "Authenticated to local Tor; collecting read-only metrics",
    );
    publish(state, Some(snapshots));
    let mut reported = (0, 0);
    let mut previous = BandwidthAggregate::default();
    let result = async {
        subscribe(&mut connection, state).await?;
        refresh(&mut connection, state, Some(snapshots)).await?;
        let mut cpu_previous = None;
        let mut resource_tick = interval(Duration::from_secs(5));
        resource_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut poll = interval(Duration::from_secs(config.poll_seconds));
        poll.set_missed_tick_behavior(MissedTickBehavior::Skip);
        poll.tick().await;
        let mut tick = interval(Duration::from_millis(250));
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = resource_tick.tick() => {
                    let pid = state.number("process/pid").and_then(|v| u32::try_from(v).ok());
                    let (sample, previous) = resources::sample(pid, cpu_previous).await;
                    cpu_previous = previous;
                    state.resources = sample;
                    publish(state, Some(snapshots));
                }
                _ = tick.tick() => {
                    while let Ok(event) = connection.events.try_recv() { state.event(event); }
                    state.bandwidth(&mut previous, *connection.bandwidth.borrow_and_update());
                    collect_reader_counts(&connection, state, &mut reported);
                    publish(state, Some(snapshots));
                }
                _ = poll.tick() => refresh(&mut connection, state, Some(snapshots)).await?,
                ended = connection.ended() => return ended,
            }
        }
    }
    .await;
    // Keep bytes received immediately before a disconnect, even between UI ticks.
    state.bandwidth(&mut previous, *connection.bandwidth.borrow_and_update());
    collect_reader_counts(&connection, state, &mut reported);
    result
}

pub async fn run(
    config: Config,
    snapshots: watch::Sender<Arc<State>>,
    mut stop: watch::Receiver<bool>,
) {
    let mut state = State {
        status: "Connecting to local Tor".to_string(),
        ..State::default()
    };
    let mut delay = config.reconnect_seconds;
    loop {
        if *stop.borrow() {
            return;
        }
        let result = tokio::select! {
            result = session(&config, &mut state, &snapshots) => result,
            _ = stop.changed() => return,
        };
        let was_connected = state.connected;
        state.connected = false;
        let message = match result {
            Ok(()) => "Control session ended".to_string(),
            Err(e) => format!("{e:#}"),
        };
        state.status = format!("Disconnected: {message}. Retrying in {delay}s");
        state.log("WARN", &message);
        publish(&state, Some(&snapshots));
        tokio::select! { _ = sleep(Duration::from_secs(delay)) => {}, _ = stop.changed() => return }
        delay = if was_connected {
            config.reconnect_seconds
        } else {
            (delay * 2).min(30)
        };
    }
}

pub async fn diagnose(config: &Config) -> Result<State> {
    let mut connection = connect(config).await?;
    let mut state = State {
        connected: true,
        status: "Connected (SAFECOOKIE)".to_string(),
        ..State::default()
    };
    subscribe(&mut connection, &mut state).await?;
    refresh(&mut connection, &mut state, None).await?;
    let pid = state
        .number("process/pid")
        .and_then(|v| u32::try_from(v).ok());
    let (_, cpu_previous) = resources::sample(pid, None).await;
    let mut previous = BandwidthAggregate::default();
    // A live event is required; a cached or fabricated rate is never substituted.
    timeout(config.timeout(), connection.bandwidth.changed())
        .await
        .context("Authenticated but no BW event arrived before the diagnostic deadline")?
        .context("Tor disconnected before a BW event")?;
    state.bandwidth(&mut previous, *connection.bandwidth.borrow_and_update());
    state.resources = resources::sample(pid, cpu_previous).await.0;
    while let Ok(event) = connection.events.try_recv() {
        state.event(event);
    }
    collect_reader_counts(&connection, &mut state, &mut (0, 0));
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn parser_and_reader_error_counts_remain_cumulative() {
        let (client, _peer) = tokio::io::duplex(64);
        let connection = Connection::new(Box::new(client), Duration::from_secs(1));
        let mut state = State::default();
        let mut reported = (0, 0);
        connection.malformed.store(2, Ordering::Relaxed);
        collect_reader_counts(&connection, &mut state, &mut reported);
        state.or_connections(Some("malformed-row"));
        connection.malformed.store(3, Ordering::Relaxed);
        collect_reader_counts(&connection, &mut state, &mut reported);
        collect_reader_counts(&connection, &mut state, &mut reported);
        assert_eq!(state.malformed_events, 4);
    }
    #[tokio::test]
    async fn address_queries_reject_invalid_unspecified_and_wrong_family_values() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        for (key, value, valid) in [
            ("address/v4", "192.0.2.1", true),
            ("address/v6", "2001:db8::1", true),
            ("address/v4", "0.0.0.0", false),
            ("address/v6", "::", false),
            ("address/v4", "2001:db8::1", false),
            ("address/v6", "192.0.2.1", false),
            ("address/v6", "malformed", false),
        ] {
            let (client, peer) = tokio::io::duplex(4096);
            let server = tokio::spawn(async move {
                let mut peer = BufReader::new(peer);
                let mut command = String::new();
                peer.read_line(&mut command).await.unwrap();
                assert_eq!(command, format!("GETINFO {key}\r\n"));
                peer.write_all(format!("250 {key}={value}\r\n").as_bytes())
                    .await
                    .unwrap();
            });
            let mut connection = Connection::new(Box::new(client), Duration::from_secs(1));
            let mut state = State::default();
            query(&mut connection, &mut state, key, false)
                .await
                .unwrap();
            assert_eq!(state.metrics[key].value.is_some(), valid);
            if !valid {
                assert_eq!(state.address(key == "address/v6"), "not detected");
            }
            server.await.unwrap();
        }
    }
}
