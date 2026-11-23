# Validation record

## Source review status

The source code, tests and documentation were developed extensively with AI
assistance. The maintainer defined requirements and performed functional testing,
but has not performed a technical source-code review. No independent human code
review or security audit has been performed. The automated checks and operational
reports below must not be read as evidence of such a review or as proof of security.

## Release validation — 2026-11-23

- The 49 Rust tests passed on Rust 1.99.1 and the declared minimum 1.88.0.
  Formatting, Clippy with warnings denied and optimized Linux builds passed.
- Three Python release-boundary tests passed on Windows/Python 3.13 and on
  Debian/Python 3.11, including explicit source roots and unsafe archive paths.
- Actionlint 1.7.11 validated all three workflow files locally. Its release
  binary was checked against the upstream checksum before use. Action commits
  were resolved from their official upstream tags. These checks were run locally;
  hosted GitHub CI was not part of this validation run.
- Cargo-audit 0.23.0 checked Cargo.lock against RustSec on 2026-11-23. Database
  commit: `52384ab5581a4487b62c549eef95cf7ba7e8bad5`; 1,563 advisories and 126
  lockfile packages were reported. No vulnerability, informational warning or
  ignore was reported. This is a time-bound advisory check, not a source audit.
- Declared license metadata was inventoried. Dependency-provided license/notice
  texts for the Linux x86_64 graph (116 dependencies, including test/build crates)
  are collected in THIRD_PARTY_NOTICES.md and checked by a regeneration script.
  This is not an independent legal review or coverage for unshipped platforms.
- Private offline Tor 0.4.9.16 was exercised under Linux UID 65534, with a
  temporary unprivileged Cargo cache/target and accounting enabled. Both TCP/Unix
  integration tests and diagnostics passed, as did TUI pause/search/navigation,
  preference reload, resize, q/SIGTERM and terminal restoration. No production
  Tor, permission or service configuration was changed.

These checks cover version 0.5.3 and its distribution tooling. No monitor runtime
behavior changed.

## Operator report — 2026-11-23

The operator reported approximately 24 hours of use with the monitor left open
and no observed issue. This follows the 0.5.3 test request; the exact executable
version, memory/CPU measurements, dropped/malformed counts and fault/restart cases
were not supplied with the report. It is qualitative evidence from one deployment,
not a measured multi-platform soak test or an independent security review.
No identifying relay details are included in this record.

Historical entries below describe the checks and limitations at each version's
validation date; this report does not retroactively change those test results.

## Version 0.5.3 — optional dashboard FD field

Validated on 2026-11-21 in the isolated Linux environment below. All 49 automated
tests, formatting, Clippy with warnings denied and optimized builds passed.
Existing render checks exercise available FD pressure, unavailable usage with
a readable 65536 limit, valid zero usage, missing limit, stale samples and
disconnects. Missing FD fields leave CPU/RAM, row order and graph layout intact.
The 80x24 accounting preview contains no unavailable FD placeholder. Archive
integrity and independent installation were checked. Collection and per-user
preferences are unchanged; no permission changes, MSRV or private-Tor rerun.

## Version 0.5.2 — process resources before counters

Validated on 2026-11-21 in the isolated Linux environment below. All 49 automated
tests, formatting, Clippy with warnings denied and optimized builds passed.
Render checks and previews verify `Tor process usage` immediately above the
Window/Dropped/Malformed row in large and compact dashboards, including 80x24
with accounting. Archive integrity and independent installation were checked.
No collector behavior changed; no new private-Tor or MSRV run was required.

## Version 0.5.1 — counter order and process resource labels

Validated on 2026-11-21 in the isolated Linux environment below.

- 40 unit tests, 8 scripted-control tests and 1 CLI compatibility test passed.
- Formatting, Clippy with warnings denied and optimized Linux builds passed.
- Existing render checks now assert a separate Window/Dropped/Malformed row before
  `Tor process usage`, RAM in GiB, descriptor colors and honest missing states.
  The 80x24 accounting preview retains both graph axes and the OR count.
- Source/archive integrity and independent executable installation were checked.

