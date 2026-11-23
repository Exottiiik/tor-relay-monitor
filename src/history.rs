//! At most 1,441 minute buckets, kept only in memory for this monitor run.
use crate::model::Sample;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub const LONG_LIMIT: usize = 1441;
pub const DAY: u64 = 86400;

#[derive(Clone, Debug)]
pub struct Bucket {
    slot: u64,
    pub start: Instant,
    pub end: Instant,
    pub read: f64,
    pub written: f64,
    /// Observed BW seconds, excluding gaps; partial minutes are not zero-filled.
    pub covered: f64,
}

#[derive(Clone, Debug, Default)]
pub struct History {
    pub buckets: VecDeque<Bucket>,
    epoch: Option<Instant>,
    last_received: Option<Instant>,
}

impl History {
    pub fn push(&mut self, sample: &Sample) {
        if sample.seconds == 0 {
            return;
        }
        let duration = Duration::from_secs(sample.seconds);
        let Some(mut start) = sample.received.checked_sub(duration) else {
            return;
        };
        if let Some(previous) = self.last_received {
            let elapsed = sample
                .received
                .saturating_duration_since(previous)
                .as_secs_f64();
            if (elapsed - sample.seconds as f64).abs() <= 0.5 {
                start = previous;
            } else {
                start = start.max(previous);
            }
        }
        self.last_received = Some(sample.received);
        if start >= sample.received {
            return;
        }
        let epoch = *self.epoch.get_or_insert(start);
        let end = sample
            .received
            .saturating_duration_since(epoch)
            .as_secs_f64();
        let mut cursor = start
            .saturating_duration_since(epoch)
            .as_secs_f64()
            .max(end - DAY as f64);
        let span = sample.received.duration_since(start).as_secs_f64();
        while cursor < end {
            let index = (cursor / 60.0).floor() as u64;
            let boundary = ((index + 1) * 60) as f64;
            let next = end.min(boundary);
            let bucket_start = epoch + Duration::from_secs(index * 60);
            if self.buckets.back().is_none_or(|b| b.slot != index) {
                self.buckets.push_back(Bucket {
                    slot: index,
                    start: epoch + Duration::from_secs_f64(cursor),
                    end: bucket_start,
                    read: 0.0,
                    written: 0.0,
                    covered: 0.0,
                });
            }
            if let Some(bucket) = self.buckets.back_mut() {
                let observed = next - cursor;
                bucket.read += sample.read as f64 * observed / span;
                bucket.written += sample.written as f64 * observed / span;
                bucket.covered += sample.seconds as f64 * observed / span;
                bucket.end = epoch + Duration::from_secs_f64(next);
            }
            cursor = next;
        }
        while self.buckets.len() > LONG_LIMIT
            || self
                .buckets
                .front()
                .is_some_and(|b| sample.received.saturating_duration_since(b.end).as_secs() >= DAY)
        {
            self.buckets.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_minutes_preserve_bytes_partial_minutes_and_gaps() {
        let now = Instant::now();
        let mut h = History::default();
        for second in 0..90001 {
            if (100..220).contains(&second) {
                continue;
            }
            h.push(&Sample {
                read: 100,
                written: 200,
                seconds: 1,
                received: now + Duration::from_secs(second),
            });
        }
        assert!(h.buckets.len() <= LONG_LIMIT);
        assert!(h.buckets.iter().all(|b| b.covered <= 60.01));
        let mut short = History::default();
        for second in [0, 1, 180] {
            short.push(&Sample {
                read: 100,
                written: 200,
                seconds: 1,
                received: now + Duration::from_secs(second),
            });
        }
        assert_eq!(short.buckets.len(), 2);
        assert_eq!(short.buckets.iter().map(|b| b.read).sum::<f64>(), 300.0);
        assert_eq!(short.buckets.iter().map(|b| b.covered).sum::<f64>(), 3.0);
    }
    #[test]
    fn jitter_across_minute_boundaries_keeps_order_and_all_bytes() {
        let now = Instant::now();
        let mut h = History::default();
        for second in 0..3601 {
            h.push(&Sample {
                read: 100,
                written: 200,
                seconds: 1,
                received: now
                    + Duration::from_millis(second * 1000 + if second % 2 == 0 { 0 } else { 200 }),
            });
        }
        assert!(h.buckets.len() <= 61);
        assert!((h.buckets.iter().map(|b| b.read).sum::<f64>() - 360100.0).abs() < 0.001);
        for (a, b) in h.buckets.iter().zip(h.buckets.iter().skip(1)) {
            assert!(a.slot < b.slot);
            assert!(a.end <= b.start);
        }
    }
    #[test]
    fn delayed_aggregate_is_split_without_unbounded_work() {
        let mut h = History::default();
        h.push(&Sample {
            read: 200000,
            written: 400000,
            seconds: 200000,
            received: Instant::now(),
        });
        assert!(h.buckets.len() <= LONG_LIMIT);
        assert!((h.buckets.iter().map(|b| b.read).sum::<f64>() - 86400.0).abs() < 0.001);
    }
}
