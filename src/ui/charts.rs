//! Time-based graph buckets and a small full-width terminal graph widget.
use super::{block, color};
use crate::{
    config::Config,
    model::{State, bytes, rate},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Color,
    text::{Line, Span},
    widgets::Paragraph,
};
use std::time::Instant;

/// Each bucket covers a real time range. Missing intervals remain None, never zero.
#[cfg(test)]
pub(super) fn buckets(state: &State, width: usize, seconds: u32) -> Vec<Option<(u64, u64)>> {
    buckets_at(state, width, seconds, Instant::now())
}

fn buckets_at(state: &State, width: usize, seconds: u32, now: Instant) -> Vec<Option<(u64, u64)>> {
    let mut output = Vec::with_capacity(width);
    let history: Vec<_> = state.history.iter().rev().collect();
    let mut samples = Vec::with_capacity(history.len());
    for (index, sample) in history.iter().enumerate() {
        let age = now.saturating_duration_since(sample.received).as_secs_f64();
        let nominal = sample.seconds as f64;
        if nominal == 0.0 {
            continue;
        }
        let mut end = age + nominal;
        // BW describes nominal one-second intervals. Avoid turning sub-second
        // scheduling jitter into holes, without accumulating clock drift.
        if let Some(older) = history.get(index + 1) {
            let boundary = now.saturating_duration_since(older.received).as_secs_f64();
            if boundary > age && (end - boundary).abs() <= 0.5 {
                end = boundary;
            }
        }
        samples.push((
            age,
            end,
            sample.read as f64 / nominal,
            sample.written as f64 / nominal,
            1.0,
        ));
    }
    if seconds > 120 {
        samples.clear();
        samples.extend(
            state
                .long_history
                .buckets
                .iter()
                .filter(|b| b.covered > 0.0)
                .map(|b| {
                    (
                        now.saturating_duration_since(b.end).as_secs_f64(),
                        now.saturating_duration_since(b.start).as_secs_f64(),
                        b.read / b.covered,
                        b.written / b.covered,
                        b.covered
                            / b.end
                                .saturating_duration_since(b.start)
                                .as_secs_f64()
                                .max(f64::EPSILON),
                    )
                }),
        );
    }
    for column in 0..width {
        let newer = f64::from(seconds) * (width - column - 1) as f64 / width as f64;
        let older = f64::from(seconds) * (width - column) as f64 / width as f64;
        let (mut read, mut written, mut covered) = (0.0, 0.0, 0.0);
        for &(age, end, read_rate, write_rate, density) in &samples {
            // Partial/gapped minutes contribute only their observed interval count.
            let overlap = (older.min(end) - newer.max(age)) * density;
            if overlap > 0.0 {
                read += read_rate * overlap;
                written += write_rate * overlap;
                covered += overlap;
            }
        }
        output.push((covered > 0.0).then(|| ((read / covered) as u64, (written / covered) as u64)));
    }
    output
}

fn scale_peak(value: u64) -> u64 {
    // Round up to a 1/2/5 step in the displayed binary unit, for readable Y ticks.
    let mut unit = 1u64;
    while value / unit >= 1024 && unit <= u64::MAX / 1024 {
        unit *= 1024;
    }
    let mut step = unit;
    while value / step > 10 && step <= u64::MAX / 10 {
        step *= 10;
    }
    [1, 2, 5, 10]
        .into_iter()
        .find_map(|factor| step.checked_mul(factor).filter(|v| *v >= value))
        .unwrap_or(value)
        .max(1)
}

pub(super) fn render(
    frame: &mut Frame<'_>,
    state: &State,
    area: Rect,
    config: &Config,
    seconds: u32,
    upload: bool,
    now: Instant,
) {
    let ink = if upload { Color::Green } else { Color::Cyan };
    let rates = state.rates_at(now);
    let mean = state.mean_rates_at(u64::from(seconds), now);
    let current = rates.map(|v| if upload { v.1 } else { v.0 });
    let average = mean.map(|v| if upload { v.1 } else { v.0 });
    let label = if upload { "Upload" } else { "Download" };
    let session = if upload {
        state.session_written
    } else {
        state.session_read
    };
    let title = Line::from(vec![
        Span::styled(format!(" {label}: "), color(config, ink)),
        Span::raw(format!(
            "{} | avg {} | session: {} ",
            rate(current),
            rate(average),
            bytes(session)
        )),
    ]);
    let border = block(title, config);
    let inner = border.inner(area);
    frame.render_widget(border, area);
    if inner.width < 16 || inner.height < 2 {
        return;
    }
    let label_width = 12u16.min(inner.width / 3);
    let plot = Rect::new(
        inner.x + label_width,
        inner.y,
        inner.width - label_width,
        inner.height - 1,
    );
    let buckets = buckets_at(state, plot.width as usize, seconds, now);
    let maximum = scale_peak(
        buckets
            .iter()
            .flatten()
            .map(|v| v.0.max(v.1))
            .max()
            .unwrap_or(1),
    );
    let buffer = frame.buffer_mut();
    let symbols = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    for row in 0..plot.height {
        let tick = if row == 0 {
            Some(maximum)
        } else if row == plot.height - 1 {
            Some(0)
        } else if row == plot.height / 2 && plot.height >= 5 {
            Some(maximum / 2)
        } else {
            None
        };
        if let Some(value) = tick {
            let text = format!(
                "{:>width$} {}",
                rate(Some(value)),
                if config.ascii { '|' } else { '│' },
                width = label_width.saturating_sub(2) as usize,
            );
            buffer.set_stringn(
                inner.x,
                inner.y + row,
                text,
                label_width as usize,
                color(config, Color::DarkGray),
            );
        }
        for (column, bucket) in buckets.iter().enumerate() {
            let cell = &mut buffer[(plot.x + column as u16, plot.y + row)];
            let Some((read, written)) = bucket else {
                continue;
            };
            let value = if upload { *written } else { *read };
            let eighths =
                (u128::from(value) * u128::from(plot.height) * 8 / u128::from(maximum)) as u64;
            let floor = u64::from(plot.height - row - 1) * 8;
            let level = eighths.saturating_sub(floor).min(8) as usize;
            let symbol = if config.ascii {
                if level > 0 { "#" } else { " " }
            } else if level > 0 {
                symbols[level]
            } else {
                " "
            };
            cell.set_symbol(symbol).set_style(color(config, ink));
        }
    }
    let axis = Rect::new(plot.x, inner.y + inner.height - 1, plot.width, 1);
    let mut time_axis = vec![' '; axis.width as usize];
    for (position, text) in [
        (0usize, format!("-{}", window_label(seconds))),
        (
            axis.width as usize / 2,
            format!("-{}", window_label(seconds / 2)),
        ),
        (axis.width.saturating_sub(2) as usize, "0s".to_string()),
    ] {
        for (index, ch) in text.chars().enumerate() {
            if position + index < time_axis.len() {
                time_axis[position + index] = ch;
            }
        }
    }
    frame.render_widget(
        Paragraph::new(time_axis.into_iter().collect::<String>())
            .style(color(config, Color::DarkGray)),
        axis,
    );
}

