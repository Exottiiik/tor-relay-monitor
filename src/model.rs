//! Bounded domain state and explicit metric availability.
use crate::protocol::{BandwidthAggregate, Message};
use std::{
    collections::{BTreeMap, VecDeque},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub const HISTORY_LIMIT: usize = 120;
pub const LOG_LIMIT: usize = 500;
pub const CONNECTION_LIMIT: usize = 2048;

#[derive(Clone, Debug)]
pub struct Metric {
    pub value: Option<String>,
    pub reason: String,
    pub updated: Instant,
}
impl Metric {
    pub fn display(&self) -> &str {
        self.value.as_deref().unwrap_or("N/A")
    }
}
#[derive(Clone, Debug)]
pub struct LogEntry {
    pub seconds: u64,
    pub level: String,
    pub message: String,
    pub received: Instant,
}
#[derive(Clone, Debug)]
pub struct Sample {
    pub read: u64,
    pub written: u64,
    pub seconds: u64,
    pub received: Instant,
}
#[derive(Clone, Debug, Default)]
pub struct State {
    pub connected: bool,
    pub status: String,
    pub metrics: BTreeMap<String, Metric>,
    pub history: VecDeque<Sample>,
    pub long_history: crate::history::History,
    pub resources: crate::resources::Resources,
    pub logs: VecDeque<LogEntry>,
    pub connections: Vec<(String, String)>,
    pub connection_count: Option<usize>,
    pub connected_count: Option<usize>,
    pub omitted_connections: usize,
    pub dropped_events: u64,
    pub malformed_events: u64,
    pub aggregated_samples: u64,
    /// Sum of received BW event bytes during this monitor run, across reconnects.
    pub session_read: u64,
    pub session_written: u64,
}

/// Strip terminal control characters, including ESC and C1, from untrusted labels.
pub fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(2048)
        .collect()
}

