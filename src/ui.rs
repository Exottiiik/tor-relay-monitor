//! Terminal presentation: no Tor commands or network access.
mod charts;
mod filters;
use crate::{
    config::Config,
    model::{State, bytes, duration, rate},
    preferences::Preferences,
};
use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures_util::StreamExt;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap},
};
use std::{
    io,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use tokio::sync::watch;

#[derive(Default)]
pub struct View {
    pub page: u8,
    pub scroll: usize,
    pub help: bool,
    pub preferences: Preferences,
    pub notice: Option<String>,
    pub paused_at: Option<Instant>,
    pub paused_wall: Option<SystemTime>,
    pub log_query: String,
    pub or_query: String,
    pub search: Option<String>,
    pub severity: u8,
    pub show_repeats: bool,
    pub sort: filters::Sort,
    pub reverse: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Continue,
    Quit,
    Save,
    Pause,
}

impl View {
    fn toggle_pause(&mut self, state: &Arc<State>, frozen: &mut Option<Arc<State>>) {
        if frozen.take().is_some() {
            self.paused_at = None;
            self.paused_wall = None;
        } else {
            *frozen = Some(Arc::clone(state));
            self.paused_at = Some(Instant::now());
            self.paused_wall = Some(SystemTime::now());
        }
    }
    fn now(&self) -> Instant {
        self.paused_at.unwrap_or_else(Instant::now)
    }
    fn query(&self) -> &str {
        self.search.as_deref().unwrap_or(if self.page == 1 {
            &self.log_query
        } else {
            &self.or_query
        })
    }
    fn key(&mut self, key: KeyEvent) -> Action {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Action::Quit;
        }
        if let Some(draft) = self.search.as_mut() {
            match key.code {
                KeyCode::Esc => self.search = None,
                KeyCode::Enter => {
                    let query = self.search.take().unwrap_or_default();
                    if self.page == 1 {
                        self.log_query = query;
                    } else {
                        self.or_query = query;
                    }
                }
                KeyCode::Backspace => {
                    draft.pop();
                }
                KeyCode::Char(c)
                    if !c.is_control()
                        && !key.modifiers.contains(KeyModifiers::CONTROL)
                        && draft.chars().count() < 128 =>
                {
                    draft.push(c)
                }
                _ => {}
            }
            self.scroll = 0;
            return Action::Continue;
        }
        match key.code {
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Char('p') => return Action::Pause,
            KeyCode::Char(c @ '1'..='3') => {
                self.page = c as u8 - b'1';
                self.scroll = 0;
                self.help = false;
            }
            KeyCode::Char('h' | '?') => {
                self.help = !self.help;
                self.scroll = 0;
            }
            KeyCode::Char('/') if self.page > 0 && !self.help => {
                self.search = Some(self.query().to_string());
                self.scroll = 0;
            }
            KeyCode::Esc if self.page > 0 && !self.help => {
                if self.page == 1 {
                    self.log_query.clear();
                } else {
                    self.or_query.clear();
                }
                self.scroll = 0;
            }
            KeyCode::Char('e') if self.page == 1 && !self.help => {
                self.severity = (self.severity + 1) % 3;
                self.scroll = 0;
            }
            KeyCode::Char('d') if self.page == 1 && !self.help => {
                self.show_repeats = !self.show_repeats;
                self.scroll = 0;
            }
            KeyCode::Char('s') if self.page == 2 && !self.help => {
                self.sort = self.sort.next();
                self.scroll = 0;
            }
            KeyCode::Char('r') if self.page == 2 && !self.help => {
                self.reverse = !self.reverse;
                self.scroll = 0;
            }
            KeyCode::Up => self.scroll = self.scroll.saturating_add(1).min(4096),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_add(10).min(4096),
            KeyCode::Down => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::Home => self.scroll = 0,
            KeyCode::Char(k @ ('[' | ']' | '-' | '+' | '=')) => {
                match k {
                    '[' => {
                        self.preferences.graph_height =
                            self.preferences.graph_height.saturating_sub(1).max(5)
                    }
                    ']' => {
                        self.preferences.graph_height = (self.preferences.graph_height + 1).min(30)
                    }
                    '-' => self.preferences.step_window(false),
                    _ => self.preferences.step_window(true),
                }
                return Action::Save;
            }
            _ => {}
        }
        Action::Continue
    }
}

fn color(config: &Config, ink: Color) -> Style {
    if config.no_color {
        Style::default()
    } else {
        Style::default().fg(ink)
    }
}
fn strong(config: &Config, ink: Color) -> Style {
    color(config, ink).add_modifier(Modifier::BOLD)
}
fn block<'a>(title: impl Into<Line<'a>>, config: &Config) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(color(config, Color::DarkGray))
        .border_set(if config.ascii {
            ratatui::symbols::border::Set {
                top_left: "+",
                top_right: "+",
                bottom_left: "+",
                bottom_right: "+",
                vertical_left: "|",
                vertical_right: "|",
                horizontal_top: "-",
                horizontal_bottom: "-",
            }
        } else {
            ratatui::symbols::border::ROUNDED
        })
}
fn label(text: impl Into<String>, config: &Config) -> Span<'static> {
    Span::styled(text.into(), color(config, Color::DarkGray))
}
fn value(text: impl Into<String>, config: &Config, ink: Color) -> Span<'static> {
    Span::styled(text.into(), color(config, ink))
}
fn flag_color(flag: &str) -> Color {
    match flag {
        "Running" | "Valid" => Color::Green,
        "Guard" => Color::Cyan,
        "Fast" => Color::Yellow,
        "Stable" => Color::Blue,
        "Exit" => Color::Magenta,
        "BadExit" | "StaleDesc" => Color::Red,
        "HSDir" | "V2Dir" => Color::White,
        "Authority" => Color::Magenta,
        "MiddleOnly" | "NoEdConsensus" => Color::Yellow,
        _ => Color::Gray,
    }
}