pub(super) fn window_label(seconds: u32) -> String {
    if seconds >= 3600 && seconds.is_multiple_of(3600) {
        format!("{}h", seconds / 3600)
    } else if seconds > 120 && seconds.is_multiple_of(60) {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Sample;
    use std::time::{Duration, Instant};
    #[test]
    fn graph_spans_full_width_and_preserves_missing_intervals() {
        let mut state = State::default();
        state.history.push_back(Sample {
            read: 12000,
            written: 24000,
            seconds: 120,
            received: Instant::now(),
        });
        let columns = buckets(&state, 240, 120);
        assert!(columns.iter().all(Option::is_some));
        assert!(
            columns
                .iter()
                .flatten()
                .all(|&(r, w)| (99..=100).contains(&r) && (199..=200).contains(&w))
        );
        state.history.clear();
        state.history.push_back(Sample {
            read: 0,
            written: 0,
            seconds: 1,
            received: Instant::now() - Duration::from_secs(10),
        });
        let columns = buckets(&state, 120, 120);
        assert!(columns.contains(&Some((0, 0))));
        assert!(columns.iter().any(Option::is_none));
        assert_eq!(columns.last(), Some(&None));
    }
    #[test]
    fn scheduling_jitter_does_not_cut_black_columns_through_continuous_data() {
        let now = Instant::now();
        let mut state = State::default();
        for age in (0..30u64).rev() {
            let jitter = if age % 2 == 0 { 0 } else { 200 };
            state.history.push_back(Sample {
                read: 4096,
                written: 8192,
                seconds: 1,
                received: now - Duration::from_millis(age * 1000 + jitter),
            });
        }
        let columns = buckets_at(&state, 300, 30, now);
        assert!(columns.iter().all(Option::is_some));
        state.history.retain(|sample| {
            let age = now.duration_since(sample.received).as_secs();
            !(10..=12).contains(&age)
        });
        let columns = buckets_at(&state, 300, 30, now);
        assert!(columns.iter().any(Option::is_none));
        state.history.clear();
        for age in (0..120u64).rev() {
            state.history.push_back(Sample {
                read: 4096,
                written: 8192,
                seconds: 1,
                received: now - Duration::from_millis(age * 1020),
            });
        }
        assert!(
            buckets_at(&state, 600, 120, now)
                .iter()
                .all(Option::is_some)
        );
    }
    #[test]
    fn long_windows_use_minute_rates_and_keep_empty_minutes_blank() {
        let now = Instant::now();
        let mut state = State::default();
        for age in [7200, 7199, 120, 119, 1, 0] {
            state.long_history.push(&Sample {
                read: 1024,
                written: 2048,
                seconds: 1,
                received: now - Duration::from_secs(age),
            });
        }
        let columns = buckets_at(&state, 24, 86400, now);
        assert!(columns[23].is_some());
        assert!(columns[..21].iter().all(Option::is_none));
        assert_eq!(columns[23], Some((1024, 2048)));
        assert!(
            buckets_at(&state, 60, 3600, now)
                .iter()
                .filter(|c| c.is_some())
                .count()
                < 8
        );
    }
    #[test]
    fn zero_traffic_does_not_draw_decorative_baseline_dots() {
        use ratatui::{Terminal, backend::TestBackend};
        for ascii in [true, false] {
            let mut state = State {
                connected: true,
                ..State::default()
            };
            state.history.push_back(Sample {
                read: 0,
                written: 0,
                seconds: 30,
                received: Instant::now(),
            });
            let mut terminal = Terminal::new(TestBackend::new(100, 10)).unwrap();
            terminal
                .draw(|frame| {
                    render(
                        frame,
                        &state,
                        frame.area(),
                        &Config {
                            ascii,
                            ..Config::default()
                        },
                        30,
                        false,
                        Instant::now(),
                    )
                })
                .unwrap();
            for y in 1..8 {
                for x in 13..99 {
                    assert_eq!(terminal.backend().buffer()[(x, y)].symbol(), " ");
                }
            }
        }
    }
}