impl State {
    /// Prefer the latest own descriptor; fall back to the cached consensus.
    pub fn published_or_port(&self, ipv6: bool) -> Option<u16> {
        let keys = if ipv6 {
            ["descriptor-orport-v6", "consensus-orport-v6"]
        } else {
            ["descriptor-orport", "consensus-orport"]
        };
        keys.iter().find_map(|key| {
            self.number(key)
                .and_then(|value| u16::try_from(value).ok())
                .filter(|port| *port != 0)
        })
    }
    pub fn port_label(&self) -> String {
        self.published_or_port(false)
            .or_else(|| self.published_or_port(true))
            .map_or_else(|| "not detected".into(), |port| port.to_string())
    }
    pub fn address(&self, ipv6: bool) -> &str {
        let key = if ipv6 { "address/v6" } else { "address/v4" };
        self.metrics
            .get(key)
            .and_then(|metric| metric.value.as_deref())
            .unwrap_or("not detected")
    }
    pub fn pair(&self, key: &str) -> Option<(u64, u64)> {
        let mut fields = self.metrics.get(key)?.value.as_ref()?.split_whitespace();
        let pair = (fields.next()?.parse().ok()?, fields.next()?.parse().ok()?);
        fields.next().is_none().then_some(pair)
    }
    pub fn accounting_reset(&self, now: SystemTime) -> Option<u64> {
        if !self.connected || self.number("accounting/enabled") != Some(1) {
            return None;
        }
        let end = parse_utc(
            self.metrics
                .get("accounting/interval-end")?
                .value
                .as_deref()?,
        )?;
        let now = now.duration_since(UNIX_EPOCH).ok()?.as_secs();
        Some(end.saturating_sub(now))
    }
    pub fn metric(&self, key: &str) -> &str {
        self.metrics.get(key).map_or("N/A", Metric::display)
    }
    pub fn number(&self, key: &str) -> Option<u64> {
        self.metrics.get(key)?.value.as_ref()?.parse().ok()
    }
    pub fn set(&mut self, key: &str, value: Option<String>, reason: &str) {
        self.metrics.insert(
            key.to_string(),
            Metric {
                value: value.map(|v| clean(v.trim())),
                reason: reason.to_string(),
                updated: Instant::now(),
            },
        );
    }
    pub fn log(&mut self, level: &str, message: &str) {
        if self.logs.len() == LOG_LIMIT {
            self.logs.pop_front();
        }
        self.logs.push_back(LogEntry {
            seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            level: clean(level),
            message: clean(message),
            received: Instant::now(),
        });
    }
    pub fn event(&mut self, event: Message) {
        if let Some(line) = event.lines.first()
            && let Some((level, message)) = line.split_once(' ')
            && matches!(level, "NOTICE" | "WARN" | "ERR")
        {
            self.log(level, message);
        }
    }
    pub fn bandwidth(&mut self, previous: &mut BandwidthAggregate, current: BandwidthAggregate) {
        let count = current.samples.saturating_sub(previous.samples);
        if count == 0 {
            return;
        }
        if count > 1 {
            self.aggregated_samples = self.aggregated_samples.saturating_add(count);
        }
        let read = current.read.saturating_sub(previous.read);
        let written = current.written.saturating_sub(previous.written);
        self.session_read = self.session_read.saturating_add(read);
        self.session_written = self.session_written.saturating_add(written);
        let sample = Sample {
            read,
            written,
            seconds: count,
            received: Instant::now(),
        };
        self.long_history.push(&sample);
        self.history.push_back(sample);
        let mut seconds: u64 = self.history.iter().map(|s| s.seconds).sum();
        while self.history.len() > 1
            && (seconds > HISTORY_LIMIT as u64 || self.history.len() > HISTORY_LIMIT)
        {
            if let Some(sample) = self.history.pop_front() {
                seconds = seconds.saturating_sub(sample.seconds);
            }
        }
        *previous = current;
    }
    pub fn rates(&self) -> Option<(u64, u64)> {
        self.rates_at(Instant::now())
    }
    pub fn rates_at(&self, now: Instant) -> Option<(u64, u64)> {
        let sample = self.history.back()?;
        if !self.connected
            || sample.seconds == 0
            || now.saturating_duration_since(sample.received).as_secs() > 3
        {
            return None;
        }
        Some((
            sample.read / sample.seconds,
            sample.written / sample.seconds,
        ))
    }
    pub fn mean_rates(&self) -> Option<(u64, u64)> {
        self.mean_rates_for(HISTORY_LIMIT as u64)
    }
    pub fn mean_rates_for(&self, window: u64) -> Option<(u64, u64)> {
        self.mean_rates_at(window, Instant::now())
    }
    pub fn mean_rates_at(&self, window: u64, now: Instant) -> Option<(u64, u64)> {
        self.rates_at(now)?;
        if window > HISTORY_LIMIT as u64 {
            let mut read = 0.0;
            let mut written = 0.0;
            let mut covered = 0.0;
            for b in &self.long_history.buckets {
                let age = now.saturating_duration_since(b.end).as_secs_f64();
                let span = b.end.saturating_duration_since(b.start).as_secs_f64();
                if span == 0.0 {
                    continue;
                }
                let fraction = ((window as f64 - age) / span).clamp(0.0, 1.0);
                read += b.read * fraction;
                written += b.written * fraction;
                covered += b.covered * fraction;
            }
            return (covered > 0.0).then(|| ((read / covered) as u64, (written / covered) as u64));
        }
        let (r, w, seconds) = self
            .history
            .iter()
            .filter(|s| {
                s.seconds > 0 && now.saturating_duration_since(s.received).as_secs() < window
            })
            .fold((0u128, 0u128, 0u128), |(r, w, t), s| {
                let included = s.seconds.min(
                    window.saturating_sub(now.saturating_duration_since(s.received).as_secs()),
                );
                (
                    r + u128::from(s.read) * u128::from(included) / u128::from(s.seconds),
                    w + u128::from(s.written) * u128::from(included) / u128::from(s.seconds),
                    t + u128::from(included),
                )
            });
        (seconds > 0).then(|| ((r / seconds) as u64, (w / seconds) as u64))
    }
    pub fn or_connections(&mut self, text: Option<&str>) {
        self.connections.clear();
        self.omitted_connections = 0;
        self.connection_count = text.map(|_| 0);
        self.connected_count = text.map(|_| 0);
        if let Some(text) = text {
            for line in text.lines().filter(|l| !l.is_empty()) {
                let mut fields = line.split_whitespace();
                if let Some((peer, status)) = fields.next().zip(fields.next()) {
                    if let Some(count) = &mut self.connection_count {
                        *count += 1;
                    }
                    if status == "CONNECTED"
                        && let Some(count) = &mut self.connected_count
                    {
                        *count += 1;
                    }
                    if self.connections.len() < CONNECTION_LIMIT {
                        self.connections.push((
                            clean(peer).chars().take(256).collect(),
                            clean(status).chars().take(64).collect(),
                        ));
                    } else {
                        self.omitted_connections += 1;
                    }
                } else {
                    self.malformed_events = self.malformed_events.saturating_add(1);
                }
            }
        }
    }
    pub fn descriptor(&mut self, text: Option<&str>) {
        for key in [
            "observed",
            "advertised",
            "descriptor-published",
            "descriptor-orport",
            "descriptor-orport-v6",
        ] {
            self.set(
                key,
                None,
                "Own server descriptor unavailable or field missing",
            );
        }
        if let Some(text) = text {
            for line in text.lines() {
                if let Some(router) = line.strip_prefix("router ") {
                    let fields: Vec<_> = router.split_whitespace().take(5).collect();
                    if fields.len() == 5
                        && fields[1].parse::<std::net::Ipv4Addr>().is_ok()
                        && let Some(port) = parse_port(fields[2])
                    {
                        self.set("descriptor-orport", Some(port.to_string()), "");
                    }
                }
                if let Some(endpoint) = line.strip_prefix("or-address ")
                    && let Some(port) = ipv6_port(endpoint)
                    && self.number("descriptor-orport-v6").is_none()
                {
                    self.set("descriptor-orport-v6", Some(port.to_string()), "");
                }
                if let Some(bandwidth) = line.strip_prefix("bandwidth ") {
                    let values: Vec<_> = bandwidth
                        .split_whitespace()
                        .take(3)
                        .map(str::parse::<u64>)
                        .collect();
                    if let [Ok(avg), Ok(burst), Ok(observed)] = values.as_slice() {
                        self.set("observed", Some(observed.to_string()), "");
                        self.set(
                            "advertised",
                            Some((*avg).min(*burst).min(*observed).to_string()),
                            "",
                        );
                    }
                }
                if let Some(published) = line.strip_prefix("published ") {
                    self.set("descriptor-published", Some(published.to_string()), "");
                }
            }
        }
    }
    pub fn consensus(&mut self, text: Option<&str>) {
        for key in [
            "flags",
            "consensus",
            "consensus-orport",
            "consensus-orport-v6",
        ] {
            self.set(
                key,
                None,
                "Relay absent from local consensus, or status unavailable",
            );
        }
        if let Some(text) = text {
            for line in text.lines() {
                if let Some(router) = line.strip_prefix("r ") {
                    let fields: Vec<_> = router.split_whitespace().take(8).collect();
                    if fields.len() == 8
                        && fields[5].parse::<std::net::Ipv4Addr>().is_ok()
                        && let Some(port) = parse_port(fields[6])
                    {
                        self.set("consensus-orport", Some(port.to_string()), "");
                    }
                }
                if let Some(endpoint) = line.strip_prefix("a ")
                    && let Some(port) = ipv6_port(endpoint)
                    && self.number("consensus-orport-v6").is_none()
                {
                    self.set("consensus-orport-v6", Some(port.to_string()), "");
                }
                if let Some(flags) = line.strip_prefix("s ") {
                    self.set("flags", Some(flags.to_string()), "");
                }
                if let Some(weights) = line.strip_prefix("w ")
                    && let Some(value) = weights
                        .split_whitespace()
                        .find_map(|s| s.strip_prefix("Bandwidth="))
                    && let Some(bytes) = value.parse::<u64>().ok().and_then(|v| v.checked_mul(1000))
                {
                    self.set("consensus", Some(bytes.to_string()), "");
                }
            }
        }
    }
}

