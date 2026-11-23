//! Local Linux process sampling, isolated from Tor protocol and terminal drawing.
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Resources {
    pub cpu: Option<f64>,
    pub rss: Option<u64>,
    pub descriptors: Option<u64>,
    pub descriptor_limit: Option<u64>,
    pub reason: String,
    pub sampled: Option<Instant>,
}
impl Default for Resources {
    fn default() -> Self {
        Self {
            cpu: None,
            rss: None,
            descriptors: None,
            descriptor_limit: None,
            reason: "Waiting for a Tor process sample".into(),
            sampled: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[cfg(target_os = "linux")]
pub struct CpuPoint {
    pid: u32,
    start: u64,
    ticks: u64,
    at: Instant,
}
#[derive(Clone, Copy, Debug)]
#[cfg(not(target_os = "linux"))]
pub struct CpuPoint;

pub async fn sample(pid: Option<u32>, previous: Option<CpuPoint>) -> (Resources, Option<CpuPoint>) {
    let Some(pid) = pid.filter(|pid| *pid != 0) else {
        return (
            Resources {
                reason: "Tor PID unavailable".into(),
                ..Resources::default()
            },
            None,
        );
    };
    tokio::task::spawn_blocking(move || read(pid, previous))
        .await
        .unwrap_or_else(|_| {
            (
                Resources {
                    reason: "Process sampler unavailable".into(),
                    ..Resources::default()
                },
                None,
            )
        })
}

#[cfg(target_os = "linux")]
fn read(pid: u32, previous: Option<CpuPoint>) -> (Resources, Option<CpuPoint>) {
    use std::{
        fs::{self, File},
        io::Read,
    };
    fn bounded(path: &str) -> std::io::Result<String> {
        let mut contents = String::new();
        File::open(path)?
            .take(65536)
            .read_to_string(&mut contents)?;
        Ok(contents)
    }
    let base = format!("/proc/{pid}");
    let parsed = bounded(&format!("{base}/stat"))
        .ok()
        .and_then(|s| parse_stat(&s));
    let Some((ticks, start, rss_pages)) = parsed else {
        return (
            Resources {
                reason: "Cannot read Tor /proc stat (permissions, PID namespace or process exit)"
                    .into(),
                ..Resources::default()
            },
            None,
        );
    };
    // sysconf only reads constants; no pointer or external state is modified.
    let (hz, page_size) = unsafe {
        (
            libc::sysconf(libc::_SC_CLK_TCK),
            libc::sysconf(libc::_SC_PAGESIZE),
        )
    };
    let now = Instant::now();
    let current = CpuPoint {
        pid,
        start,
        ticks,
        at: now,
    };
    let cpu = previous
        .filter(|p| p.pid == pid && p.start == start && p.ticks <= ticks)
        .filter(|p| now.saturating_duration_since(p.at).as_secs_f64() > 0.0 && hz > 0)
        .map(|p| {
            (ticks - p.ticks) as f64 / hz as f64 / now.duration_since(p.at).as_secs_f64() * 100.0
        });
    let rss = u64::try_from(page_size)
        .ok()
        .and_then(|size| rss_pages.checked_mul(size));
    let descriptors = fs::read_dir(format!("{base}/fd")).ok().and_then(|entries| {
        let mut count = 0u64;
        for entry in entries.take(1_048_577) {
            entry.ok()?;
            count += 1;
        }
        (count <= 1_048_576).then_some(count)
    });
    let descriptor_limit = bounded(&format!("{base}/limits"))
        .ok()
        .and_then(|text| parse_limit(&text));
    // Detect exit/PID reuse during sampling before accepting unrelated counters.
    let unchanged = bounded(&format!("{base}/stat"))
        .ok()
        .and_then(|s| parse_stat(&s))
        .is_some_and(|(_, check, _)| check == start);
    if !unchanged {
        return (
            Resources {
                reason: "Tor process changed during sampling".into(),
                ..Resources::default()
            },
            None,
        );
    }
    let reason = if descriptors.is_none() || descriptor_limit.is_none() {
        "Some /proc files are inaccessible, unsupported or exceed the descriptor scan limit"
    } else if cpu.is_none() {
        "CPU needs two samples from the same Tor process"
    } else {
        ""
    };
    (
        Resources {
            cpu,
            rss,
            descriptors,
            descriptor_limit,
            reason: reason.into(),
            sampled: Some(now),
        },
        Some(current),
    )
}

#[cfg(not(target_os = "linux"))]
fn read(_pid: u32, _previous: Option<CpuPoint>) -> (Resources, Option<CpuPoint>) {
    (
        Resources {
            reason: "Process resources require Linux /proc".into(),
            ..Resources::default()
        },
        None,
    )
}

#[cfg(any(target_os = "linux", test))]
fn parse_stat(text: &str) -> Option<(u64, u64, u64)> {
    let fields: Vec<_> = text
        .rsplit_once(')')?
        .1
        .split_whitespace()
        .take(22)
        .collect();
    Some((
        fields
            .get(11)?
            .parse::<u64>()
            .ok()?
            .checked_add(fields.get(12)?.parse().ok()?)?,
        fields.get(19)?.parse().ok()?,
        fields.get(21)?.parse().ok()?,
    ))
}

#[cfg(any(target_os = "linux", test))]
fn parse_limit(text: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        line.strip_prefix("Max open files")
            .and_then(|v| v.split_whitespace().next()?.parse().ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_complex_comm_and_soft_limit_without_panicking_on_bad_data() {
        let mut fields = vec!["0"; 22];
        fields[0] = "S";
        fields[11] = "100";
        fields[12] = "20";
        fields[19] = "999";
        fields[21] = "42";
        assert_eq!(
            parse_stat(&format!("123 (tor (worker)) {}", fields.join(" "))),
            Some((120, 999, 42))
        );
        assert_eq!(parse_stat("123 (tor) S 1"), None);
        assert_eq!(
            parse_limit(
                "Max open files            1024                 65536                files\n"
            ),
            Some(1024)
        );
        assert_eq!(
            parse_limit("Max open files unlimited unlimited files"),
            None
        );
    }
    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn samples_real_local_process_and_handles_missing_pid() {
        let (first, previous) = sample(Some(std::process::id()), None).await;
        assert!(first.rss.is_some_and(|n| n > 0));
        assert!(first.descriptors.is_some_and(|n| n > 0));
        assert!(first.descriptor_limit.is_some());
        assert!(first.cpu.is_none());
        let mut replaced = previous.unwrap();
        replaced.start = replaced.start.wrapping_add(1);
        assert!(
            sample(Some(std::process::id()), Some(replaced))
                .await
                .0
                .cpu
                .is_none()
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(
            sample(Some(std::process::id()), previous)
                .await
                .0
                .cpu
                .is_some()
        );
        assert!(sample(None, previous).await.0.rss.is_none());
        assert!(sample(Some(u32::MAX), previous).await.0.rss.is_none());
    }
}