This display-only patch does not change resource collection, Tor commands or
permissions. The private-Tor and MSRV results for 0.5.0 remain recorded below;
those additional checks were not rerun for this patch.

## Version 0.5.0 — resources, long history and interactive inspection

Validated on 2026-11-21 using the isolated Linux/Rust/private Tor environment
described below. No production service or configuration was changed.

- 40 unit tests, 8 scripted-control tests and 1 CLI compatibility test passed.
  The declared Rust 1.88 minimum and Rust 1.99.1 were exercised.
- Formatting, Clippy with warnings denied and optimized Linux builds passed.
- Private offline Tor diagnostics and the opt-in integration test passed over
  TCP and Unix, with accounting and real `/proc` resource assertions enabled.
- Terminal smoke tests cover pause with no unsolicited redraws, resume, typing
  `q` inside search, severity/grouping/sort keys, 1h/24h window navigation,
  preferences reload, resize, q/SIGTERM and restoration of terminal attributes.
- Deterministic tests cover a full-day retention bound, partial minutes, byte
  preservation, delayed aggregates, gaps, minute-boundary scheduling jitter,
  weighted selected-window averages and long-window graph buckets.
- UI tests cover continued watch delivery while paused, frozen clocks/buffers,
  query bounds/cancel/commit, log grouping boundaries, case-insensitive searches,
  stable OR sorting, no matches, resource pressure/missing states and both graph
  axes at 80x24 with accounting, a long IPv6 address and wrapped flags.
- Resource tests exercise an actual local Linux process, two-sample CPU, changed
  start time, invalid/missing PID, complex process names and soft-limit parsing.
- Source/archive integrity, executable hash/mode, the relative compatibility
  alias and execution after removing the extracted source directory were checked.

The 24-hour boundary is exercised with deterministic timestamps, not a 24-hour
production soak. The real Tor fixture is offline with no OR traffic; nonempty OR
search/sort is tested with explicit fixtures. Restricted production `/proc`
permissions, host/container PID mismatches, busy-relay performance, exit-relay
behavior and extended operator-authorized production use remain unvalidated.
The sampler requires Tor's PID namespace and never changes permissions.

## Version 0.4.0 — TRM command and rendering corrections

Validated on 2026-11-21 in the same isolated Linux/Tor environment below.

- 26 unit tests, 8 scripted-control tests and 1 CLI compatibility test passed
  on Rust 1.99.1 and on the declared minimum Rust 1.88.0.
- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`
  and optimized builds of `trm` and `tor-relay-monitor` passed.
- `trm --diagnose` passed on real private offline Tor via TCP and Unix, with
  accounting enabled. TUI navigation, graph preferences/reload, resizing, q,
  SIGTERM and terminal restoration passed using the short command.
- Deterministic graph tests cover alternating 200 ms receipt jitter, a slow
  recurring timer without accumulated holes, real multi-second missing intervals,
  and zero traffic without decorative dots in ASCII and Unicode rendering.
- Render tests verify neutral flag commas at wide/narrow widths. Port tests
  distinguish advertised ports from local-only listeners, exercise descriptor
  priority/consensus fallback, IPv6 ports, missing data and invalid port ranges.
- Both command names report the correct name/version and share one lifecycle.
  The Linux archive uses `trm` plus a relative compatibility symlink.
- Archive CRC, executable hash/permissions and the compatibility link were
  verified. A copied executable still launched after the temporary extracted
  directory had been removed, confirming installation independence.

No system installation, service creation, production Tor change, dependency
vulnerability audit, sustained busy-relay test or 24-hour production soak was
performed. Nonempty published ports still need comparison on an operator's relay.
The MSRV gate is now verified; broader distro/exit-relay coverage and
an operator-authorized extended soak remain before a production-stability claim.
No new performance benchmark is claimed for this release.

## Version 0.3.0 — relay details and monitor-session totals

Validated on 2026-11-21 in the same Linux/Rust/Tor environment described below.

- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`,
  `cargo test --locked`, and `cargo build --release --locked` passed.