fn version_badge(state: &State, config: &Config) -> (&'static str, Color) {
    if !state.connected {
        return ("(?)", Color::DarkGray);
    }
    match state.metric("status/version/current") {
        "recommended" => (
            if config.no_color {
                "(UP: recommended)"
            } else {
                "(UP)"
            },
            Color::Green,
        ),
        "old" | "obsolete" | "unrecommended" => (
            if config.no_color {
                "(UP: update)"
            } else {
                "(UP)"
            },
            Color::Red,
        ),
        "new" | "new in series" => ("(NEW)", Color::Cyan),
        _ => ("(?)", Color::DarkGray),
    }
}

/// Pack whole fields before wrapping, so separators never start a new row.
fn pack_fields<'a>(fields: Vec<Vec<Span<'a>>>, width: usize, separator: Span<'a>) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let mut current = Line::default();
    for field in fields {
        let field = Line::from(field);
        let separator_width = if current.spans.is_empty() {
            0
        } else {
            separator.width()
        };
        if !current.spans.is_empty() && current.width() + separator_width + field.width() > width {
            lines.push(current);
            current = Line::default();
        }
        if !current.spans.is_empty() {
            current.spans.push(separator.clone());
        }
        for span in field.spans {
            if !current.spans.is_empty() && current.width() + span.width() > width {
                lines.push(current);
                current = Line::default();
            }
            current.spans.push(span);
        }
    }
    if !current.spans.is_empty() {
        lines.push(current);
    }
    lines
}

fn header_lines<'a>(state: &'a State, config: &Config, width: u16, now: Instant) -> Vec<Line<'a>> {
    let (badge, badge_color) = version_badge(state, config);
    let version = state
        .metric("version")
        .split_whitespace()
        .next()
        .unwrap_or("N/A");
    let fields = vec![
        vec![
            label("Relay name: ", config),
            Span::styled(state.metric("Nickname"), strong(config, Color::White)),
        ],
        vec![
            value(format!("V.{version} "), config, Color::Cyan),
            Span::styled(badge, strong(config, badge_color)),
        ],
        vec![
            label("PID: ", config),
            Span::raw(state.metric("process/pid")),
        ],
        vec![
            label("Uptime: ", config),
            value(process_uptime(state, now), config, Color::Green),
        ],
        vec![label("IPv4: ", config), Span::raw(state.address(false))],
        vec![label("IPv6: ", config), Span::raw(state.address(true))],
        vec![label("Port: ", config), Span::raw(state.port_label())],
    ];
    let mut fields = fields;
    if let Some(port) = state.published_or_port(true)
        && state
            .published_or_port(false)
            .is_some_and(|ipv4| ipv4 != port)
    {
        fields.push(vec![
            label("IPv6 port: ", config),
            Span::raw(port.to_string()),
        ]);
    }
    let mut lines = pack_fields(fields, width as usize, label(" | ", config));
    lines.push(Line::from(vec![
        label(if width < 54 { "" } else { "Fingerprint: " }, config),
        Span::raw(state.metric("fingerprint")),
    ]));
    let mut flags = vec![label("Flags: ", config)];
    for (index, flag) in state.metric("flags").split_whitespace().enumerate() {
        let prefix = if index == 0 { "" } else { ", " };
        let span = Span::styled(flag.to_string(), strong(config, flag_color(flag)));
        if Line::from(flags.clone()).width() + prefix.len() + span.width() > width as usize
            && flags.len() > 1
        {
            // Keep the comma on the preceding line and all flags visible.
            flags.push(label(",", config));
            lines.push(Line::from(flags));
            flags = vec![Span::styled(
                flag.to_string(),
                strong(config, flag_color(flag)),
            )];
        } else {
            if !prefix.is_empty() {
                flags.push(label(prefix, config));
            }
            flags.push(span);
        }
    }
    lines.push(Line::from(flags));
    lines
}

