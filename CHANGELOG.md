# Changelog

Notable changes follow [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses semantic versioning.

## [Unreleased]

### Added

- Public disclosure of the project's personal-use origins and extensive AI
  assistance in design, code, tests, debugging and documentation, with an explicit
  statement that no human technical code review or independent security audit
  has been performed.
- Project motivation: modernizing the maintainer's Nyx-style monitoring experience
  after unexpected behavior and concerns about upstream maintenance, with a dated
  reference to Nyx's published release history.
- GitHub CI for Rust 1.88/stable, lint/build checks, release packaging and a private
  offline Tor child; scheduled RustSec auditing and Dependabot configuration.
- Security reporting policy, sanitized issue/PR templates and maintainer release
  documentation.
- Checked-in release/source packaging and verification scripts with explicit
  source boundaries, SHA256SUMS and independent-install checks.
- Collected dependency license/notice texts for the distributed Linux graph,
  with regeneration and CI consistency checks.
- Qualified operator feedback: approximately 24 hours without an observed issue,
  reported 2026-11-23, without measured performance or fault-injection results.

## [0.5.3] - 2026-11-21

### Changed

- Dashboard FD usage appears only when both count and limit are available and
  fresh. Restricted `/proc` access no longer leaves an N/A FD field in the UI.
  CPU/RAM, layout and per-user graph preferences remain independent; no elevated
  privileges or Tor permission changes are required.

## [0.5.2] - 2026-11-21

### Changed

- `Tor process usage` now appears immediately above Window/Dropped/Malformed
  counters in every dashboard layout.

## [0.5.1] - 2026-11-21

### Changed

- Window/dropped/malformed counters keep a separate row before process resources
  in compact layouts as well as large ones.
- Resource row now reads `Tor process usage: CPU ...% | RAM ... GiB | FD used/limit`.
  RAM uses two decimal places; help explains descriptors and partial availability.

## [0.5.0] - 2026-11-21

### Added

- Linux Tor CPU/RSS and open-descriptor/soft-limit sampling, with pressure colors,
  process-identity checks, bounded reads and explicit missing/stale states.
- In-memory 1h/24h bandwidth views using bounded minute aggregation, alongside
  existing detailed short windows. Long history survives control reconnects.
- Case-insensitive log search, severity filters and consecutive repeat grouping.
- Global display pause with frozen timers/graphs; collection continues and resume
  selects the latest snapshot without retaining a backlog.
- OR neighbor search, deterministic status/name/identity sorting and reverse order.
- Keyboard, retention, jitter, missing-data and real-process tests; expanded
  private-Tor terminal smoke coverage for pause, search and long windows.

### Changed

- Time-window keys now step through 30s, 60s, 90s, 120s, 1h and 24h. Existing
  valid short-window preferences remain readable; no metrics or searches persist.

## [0.4.0] - 2026-11-21

### Changed

- Primary command is `trm`; the longer `tor-relay-monitor` entry remains compatible.
  Both share one application implementation and retain existing preferences.
- Flag commas use a neutral color independently of flag colors.
- Removed decorative graph baseline dots and aligned sub-second BW scheduling
  jitter to prevent false vertical gaps while retaining larger missing intervals.

### Added

- Advertised OR port after the IP fields, with a distinct IPv6 port when needed,
  using own descriptor data and cached-consensus fallback.
- Regression tests for jitter, zero bars, flag punctuation, published-vs-local
  ports, and both CLI entry points.
- Archive installation, command-name collision and extracted-directory cleanup
  instructions; no service is created.

## [0.3.0] - 2026-11-21

### Changed

- Header labels use `PID:` and `Uptime:`. Flags are comma-separated, retain all
  reported additional flags, and wrap when needed.
- Removed duplicate graph-resize hints from Bandwidth and the process reset
  annotation; keyboard help and counter semantics remain documented.
- Download/upload headings show current rate, window average and monitor-session
  byte totals separately for each direction.

### Added

- Always-visible Tor-reported IPv4/IPv6, with `not detected` for missing addresses.
- Colored Tor recommendation badge alongside the version; unknown/newer states
  stay distinct and do not trigger external package/version lookups.
- Monitor-run BW counters preserved across reconnects and tests covering
  recommendation states, address validation, full flag display and session totals.

## [0.2.0] - 2026-11-21

### Changed

- Compact relay header: relay name, Tor version, PID, uptime, fingerprint and
  individually colored flags. Normal connection/read-only labels removed;
  disconnect warnings remain visible.
- Bandwidth statistics above larger graphs, colored totals and key hints,
  diagnostics counters outside the footer, and compact OR/notice panels.
- Full-width graphs with elapsed-time X axes, automatic shared B/s scale,
  configurable height and time window, and matching window averages.
- OR table with separate status/name/identity columns and bottom source details.

### Added

- Automatic per-user persistence of graph height/window preferences only.
- Read-only accounting period counters, quota/rule, remaining bytes and reset
  countdown when enabled, separate from process-lifetime totals.
- Tests for graph coverage/gaps, window averages, UTC accounting countdowns,
  preference persistence, compact headers and OR view layout.

## [0.1.0] - 2026-11-21

### Added

- Initial read-only Rust TUI with SAFECOOKIE, local Unix/TCP control access,
  bounded protocol framing, event separation and reconnect supervision.
- Live BW graphs and weighted history, process totals, relay descriptor/consensus
  bandwidth, flags, configured limits, and OR connection snapshots.
- In-memory severity-colored Tor events, diagnostics, help, scrolling,
  no-color/ASCII modes and terminal restoration.
- Unit, scripted-peer integration, event-flood and opt-in real-Tor tests.
- Build, installation, architecture, protocol, research, contributor and agent docs.

### Known limitations

- MetricsPort, local circuit details, continuous network uptime, published-relay
  soak testing and exit-specific validation are not included in the initial MVP.
- OR connections use periodic snapshots; no transit circuit paths are inferred.