- 22 unit tests and 8 scripted-peer integration tests passed; the opt-in real-Tor
  test was additionally executed successfully over both TCP and Unix sockets.
- The real private offline Tor returned 551 for both external address guesses
  and `unknown` for its version recommendation. The monitor tolerated these
  values, retained explicit absence reasons, and used neutral display states.
- Unit tests cover valid, unspecified, malformed and wrong-family addresses,
  all known version-status categories, additional/unknown flags and narrow headers.
- Session sums are checked for aggregation without duplication, a fresh monitor
  state, and preservation across an actual scripted control reconnection.
- Render tests verify `PID:`, `Uptime:`, comma-separated flags, both IP families,
  separate RX/TX session titles, removed reset text and footer-only resize hints.
- Real release-mode TUI navigation, graph preference changes/reload, resizing,
  normal quit, SIGTERM and terminal restoration passed. No production service
  or Tor configuration was changed.
- The stripped Linux x86_64 executable is 2,002,312 bytes. No new loaded-relay
  benchmark or independent live-consensus recommendation/IP validation is claimed.

The version badge reflects local Tor consensus recommendations, not the newest
distribution package. Session sums count received BW events during this monitor
run; traffic during control disconnections is unknown. Earlier production soak,
exit-relay, MSRV and distribution coverage limitations remain.

## Version 0.2.0 — UI and accounting update

Validated on 2026-11-21 in the same Linux/Rust/Tor environment described below.

- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`,
  `cargo test --locked`, and `cargo build --release --locked` passed.
- 18 unit tests and 8 scripted-peer integration tests passed. The real-Tor test
  remains opt-in in the normal suite and was executed separately on both TCP
  and Unix, with accounting enabled and disabled.
- With a private offline Tor configured for a daily 1 GiB accounting period,
  the monitor retrieved the real accounting counters, bytes left, quota/rule,
  and UTC interval start/end. The countdown is tested against those timestamps.
- Release diagnostics, TUI navigation, 80x24 to 60x18 resize, `q`, SIGTERM,
  exact terminal restoration, interactive graph adjustments and preference
  persistence across application restarts passed with real Tor.
- TestBackend previews verify the compact header, statistics before graphs,
  colored/no-color rendering, full-width time buckets, separate missing/zero
  intervals, OR table columns, and bottom information panel. A test-only fixture
  supplies nonzero traffic and populated neighbors; production still uses Tor only.
- Full graph windows are built from elapsed-time buckets; source history is not
  invented or expanded with artificial zero samples.
- A 5.10-second idle offline-Tor sample showed 3596 KiB maximum sampled RSS and
  no observed increase in process CPU ticks. The tick resolution and short run
  make this unsuitable for a claim of zero CPU use or loaded-relay performance.
- The stripped Linux x86_64 release executable is 1,992,056 bytes.

The earlier production soak, exit-relay, MSRV and distro-coverage limitations
still apply. The operator has reported that version 0.1.0 works on their relay;
that report is not an independent production certification of version 0.2.0.
Only per-user graph preferences are persisted. No Tor configuration or existing
service was modified during this update.

## Version 0.1.0 — original MVP baseline

Run on 2026-11-21. These are observed results for the initial MVP, not a production
certification or a performance claim for a busy public relay.

## Environment

- Windows workspace; native Rust GNU build scripts were denied execution by the
  host. Native Windows compilation is not claimed to pass.
- Validation ran in a disposable `rust:1.99-bookworm` Linux container, using
  `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, x86_64 and glibc 2.36.
- Linux kernel: 6.18.58.1-microsoft-standard-WSL2. CPU reported by the container:
  Intel Core i5-13600KF. Results include container/virtualization effects.
- Real Tor: 0.4.9.16, git-c51547ca15ea6659, from the official Linux expert bundle
  16.0.1. SHA-256 matched the file published on dist.torproject.org. This was a
  checksum check over HTTPS; a detached OpenPGP signature was not independently
  verified.
- Tor ran with a private temporary configuration/data directory, `DisableNetwork 1`,
  `ClientOnly 1`, `SocksPort 0`, SAFECOOKIE, an automatic loopback ControlPort and
  a private Unix ControlSocket. No existing service or relay was modified.