pub fn render(frame: &mut Frame<'_>, state: &State, view: &View, config: &Config) {
    let area = frame.area();
    if area.width < 44 || area.height < 14 {
        frame.render_widget(
            Paragraph::new("Tor Relay Monitor\nResize to at least 44x14.\nq: quit")
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let header_text = header_lines(state, config, area.width.saturating_sub(2), view.now());
    let header_height = (header_text.len() as u16 + 2).min(area.height.saturating_sub(4));
    let regions = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    let title = if view.paused_at.is_some() {
        Line::from(value(
            " TRM | PAUSED - collection continues | p resume ",
            config,
            Color::Yellow,
        ))
    } else if state.connected {
        Line::from(" TRM | Tor Relay Monitor ")
    } else {
        Line::from(Span::styled(
            " TRM | DISCONNECTED - stale data ",
            strong(config, Color::Red),
        ))
    };
    frame.render_widget(
        Paragraph::new(header_text).block(block(title, config)),
        regions[0],
    );
    if view.help {
        help(frame, state, regions[1], config, view);
    } else {
        match view.page {
            1 => logs(frame, state, regions[1], config, view),
            2 => connections(frame, state, regions[1], config, view),
            _ => dashboard(frame, state, regions[1], config, view),
        }
    }
    let mut footer = Vec::new();
    for (key, action) in [
        ("q", "quit"),
        ("1", "dashboard"),
        ("2", "logs"),
        ("3", "OR"),
        ("h", "help"),
        (
            "p",
            if view.paused_at.is_some() {
                "resume"
            } else {
                "pause"
            },
        ),
    ] {
        if !footer.is_empty() {
            footer.push(label(" | ", config));
        }
        footer.push(Span::styled(key, strong(config, Color::Cyan)));
        footer.push(Span::raw(format!(" {action}")));
    }
    if area.width >= 92 {
        footer.push(label(" | ", config));
        footer.push(value("[ ]", config, Color::Yellow));
        footer.push(Span::raw(" height  "));
        footer.push(value("- +", config, Color::Yellow));
        footer.push(Span::raw(" time"));
    }
    if let Some(query) = &view.search {
        let tail: String = query
            .chars()
            .rev()
            .take(area.width.saturating_sub(37) as usize)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        footer = vec![value(
            format!("Search: {tail}_ | Enter apply | Esc cancel"),
            config,
            Color::Yellow,
        )];
    }
    frame.render_widget(Paragraph::new(Line::from(footer)), regions[2]);
}

fn process_uptime(state: &State, now: Instant) -> String {
    state
        .number("uptime")
        .map(|seconds| {
            let age = if state.connected {
                state
                    .metrics
                    .get("uptime")
                    .map_or(0, |m| now.saturating_duration_since(m.updated).as_secs())
            } else {
                0
            };
            duration(seconds.saturating_add(age))
        })
        .unwrap_or_else(|| "N/A".into())
}
fn dashboard(frame: &mut Frame<'_>, state: &State, area: Rect, config: &Config, view: &View) {
    let accounting = state.number("accounting/enabled") == Some(1);
    let compact = area.height < 22;
    let stats_height = if accounting { 8 } else { 6 };
    let stats_height = stats_height.min(area.height.saturating_sub(5));
    let bottom_height = if !compact {
        4
    } else if !state.connected
        || view.notice.is_some()
        || state
            .logs
            .iter()
            .any(|l| matches!(l.level.as_str(), "WARN" | "ERR"))
    {
        1
    } else {
        0
    };
    let parts = Layout::vertical([
        Constraint::Length(stats_height),
        Constraint::Min(2),
        Constraint::Length(bottom_height),
    ])
    .split(area);
    bandwidth(frame, state, parts[0], config, view, compact);
    let desired = view.preferences.graph_height.saturating_mul(2);
    let graph_height = desired.min(parts[1].height);
    let graphs_and_spare =
        Layout::vertical([Constraint::Length(graph_height), Constraint::Min(0)]).split(parts[1]);
    let graphs = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(graphs_and_spare[0]);
    charts::render(
        frame,
        state,
        graphs[0],
        config,
        view.preferences.history_seconds,
        false,
        view.now(),
    );
    charts::render(
        frame,
        state,
        graphs[1],
        config,
        view.preferences.history_seconds,
        true,
        view.now(),
    );
    if parts[2].height == 0 {
        return;
    }
    if parts[2].height == 1 {
        let count = state
            .connected_count
            .map_or_else(|| "N/A".into(), |n| n.to_string());
        let mut summary = vec![
            label("OR ", config),
            value(count, config, Color::Cyan),
            label(" | Port ", config),
            Span::raw(state.port_label()),
        ];
        if !state.connected {
            summary.push(value(format!(" | {}", state.status), config, Color::Red));
        } else if let Some(message) = &view.notice {
            summary.push(value(format!(" | {message}"), config, Color::Yellow));
        } else if let Some(entry) = state
            .logs
            .iter()
            .rev()
            .find(|l| matches!(l.level.as_str(), "WARN" | "ERR"))
        {
            summary.push(value(
                format!(" | {} {}", entry.level, entry.message),
                config,
                Color::Yellow,
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(summary)), parts[2]);
        return;
    }
    let bottom = Layout::horizontal([
        Constraint::Length(if area.width >= 100 { 30 } else { 23 }),
        Constraint::Min(1),
    ])
    .split(parts[2]);
    let count = state
        .connected_count
        .map_or_else(|| "N/A".into(), |n| n.to_string());
    let port = state.port_label();
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                label("Connected: ", config),
                value(count, config, Color::Cyan),
            ]),
            Line::from(vec![label("OR port: ", config), Span::raw(port)]),
        ])
        .block(block(" OR ", config)),
        bottom[0],
    );
    let latest: Vec<_> = state
        .logs
        .iter()
        .rev()
        .filter(|l| matches!(l.level.as_str(), "WARN" | "ERR"))
        .take(bottom[1].height.saturating_sub(2) as usize)
        .map(|l| log_line(l, config))
        .collect();
    let lines = if !state.connected {
        vec![Line::from(value(state.status.clone(), config, Color::Red))]
    } else if let Some(message) = &view.notice {
        vec![Line::from(value(message.clone(), config, Color::Yellow))]
    } else if latest.is_empty() {
        vec![Line::from(label("No warnings received", config))]
    } else {
        latest
    };
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block(" Notices ", config)),
        bottom[1],
    );
}