fn parse_port(value: &str) -> Option<u16> {
    value.parse().ok().filter(|port| *port != 0)
}
fn ipv6_port(value: &str) -> Option<u16> {
    let endpoint: std::net::SocketAddr = value.split_whitespace().next()?.parse().ok()?;
    (endpoint.is_ipv6() && endpoint.port() != 0).then_some(endpoint.port())
}

/// Parse Tor's DateSpTime as UTC without using the host's local timezone.
pub fn parse_utc(value: &str) -> Option<u64> {
    use time::{PrimitiveDateTime, macros::format_description};
    let description = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    let timestamp = PrimitiveDateTime::parse(value, description)
        .ok()?
        .assume_utc()
        .unix_timestamp();
    u64::try_from(timestamp).ok()
}

pub fn bytes(value: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let mut number = value as f64;
    let mut index = 0;
    while number >= 1024.0 && index < units.len() - 1 {
        number /= 1024.0;
        index += 1;
    }
    format!("{number:.1} {}", units[index])
}
pub fn rate(value: Option<u64>) -> String {
    value.map_or_else(|| "N/A".to_string(), |v| format!("{}/s", bytes(v)))
}
pub fn duration(seconds: u64) -> String {
    format!(
        "{}d {:02}:{:02}:{:02}",
        seconds / 86400,
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn long_averages_follow_window_without_counting_unknown_hours_as_zero() {
        let now = std::time::Instant::now();
        let mut state = super::State {
            connected: true,
            ..super::State::default()
        };
        state.long_history.push(&super::Sample {
            read: 6000,
            written: 12000,
            seconds: 60,
            received: now - std::time::Duration::from_secs(7200),
        });
        let recent = super::Sample {
            read: 60000,
            written: 120000,
            seconds: 60,
            received: now,
        };
        state.long_history.push(&recent);
        state.history.push_back(recent);
        assert_eq!(state.mean_rates_at(3600, now), Some((1000, 2000)));
        assert_eq!(state.mean_rates_at(86400, now), Some((550, 1100)));
        state.history.clear();
        assert!(state.mean_rates_at(86400, now).is_none());
        assert!(!state.long_history.buckets.is_empty());
    }
    use super::*;
    #[test]
    fn units_and_absence() {
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(rate(Some(1048576)), "1.0 MiB/s");
        assert_eq!(rate(None), "N/A");
        assert_eq!(State::default().metric("observed"), "N/A");
        assert_eq!(duration(90061), "1d 01:01:01");
    }
    #[test]
    fn published_port_uses_advertised_sources_instead_of_a_local_listener() {
        let mut state = State::default();
        state.set("ORPort", Some("9001 NoAdvertise".into()), "");
        assert_eq!(state.port_label(), "not detected");
        state.consensus(Some("r Example identity digest 2026-11-21 00:00:00 192.0.2.1 8443 0\na [2001:db8::1]:9443\ns Running\nw Bandwidth=10"));
        assert_eq!(state.published_or_port(false), Some(8443));
        assert_eq!(state.published_or_port(true), Some(9443));
        state.descriptor(Some(
            "router Example 192.0.2.1 443 0 0\nor-address [2001:db8::1]:444\nbandwidth 100 200 90",
        ));
        assert_eq!(state.port_label(), "443");
        assert_eq!(state.published_or_port(true), Some(444));
        state.descriptor(None);
        assert_eq!(state.port_label(), "8443");
        state.consensus(None);
        assert_eq!(state.port_label(), "not detected");
        state.descriptor(Some(
            "router Example 192.0.2.1 99999 0 0\nor-address [2001:db8::1]:0",
        ));
        assert_eq!(state.port_label(), "not detected");
    }
    #[test]
    fn counters_aggregate_without_losing_bytes() {
        let mut s = State {
            connected: true,
            ..State::default()
        };
        let mut last = BandwidthAggregate::default();
        s.bandwidth(
            &mut last,
            BandwidthAggregate {
                samples: 2,
                read: 300,
                written: 600,
            },
        );
        assert_eq!(s.rates(), Some((150, 300)));
        assert_eq!(s.history[0].read, 300);
        assert_eq!((s.session_read, s.session_written), (300, 600));
        let unchanged = last;
        s.bandwidth(&mut last, unchanged);
        assert_eq!(s.history.len(), 1);
        assert_eq!((s.session_read, s.session_written), (300, 600));
        s.connected = false;
        assert!(s.rates().is_none());
    }
    #[test]
    fn session_totals_survive_reconnect_but_start_fresh_for_a_new_monitor() {
        let mut state = State::default();
        let mut previous = BandwidthAggregate::default();
        state.bandwidth(
            &mut previous,
            BandwidthAggregate {
                samples: 3,
                read: 90,
                written: 120,
            },
        );
        state.history.clear();
        previous = BandwidthAggregate::default();
        state.bandwidth(
            &mut previous,
            BandwidthAggregate {
                samples: 1,
                read: 5,
                written: 10,
            },
        );
        assert_eq!((state.session_read, state.session_written), (95, 130));
        assert_eq!(
            (
                State::default().session_read,
                State::default().session_written
            ),
            (0, 0)
        );
    }
    #[test]
    fn descriptors_and_consensus_are_distinct() {
        let mut s = State::default();
        s.descriptor(Some(
            "bandwidth 100000 200000 80000\npublished 2026-01-01 00:00:00",
        ));
        s.consensus(Some("s Fast Guard FutureFlag\nw Bandwidth=75 Unmeasured=1"));
        assert_eq!(s.number("observed"), Some(80000));
        assert_eq!(s.number("advertised"), Some(80000));
        assert_eq!(s.number("consensus"), Some(75000));
        assert!(s.metric("flags").contains("FutureFlag"));
        s.descriptor(None);
        assert!(s.number("observed").is_none());
    }
    #[test]
    fn storage_is_bounded() {
        let mut s = State::default();
        for _ in 0..10_000 {
            s.log("WARN", "hello\x1b[31m");
        }
        assert_eq!(s.logs.len(), LOG_LIMIT);
        assert!(!s.logs[0].message.contains('\x1b'));
        s.or_connections(Some(&"$ABC=peer CONNECTED\n".repeat(3000)));
        assert_eq!(s.connections.len(), CONNECTION_LIMIT);
        assert_eq!(s.connection_count, Some(3000));
        assert_eq!(s.connected_count, Some(3000));
        assert_eq!(s.omitted_connections, 952);
    }
    #[test]
    fn accounting_countdown_uses_tor_utc_period_and_never_guesses_midnight() {
        let mut state = State {
            connected: true,
            ..State::default()
        };
        state.set("accounting/enabled", Some("1".into()), "");
        state.set(
            "accounting/interval-end",
            Some("2026-11-22 12:00:00".into()),
            "",
        );
        state.set("accounting/bytes", Some("1024 2048".into()), "");
        let now =
            UNIX_EPOCH + std::time::Duration::from_secs(parse_utc("2026-11-22 11:59:00").unwrap());
        assert_eq!(state.accounting_reset(now), Some(60));
        assert_eq!(state.pair("accounting/bytes"), Some((1024, 2048)));
        state.connected = false;
        assert_eq!(state.accounting_reset(now), None);
        state.connected = true;
        state.set("accounting/enabled", Some("0".into()), "");
        assert_eq!(state.accounting_reset(now), None);
        assert_eq!(parse_utc("2026-02-30 00:00:00"), None);
        assert_eq!(parse_utc("2026-11-22 99:00:00"), None);
    }
    #[test]
    fn average_follows_selected_window_and_handles_partial_aggregates() {
        let mut state = State {
            connected: true,
            ..State::default()
        };
        state.history.push_back(Sample {
            read: 600,
            written: 1200,
            seconds: 60,
            received: Instant::now(),
        });
        assert_eq!(state.mean_rates_for(30), Some((10, 20)));
        assert_eq!(state.mean_rates_for(0), None);
    }
}