## Executed checks

| Check | Observed result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo clippy --all-targets --locked -- -D warnings` | Passed without allowances |
| `cargo test --locked` | 12 unit tests and 8 scripted-peer integration tests passed; 1 opt-in real-Tor test ignored by default |
| `cargo build --release --locked` | Passed; final optimized executable also built and executed by the release harness |
| Real-Tor integration test, release, TCP | Passed using Tor-advertised cookie path |
| Real-Tor integration test, release, Unix socket | Passed using Tor-advertised cookie path |
| CLI `--diagnose`, release, TCP and Unix | Passed; exact Tor version, process metrics, configured rates, live zero BW events and honest N/A relay-only data |
| Real pseudo-terminal, release | Authenticated, navigated 2/3/help/1, resized from 80x24 to 60x18, then exited successfully |
| Terminal restoration on `q` and SIGTERM | Passed: termios attributes restored exactly and alternate-screen exit observed |
| TestBackend layouts | All three pages and help, including tiny terminals and scrolled views, rendered without panic |
| `cargo package --list --locked --allow-dirty` | Source allowlist excluded `.tools` and `target`; Cargo noted that public repository/homepage/documentation metadata is not configured |

Protocol tests include CRLF, single/continuation/data replies, dot stuffing,
asynchronous events before/between responses, future fields/events, malformed and
incomplete frames, disconnects, command deadlines, reconnect and prompt shutdown.
Authentication tests include a Python-derived known HMAC vector, valid proofs,
rejected server proofs without sending AUTHENTICATE, rejected unsafe methods,
short cookies, Unix symlinks, world-readable files and a nonblocking FIFO case.
State tests cover bytes/units, missing data, independent descriptor/consensus
values, bounded logs and OR rows, and cumulative malformed-event accounting.

## Measured samples

Commands:

```sh
python3 scripts/test_local_tor.py --tor /path/to/tor --release --measure-seconds 5
cargo test --release --locked --test control event_flood_is_bounded -- --nocapture
```

- Optimized monitor on a private pseudo-terminal with offline idle Tor: **5.00 s**,
  **0.20% of one CPU**, **3480 KiB maximum sampled RSS**. CPU uses Linux process
  ticks; RSS is periodically sampled, not a guaranteed peak. Tor's own memory is
  excluded. This short sample does not establish loaded-relay performance.
- In-memory scripted flood: **20,000 valid events in 3.43815 ms** (10,000 BW and
  10,000 NOTICE), plus one malformed BW event. Verified exactly 100,000 RX bytes
  and 200,000 TX bytes retained, 256 queued events, 9744 drops, and one malformed
  event counted. No terminal or real Tor participates in this parser/queue sample.
- Stripped optimized Linux executable: **1,744,536 bytes**. This build is dynamically
  linked; rebuild from source for a different ABI or older distribution.

Measurable regression criteria: preserve exact BW sums and drop counts for the
fixed flood, never exceed the declared queue/history limits, and flag a repeated
>2x slowdown versus this same-environment baseline for investigation. Before a
public production-readiness claim, measure 24-hour stability and memory growth on
an authorized relay, including reconnects, normal and peak event rates, and SSH
output volume. No such soak test was performed here.

## Remaining release gates and limitations

- Public Middle/Guard deployment, real published descriptor/consensus comparisons,
  nonempty live OR snapshots, and exit-specific behavior remain unvalidated.
- Test the declared Rust 1.88 minimum and other supported Debian/Ubuntu releases;
  this run establishes the tested newer stable toolchain only.
- No broad fuzz campaign, dependency vulnerability audit, or multi-day soak run
  is claimed. Input bounds and targeted malformed-input/stress tests passed.
- MetricsPort and local circuit details remain deferred. Full transit paths and
  official continuous network-presence uptime are not inferred.
- SIGKILL, host failure and broken SSH transports cannot guarantee terminal cleanup.

The full source, tests and docs are the deliverable; no package was published and
no production Tor configuration, system permission or service was changed.