fn bandwidth(
    frame: &mut Frame<'_>,
    state: &State,
    area: Rect,
    config: &Config,
    view: &View,
    compact: bool,
) {
    let total = |key: &str| state.number(key).map_or_else(|| "N/A".to_string(), bytes);
    let mut lines = vec![
        Line::from(vec![
            label("Observed: ", config),
            value(rate(state.number("observed")), config, Color::Cyan),
            label(" | Advertised: ", config),
            value(rate(state.number("advertised")), config, Color::Green),
            label(" | Consensus: ", config),
            value(rate(state.number("consensus")), config, Color::Magenta),
        ]),
        Line::from(vec![
            label("Process totals: RX ", config),
            value(total("traffic/read"), config, Color::Cyan),
            label(" | TX ", config),
            value(total("traffic/written"), config, Color::Green),
        ]),
    ];
    if compact && let Some(line) = lines.last_mut() {
        line.spans.push(label(" | OR ", config));
        line.spans.push(value(
            state
                .connected_count
                .map_or_else(|| "N/A".into(), |n| n.to_string()),
            config,
            Color::Cyan,
        ));
    }
    if state.number("accounting/enabled") == Some(1) {
        let pair = state.pair("accounting/bytes");
        let reset = state
            .accounting_reset(view.paused_wall.unwrap_or_else(SystemTime::now))
            .map(duration)
            .unwrap_or_else(|| "N/A".into());
        lines.push(Line::from(vec![
            label("Period: RX ", config),
            value(
                pair.map_or_else(|| "N/A".into(), |v| bytes(v.0)),
                config,
                Color::Cyan,
            ),
            label(" | TX ", config),
            value(
                pair.map_or_else(|| "N/A".into(), |v| bytes(v.1)),
                config,
                Color::Green,
            ),
            label(" | Quota ", config),
            value(total("AccountingMax"), config, Color::Yellow),
            label(format!(" ({})", state.metric("AccountingRule")), config),
        ]));
        let left = state.pair("accounting/bytes-left");
        lines.push(Line::from(vec![
            label("Period reset in ", config),
            value(reset, config, Color::Yellow),
            label(" | Remaining RX / TX: ", config),
            value(
                left.map_or_else(
                    || "N/A".into(),
                    |v| format!("{} / {}", bytes(v.0), bytes(v.1)),
                ),
                config,
                Color::Cyan,
            ),
        ]));
    }
    let counts_ink = if state.dropped_events != 0 || state.malformed_events != 0 {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    let counters = Line::from(vec![
        label(
            format!(
                "Window {} | ",
                charts::window_label(view.preferences.history_seconds)
            ),
            config,
        ),
        value(
            format!(
                "Dropped {} | Malformed {}",
                state.dropped_events, state.malformed_events
            ),
            config,
            counts_ink,
        ),
    ]);
    let resources = &state.resources;
    let stale = !state.connected
        || resources
            .sampled
            .is_none_or(|at| view.now().saturating_duration_since(at).as_secs() > 10);
    let mut usage = vec![
        label("Tor process usage: CPU ", config),
        value(
            resources
                .cpu
                .filter(|_| !stale)
                .map_or_else(|| "N/A".into(), |v| format!("{v:.1}%")),
            config,
            Color::Cyan,
        ),
        label(" | RAM ", config),
        value(
            resources.rss.filter(|_| !stale).map_or_else(
                || "N/A".into(),
                |bytes| format!("{:.2} GiB", bytes as f64 / 1_073_741_824.0),
            ),
            config,
            Color::Magenta,
        ),
    ];
    if let Some((used, limit)) = resources
        .descriptors
        .zip(resources.descriptor_limit)
        .filter(|_| !stale)
    {
        let (ink, pressure) = if limit > 0 && used as f64 / limit as f64 >= 0.9 {
            (Color::Red, " CRITICAL")
        } else if limit > 0 && used as f64 / limit as f64 >= 0.6 {
            (Color::Yellow, " HIGH")
        } else {
            (Color::Green, "")
        };
        usage.extend([
            label(" | FD ", config),
            value(format!("{used}/{limit}{pressure}"), config, ink),
        ]);
    }
    lines.push(Line::from(usage));
    lines.push(counters);
    frame.render_widget(
        Paragraph::new(lines).block(block(" Bandwidth ", config)),
        area,
    );
}

fn log_line<'a>(entry: &'a crate::model::LogEntry, config: &Config) -> Line<'a> {
    let seconds = entry.seconds % 86400;
    Line::from(vec![
        label(
            format!(
                "{:02}:{:02}:{:02}Z ",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            ),
            config,
        ),
        value(
            format!("{:<6} ", entry.level),
            config,
            match entry.level.as_str() {
                "ERR" => Color::Red,
                "WARN" => Color::Yellow,
                _ => Color::Cyan,
            },
        ),
        Span::raw(entry.message.as_str()),
    ])
}
fn logs(frame: &mut Frame<'_>, state: &State, area: Rect, config: &Config, view: &View) {
    let parts = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(if area.height >= 12 { 5 } else { 4 }),
    ])
    .split(area);
    let groups = filters::logs(state, view.query(), view.severity, !view.show_repeats);
    let visible = parts[0].height.saturating_sub(2) as usize;
    let offset = view.scroll.min(groups.len().saturating_sub(visible));
    let mut lines: Vec<_> = groups
        .iter()
        .skip(offset)
        .take(visible)
        .map(|g| {
            let mut line = log_line(g.entry, config);
            if g.count > 1 {
                // Put repetition metadata before the message so it survives truncation.
                line.spans.insert(
                    2,
                    value(
                        format!(
                            "[x{} since {:02}:{:02}:{:02}Z] ",
                            g.count,
                            g.first % 86400 / 3600,
                            g.first / 60 % 60,
                            g.first % 60
                        ),
                        config,
                        Color::Magenta,
                    ),
                );
            }
            line
        })
        .collect();
    if groups.is_empty() {
        lines.push(Line::from(label(
            if state.logs.is_empty() {
                "No events received yet"
            } else {
                "No matching events"
            },
            config,
        )));
    }
    frame.render_widget(
        Paragraph::new(lines).block(block(" Events ", config)),
        parts[0],
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!(
                "/ search | e severity: {} | d repeats: {} | Esc clear",
                ["all", "WARN+ERR", "ERR"][view.severity as usize],
                if view.show_repeats {
                    "expanded"
                } else {
                    "grouped"
                }
            )),
            Line::from(format!(
                "Filter: {} | {} matches / {} retained events",
                if view.query().is_empty() {
                    "(none)"
                } else {
                    view.query()
                },
                groups.iter().map(|g| g.count).sum::<usize>(),
                state.logs.len()
            )),
            Line::from("Newest first | UTC receipt time | Up/PgUp older, Down/PgDn newer"),
        ])
        .style(color(config, Color::DarkGray))
        .block(block(" Information ", config)),
        parts[1],
    );
}

fn peer_parts(peer: &str) -> (String, String) {
    if let Some(identity) = peer.strip_prefix('$') {
        if let Some((fingerprint, nickname)) = identity.split_once(['=', '~']) {
            return (nickname.to_string(), fingerprint.to_string());
        }
        return ("N/A".into(), identity.to_string());
    }
    ("N/A".into(), peer.to_string())
}
fn connections(frame: &mut Frame<'_>, state: &State, area: Rect, config: &Config, view: &View) {
    let parts = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(if area.height >= 12 { 5 } else { 4 }),
    ])
    .split(area);
    let wide = area.width >= 78;
    let count = state
        .connection_count
        .map_or_else(|| "N/A".into(), |n| n.to_string());
    let connected = state
        .connected_count
        .map_or_else(|| "N/A".into(), |n| n.to_string());
    let neighbors = filters::connections(state, view.query(), view.sort, view.reverse);
    let visible = parts[0].height.saturating_sub(3) as usize;
    let rows = neighbors
        .iter()
        .skip(view.scroll.min(neighbors.len().saturating_sub(visible)))
        .take(visible)
        .map(|neighbor| {
            let ink = match neighbor.status {
                "CONNECTED" => Color::Green,
                "LAUNCHED" | "NEW" => Color::Yellow,
                "FAILED" | "CLOSED" => Color::Red,
                _ => Color::Gray,
            };
            if wide {
                Row::new(vec![
                    Cell::from(neighbor.status).style(color(config, ink)),
                    Cell::from(neighbor.name.as_str()).style(color(config, Color::Cyan)),
                    Cell::from(neighbor.identity.as_str()),
                ])
            } else {
                Row::new(vec![
                    Cell::from(neighbor.status).style(color(config, ink)),
                    Cell::from(neighbor.peer),
                ])
            }
        });
    let widths = if wide {
        vec![
            Constraint::Length(12),
            Constraint::Length(20),
            Constraint::Min(8),
        ]
    } else {
        vec![Constraint::Length(12), Constraint::Min(8)]
    };
    let headings = if wide {
        vec!["STATUS", "RELAY NAME", "FINGERPRINT / ENDPOINT"]
    } else {
        vec!["STATUS", "PEER"]
    };
    let title = format!(
        " OR connections | {connected} connected / {count} listed{} ",
        if state.connected { "" } else { " | STALE" }
    );
    frame.render_widget(
        Table::new(rows, widths)
            .header(Row::new(headings).style(strong(config, Color::White)))
            .column_spacing(1)
            .block(block(title, config)),
        parts[0],
    );
    let mut info = vec![
        Line::from(format!(
            "Source: GETINFO orconn-status | Snapshot: {} | {} rows omitted",
            state.metrics.get("orconn-source").map_or_else(
                || "N/A".into(),
                |m| format!(
                    "{}s ago",
                    view.now().saturating_duration_since(m.updated).as_secs()
                )
            ),
            state.omitted_connections
        )),
        Line::from(format!(
            "/ search | s sort: {} {} | r reverse | Esc clear | {} matches",
            view.sort.label(),
            if view.reverse { "desc" } else { "asc" },
            neighbors.len()
        )),
        Line::from(format!(
            "Filter: {} | OR neighbors only; no complete transit circuit paths.",
            if view.query().is_empty() {
                "(none)"
            } else {
                view.query()
            }
        )),
    ];
    if state.connection_count.is_none() {
        info[0] = Line::from("N/A: Tor did not provide an OR connection snapshot.");
    } else if neighbors.is_empty() {
        info[0] = Line::from(if state.connections.is_empty() {
            "No OR neighbors in the latest snapshot."
        } else {
            "No matching OR neighbors."
        });
    }
    frame.render_widget(
        Paragraph::new(info)
            .style(color(config, Color::DarkGray))
            .block(block(" Information ", config)),
        parts[1],
    );
}

fn help(frame: &mut Frame<'_>, state: &State, area: Rect, config: &Config, view: &View) {
    let mut lines = vec![
        Line::from("q/Ctrl-C quit | 1 dashboard | 2 logs | 3 OR | h/? help"),
        Line::from(
            "p pause/resume display; collection continues. All pages use the frozen snapshot.",
        ),
        Line::from(
            "Logs / search (case-insensitive text), Enter apply, Esc cancel/clear; max 128 characters.",
        ),
        Line::from(
            "Logs e cycles all / WARN+ERR / ERR; d groups/expands consecutive identical events.",
        ),
        Line::from("OR / searches name, identity, endpoint or status; s sorts; r reverses order."),
        Line::from(format!("Tor version: {}", state.metric("version"))),
        Line::from(format!(
            "Version status reported by Tor: {}",
            state.metric("status/version/current")
        )),
        Line::from(
            "(UP) green: recommended; red: old/obsolete/unrecommended. NEW: newer series; ?: unknown.",
        ),
        Line::from(
            "This reflects Tor's local consensus recommendations, not the newest Debian package.",
        ),
        Line::from(format!(
            "All reported consensus flags: {}",
            state.metric("flags")
        )),
        Line::from(
            "IPv4/IPv6 are Tor's own external-address guesses; missing addresses say not detected.",
        ),
        Line::from(
            "Session: received BW event bytes since monitor attachment in this run; preserved on reconnect.",
        ),
        Line::from(
            "Traffic during a control disconnect cannot be counted; session totals are not persisted.",
        ),
        Line::from("[ / ] decrease/increase graph height; - / + shorten/lengthen time window"),
        Line::from("Display settings save automatically in ~/.config/tor-relay-monitor/ui.toml"),
        Line::from("XDG_CONFIG_HOME overrides ~/.config. Terminal size may limit graph height."),
        Line::from("Up/PgUp: older/lower rows; Down/PgDn: newer/upper; Home: reset"),
        Line::from(
            "Graphs: seconds to now, shared automatic B/s scale; zero-height bars have no dots.",
        ),
        Line::from(
            "Sub-second BW scheduling jitter is aligned; larger missing intervals stay blank.",
        ),
        Line::from(
            "Port is the advertised OR port from Tor's descriptor/consensus; no reachability probe.",
        ),
        Line::from(format!(
            "Configured ORPort (may include local-only listeners): {}",
            state.metric("ORPort")
        )),
        Line::from("KiB = 1024 bytes; windows: 30s, 60s, 90s, 120s, 1h, 24h (- / +)."),
        Line::from("1h/24h use minute aggregates of observed samples. Empty minutes stay blank."),
        Line::from(
            "History fills from attachment, survives control reconnects, and is not saved to disk.",
        ),
        Line::from("Tor CPU: 100% = one logical CPU; RAM is RSS. /proc sampled every 5 seconds."),
        Line::from(
            "FD: open file descriptors (files, sockets, pipes) / soft process limit; not an exit policy.",
        ),
        Line::from(
            "FD HIGH >=60%, CRITICAL >=90%. Hidden when usage/limit is missing or stale; no elevation required.",
        ),
        Line::from(
            "Tor process usage precedes Window, Dropped and Malformed. OR: connected neighbors.",
        ),
        Line::from(format!(
            "Resource status: {}",
            if !state.connected {
                "unavailable while disconnected"
            } else if state.resources.sampled.is_some_and(|at| view
                .now()
                .saturating_duration_since(at)
                .as_secs()
                > 10)
            {
                "sample older than ten seconds; waiting for refresh"
            } else if state.resources.reason.is_empty() {
                "available"
            } else {
                &state.resources.reason
            }
        )),
        Line::from("Process totals reset when Tor restarts, not at midnight."),
        Line::from("Period totals/reset countdown use Tor accounting, only when enabled."),
        Line::from(format!(
            "Accounting schedule: {} | Rule: {} (max / sum / in / out)",
            state.metric("AccountingStart"),
            state.metric("AccountingRule")
        )),
        Line::from(
            "A Tor accounting period can be daily, weekly or monthly; no quota is invented.",
        ),
        Line::from(format!(
            "Accounting UTC interval: {} -> {}",
            state.metric("accounting/interval-start"),
            state.metric("accounting/interval-end")
        )),
        Line::from(format!(
            "Accounting remaining RX/TX: {}",
            state.pair("accounting/bytes-left").map_or_else(
                || "N/A".into(),
                |v| format!("{} / {}", bytes(v.0), bytes(v.1))
            )
        )),
        Line::from(format!(
            "Global rate/burst: {} / {}",
            rate(state.number("BandwidthRate")),
            rate(state.number("BandwidthBurst"))
        )),
        Line::from(format!(
            "Relay rate/burst: {} / {} (0 = inherit)",
            rate(state.number("RelayBandwidthRate")),
            rate(state.number("RelayBandwidthBurst"))
        )),
        Line::from(format!(
            "Coalesced BW intervals: {} (weighted averages).",
            state.aggregated_samples
        )),
        Line::from("No BW for >3 seconds: N/A. Disconnected snapshots remain stale."),
        Line::from("Observed: descriptor estimate. Advertised: min(avg, burst, observed)."),
        Line::from("Consensus: weight *1000 B/s; not current throughput or a rate cap."),
        Line::from(format!(
            "Descriptor publication (UTC): {}",
            state.metric("descriptor-published")
        )),
        Line::from("Continuous network uptime, transit paths and MetricsPort are unavailable."),
        Line::from(format!("Control status: {}", state.status)),
        Line::from(format!(
            "Display preferences: {}",
            view.notice
                .as_deref()
                .unwrap_or("loaded/default; changes save automatically")
        )),
        Line::from("Missing metrics:"),
    ];
    for (key, metric) in &state.metrics {
        if metric.value.is_none() {
            lines.push(Line::from(format!("{key}: N/A - {}", metric.reason)));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((view.scroll.min(u16::MAX as usize) as u16, 0))
            .block(block(" Help ", config)),
        area,
    );
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
}
struct TerminalGuard;
impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let guard = Self;
        execute!(io::stdout(), EnterAlternateScreen, Hide)?;
        Ok(guard)
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

pub async fn run(mut snapshots: watch::Receiver<Arc<State>>, config: &Config) -> Result<()> {
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        old_hook(info);
    }));
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut input = EventStream::new();
    let mut view = View::default();
    let preferences_path = Preferences::path();
    if let Some(path) = &preferences_path {
        match Preferences::load(path).await {
            Ok(preferences) => view.preferences = preferences,
            Err(error) => view.notice = Some(format!("Display settings: {error:#}")),
        }
    }
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let termination = async {
        #[cfg(unix)]
        {
            terminate.recv().await;
        }
        #[cfg(not(unix))]
        {
            std::future::pending::<()>().await;
        }
    };
    tokio::pin!(termination);
    let mut frozen: Option<Arc<State>> = None;
    let mut repaint = true;
    loop {
        if snapshots.has_changed().is_err() {
            break;
        }
        let latest = snapshots.borrow_and_update().clone();
        let state = frozen.as_ref().unwrap_or(&latest).clone();
        if repaint {
            terminal.draw(|frame| render(frame, &state, &view, config))?;
        }
        repaint = true;
        tokio::select! {
            event = input.next() => match event {
                Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => match view.key(key) {
                    Action::Quit => break,
                    Action::Pause => view.toggle_pause(&state, &mut frozen),
                    Action::Save => {
                        view.notice = match &preferences_path {
                            Some(path) => match view.preferences.save(path).await {
                                Ok(()) => None,
                                Err(error) => Some(format!("Cannot save display settings: {error:#}")),
                            },
                            None => Some("Display settings apply for this session; HOME/XDG_CONFIG_HOME unavailable".into()),
                        };
                    }
                    Action::Continue => {},
                },
                Some(Err(error)) => return Err(error.into()),
                None => break,
                _ => {},
            },
            _ = tick.tick() => repaint = view.paused_at.is_none(),
            signal = tokio::signal::ctrl_c() => { signal?; break; },
            _ = &mut termination => break,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    fn fixture() -> State {
        let mut state = State {
            connected: true,
            status: "Connected (SAFECOOKIE)".into(),
            ..State::default()
        };
        for (key, text) in [
            ("Nickname", "ExampleRelay"),
            ("version", "0.4.9.16 (git-test)"),
            ("process/pid", "1234"),
            ("uptime", "90061"),
            ("address/v4", "192.0.2.10"),
            ("address/v6", "2001:db8::10"),
            ("status/version/current", "recommended"),
            ("fingerprint", "0123456789ABCDEF0123456789ABCDEF01234567"),
            ("flags", "Running Valid Fast Stable Guard HSDir"),
            ("observed", "10485760"),
            ("advertised", "8388608"),
            ("consensus", "8000000"),
            ("traffic/read", "10737418240"),
            ("traffic/written", "12884901888"),
            ("ORPort", "9001"),
            ("descriptor-orport", "443"),
        ] {
            state.set(key, Some(text.into()), "");
        }
        let now = std::time::Instant::now();
        for age in (0..120).rev() {
            state.history.push_back(crate::model::Sample {
                read: (2 + age % 5) * 1048576,
                written: (3 + age % 7) * 1048576,
                seconds: 1,
                received: now - Duration::from_secs(age),
            });
        }
        state.session_read = 1073741824;
        state.session_written = 2147483648;
        state.or_connections(Some("$0123456789ABCDEF0123456789ABCDEF01234567=NeighborOne CONNECTED\n$ABCDEF0123456789ABCDEF0123456789ABCDEF01~NeighborTwo LAUNCHED"));
        state
    }
    fn text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .chunks(terminal.backend().buffer().area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn compact_header_and_full_width_graphs_have_real_axes() {
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|f| render(f, &fixture(), &View::default(), &Config::default()))
            .unwrap();
        let rendered = text(&terminal);
        assert!(
            rendered.contains("Relay name: ExampleRelay | V.0.4.9.16 (UP) | PID: 1234 | Uptime:")
        );
        assert!(rendered.contains("IPv4: 192.0.2.10") && rendered.contains("IPv6: 2001:db8::10"));
        assert!(rendered.contains("Port: 443"));
        assert!(rendered.contains("Flags: Running, Valid, Fast, Stable, Guard, HSDir"));
        assert!(!rendered.contains("read only"));
        assert!(!rendered.contains("Connected (SAFECOOKIE)"));
        assert!(!rendered.contains("Reset: Tor restart"));
        assert!(rendered.find("Bandwidth").unwrap() < rendered.find("Download").unwrap());
        assert!(rendered.contains("-120s") && rendered.contains("-60s") && rendered.contains("0s"));
        let bottom = rendered.lines().last().unwrap();
        assert!(bottom.contains("quit") && !bottom.contains("Dropped"));
        assert_eq!(rendered.matches("height").count(), 1);
        assert!(rendered.contains("Download:") && rendered.contains("Upload:"));
        assert_eq!(rendered.matches("session: ").count(), 2);
        assert!(rendered.contains("session: 1.0 GiB") && rendered.contains("session: 2.0 GiB"));
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .any(|cell| cell.fg == Color::Cyan)
        );
        if let Ok(directory) = std::env::var("TRM_RENDER_DIR") {
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                std::path::Path::new(&directory).join("dashboard-120x40.txt"),
                rendered,
            )
            .unwrap();
        }
    }
    #[test]
    fn header_always_keeps_both_address_families_and_additional_flags() {
        for width in [44, 60, 80, 120, 200] {
            let mut state = fixture();
            state.metrics.remove("address/v4");
            state.metrics.remove("address/v6");
            state.set("flags", Some("Authority Fast Guard HSDir MiddleOnly NoEdConsensus Running Stable StaleDesc V2Dir Valid FutureFlag".into()), "");
            let lines = header_lines(&state, &Config::default(), width, Instant::now());
            let rendered: String = lines
                .iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(rendered.contains("IPv4: not detected"));
            assert!(rendered.contains("IPv6: not detected"));
            for flag in state.metric("flags").split_whitespace() {
                assert!(rendered.contains(flag));
            }
            state.set(
                "address/v6",
                Some("2001:db8:1234:5678:90ab:cdef:1234:5678".into()),
                "",
            );
            let rendered = header_lines(&state, &Config::default(), width, Instant::now())
                .iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(rendered.contains("IPv6:"));
            assert!(rendered.contains("2001:db8:1234:5678:90ab:cdef:1234:5678"));
        }
    }
    #[test]
    fn version_badges_do_not_guess_recommendations_for_unknown_or_new_versions() {
        let mut state = fixture();
        let config = Config::default();
        for (status, expected, ink) in [
            ("recommended", "(UP)", Color::Green),
            ("old", "(UP)", Color::Red),
            ("obsolete", "(UP)", Color::Red),
            ("unrecommended", "(UP)", Color::Red),
            ("new", "(NEW)", Color::Cyan),
            ("new in series", "(NEW)", Color::Cyan),
            ("unknown", "(?)", Color::DarkGray),
            ("future-state", "(?)", Color::DarkGray),
        ] {
            state.set("status/version/current", Some(status.into()), "");
            assert_eq!(version_badge(&state, &config), (expected, ink));
        }
        state.connected = false;
        assert_eq!(version_badge(&state, &config).0, "(?)");
    }
    #[test]
    fn or_view_separates_peer_columns_and_information() {
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        let view = View {
            page: 2,
            ..View::default()
        };
        let config = Config {
            no_color: true,
            ..Config::default()
        };
        terminal
            .draw(|f| render(f, &fixture(), &view, &config))
            .unwrap();
        let rendered = text(&terminal);
        assert!(rendered.contains("RELAY NAME") && rendered.contains("FINGERPRINT / ENDPOINT"));
        assert!(rendered.contains("NeighborOne") && rendered.contains("NeighborTwo"));
        assert!(rendered.find("Information").unwrap() > rendered.find("NeighborTwo").unwrap());
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .all(|cell| cell.fg == Color::Reset)
        );
        if let Ok(directory) = std::env::var("TRM_RENDER_DIR") {
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                std::path::Path::new(&directory).join("connections-120x30.txt"),
                rendered,
            )
            .unwrap();
        }
    }
    #[test]
    fn flag_separators_keep_a_neutral_color_even_when_flags_wrap() {
        for width in [44, 120] {
            let mut terminal = Terminal::new(TestBackend::new(width, 40)).unwrap();
            terminal
                .draw(|frame| render(frame, &fixture(), &View::default(), &Config::default()))
                .unwrap();
            let commas: Vec<_> = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .filter(|cell| cell.symbol() == ",")
                .collect();
            assert!(!commas.is_empty());
            assert!(commas.iter().all(|cell| cell.fg == Color::DarkGray));
        }
    }
    #[test]
    fn search_input_is_bounded_cancelable_and_does_not_trigger_shortcuts() {
        let mut view = View {
            page: 1,
            ..View::default()
        };
        let key = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        view.key(key('/'));
        for c in "qph123".chars() {
            assert_eq!(view.key(key(c)), Action::Continue);
        }
        assert_eq!(view.page, 1);
        assert!(!view.help);
        assert_eq!(view.search.as_deref(), Some("qph123"));
        view.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(view.log_query, "qph123");
        view.key(key('/'));
        for _ in 0..200 {
            view.key(key('é'));
        }
        assert_eq!(view.search.as_ref().unwrap().chars().count(), 128);
        view.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(view.log_query, "qph123");
        view.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(view.log_query.is_empty());
        view.key(key('e'));
        assert_eq!(view.severity, 1);
        view.key(key('d'));
        assert!(view.show_repeats);
        view.key(key('3'));
        view.key(key('s'));
        view.key(key('r'));
        assert_eq!(view.sort, filters::Sort::Name);
        assert!(view.reverse);
        assert_eq!(view.key(key('q')), Action::Quit);
    }
    #[tokio::test]
    async fn pause_freezes_snapshot_timers_and_graphs_while_watch_updates_continue() {
        let original = Arc::new(fixture());
        let (sender, mut receiver) = watch::channel(original.clone());
        let mut view = View::default();
        let mut frozen = None;
        view.toggle_pause(&original, &mut frozen);
        let at = view.now();
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|f| render(f, frozen.as_ref().unwrap(), &view, &Config::default()))
            .unwrap();
        let before = terminal.backend().buffer().clone();
        let mut updated = (*original).clone();
        updated.session_read += 999;
        updated.log("ERR", "Event received while display was paused");
        sender.send_replace(Arc::new(updated));
        tokio::time::sleep(Duration::from_millis(20)).await;
        let latest = receiver.borrow_and_update().clone();
        assert_eq!(latest.session_read, original.session_read + 999);
        assert_eq!(view.now(), at);
        terminal
            .draw(|f| render(f, frozen.as_ref().unwrap(), &view, &Config::default()))
            .unwrap();
        assert_eq!(*terminal.backend().buffer(), before);
        assert!(text(&terminal).contains("PAUSED"));
        view.toggle_pause(&latest, &mut frozen);
        assert!(frozen.is_none() && view.paused_at.is_none());
        view.page = 1;
        terminal
            .draw(|f| render(f, &latest, &view, &Config::default()))
            .unwrap();
        assert!(text(&terminal).contains("Event received while display was paused"));
    }
    #[test]
    fn resources_filters_and_long_windows_render_with_honest_missing_states() {
        let mut state = fixture();
        state.resources = crate::resources::Resources {
            cpu: Some(123.4),
            rss: Some(104857600),
            descriptors: Some(950),
            descriptor_limit: Some(1000),
            sampled: Some(Instant::now()),
            reason: String::new(),
        };
        state.log("WARN", "Repeated warning");
        state.log("WARN", "Repeated warning");
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        let mut view = View::default();
        terminal
            .draw(|f| render(f, &state, &view, &Config::default()))
            .unwrap();
        let output = text(&terminal);
        assert!(
            output.contains("Tor process usage: CPU 123.4% | RAM 0.10 GiB | FD 950/1000 CRITICAL")
        );
        assert!(
            output
                .find("Window 120s | Dropped 0 | Malformed 0")
                .unwrap()
                > output.find("Tor process usage:").unwrap()
        );
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .any(|c| c.symbol() == "9" && c.fg == Color::Red)
        );
        state.resources.descriptors = None;
        state.resources.descriptor_limit = Some(65536);
        terminal
            .draw(|f| render(f, &state, &view, &Config::default()))
            .unwrap();
        let unavailable = text(&terminal);
        assert!(unavailable.contains("Tor process usage: CPU 123.4% | RAM 0.10 GiB"));
        assert!(!unavailable.contains(" | FD ") && !unavailable.contains("N/A/65536"));
        state.resources.descriptors = Some(0);
        terminal
            .draw(|f| render(f, &state, &view, &Config::default()))
            .unwrap();
        assert!(text(&terminal).contains(" | FD 0/65536"));
        state.resources.descriptor_limit = None;
        terminal
            .draw(|f| render(f, &state, &view, &Config::default()))
            .unwrap();
        assert!(!text(&terminal).contains(" | FD "));
        state.resources.descriptor_limit = Some(65536);
        state.resources.sampled = Some(Instant::now() - Duration::from_secs(11));
        terminal
            .draw(|f| render(f, &state, &view, &Config::default()))
            .unwrap();
        assert!(!text(&terminal).contains(" | FD "));
        state.resources.sampled = Some(Instant::now());
        state.connected = false;
        terminal
            .draw(|f| render(f, &state, &view, &Config::default()))
            .unwrap();
        assert!(text(&terminal).contains("Tor process usage: CPU N/A | RAM N/A"));
        assert!(!text(&terminal).contains(" | FD "));
        assert!(!text(&terminal).contains("CRITICAL"));
        view.page = 1;
        view.log_query = "repeated".into();
        terminal
            .draw(|f| render(f, &state, &view, &Config::default()))
            .unwrap();
        assert!(text(&terminal).contains("[x2 since"));
        view.log_query = "unmatched".into();
        terminal
            .draw(|f| render(f, &state, &view, &Config::default()))
            .unwrap();
        assert!(text(&terminal).contains("No matching events"));
        view.page = 2;
        view.or_query = "unmatched".into();
        terminal
            .draw(|f| render(f, &state, &view, &Config::default()))
            .unwrap();
        assert!(text(&terminal).contains("No matching OR neighbors"));
        view.page = 0;
        view.preferences.history_seconds = 86400;
        terminal
            .draw(|f| render(f, &state, &view, &Config::default()))
            .unwrap();
        assert!(text(&terminal).contains("-24h") && text(&terminal).contains("-12h"));
    }
    #[test]
    fn compact_accounting_dashboard_keeps_both_axes_and_resources_visible() {
        let mut state = fixture();
        state.set("accounting/enabled", Some("1".into()), "");
        state.set(
            "address/v6",
            Some("2001:db8:1234:5678:abcd:ef01:2345:6789".into()),
            "",
        );
        state.set(
            "flags",
            Some("Fast Guard HSDir Running Stable V2Dir Valid AdditionalFlag AnotherFlag".into()),
            "",
        );
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| render(f, &state, &View::default(), &Config::default()))
            .unwrap();
        let output = text(&terminal);
        assert_eq!(output.matches("-120s").count(), 2, "{output}");
        assert!(
            output.contains("Tor process usage: CPU")
                && !output.contains(" | FD ")
                && output.contains("Window 120s | Dropped 0 | Malformed 0"),
            "{output}"
        );
        assert!(output.contains("Period: RX") && output.contains("OR 1"));
        assert!(output.find("Window 120s").unwrap() > output.find("Tor process usage:").unwrap());
        if let Ok(directory) = std::env::var("TRM_RENDER_DIR") {
            std::fs::write(
                std::path::Path::new(&directory).join("accounting-80x24.txt"),
                output,
            )
            .unwrap();
        }
    }
    #[test]
    fn renders_small_resized_and_normal_views_without_panics() {
        for (width, height) in [(1, 1), (43, 13), (44, 14), (80, 24), (160, 50)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for page in 0..3 {
                for help in [false, true] {
                    let config = Config {
                        no_color: true,
                        ascii: true,
                        ..Config::default()
                    };
                    terminal
                        .draw(|f| {
                            render(
                                f,
                                &State::default(),
                                &View {
                                    page,
                                    scroll: 999,
                                    help,
                                    ..View::default()
                                },
                                &config,
                            )
                        })
                        .unwrap();
                }
            }
        }
    }
}
