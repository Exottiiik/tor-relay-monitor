# TRM — Tor Relay Monitor

A small, read-only terminal monitor for one local Tor relay, inspired by Nyx.
Implemented in Rust for Linux servers and SSH terminals. Relay data comes from
the local Tor Control Protocol; process resources come from Linux `/proc`.
There is no web server, telemetry, reverse
DNS, external API, or persistent event log.

Version 0.5.3 is beta software. See [validation](docs/VALIDATION.md) for automated
checks, operator feedback, tested environments and remaining limitations.

This is an independent project, not an official Tor Project or Nyx release.
Security reports follow [SECURITY.md](SECURITY.md).

## Project origin and AI assistance

TRM started as a personal tool for monitoring the maintainer's own Tor relay.
The idea came from a desire to improve and modernize the Nyx-style monitoring
experience after encountering unexpected errors and behavior in the maintainer's
Nyx setup. The goal was a clearer interface and a focused, read-only workflow.

The maintainer was also concerned about the pace of upstream maintenance. As of
2026-11-23, [Nyx's official website](https://nyx.torproject.org/) still listed
version 2.1.0, released on January 12, 2019, as its latest release. This release
gap was part of the motivation for a separate implementation.

TRM is shared openly so other relay operators can inspect, use and improve it.

AI assistance was used extensively for the design, implementation, tests,
debugging and documentation. The published source includes substantial
AI-generated contributions. The maintainer's role has been to define the desired
behavior and test the tool in real use.

The maintainer has not performed a technical review of the source code. No
independent human code review or security audit has been performed. Validation
to date consists of automated checks and functional testing; these checks do
not establish that the code is correct or secure. See [validation](docs/VALIDATION.md)
for the checks actually performed. AI assistance is part of the development
process; TRM does not call an AI service at runtime.

## Implemented

- Unix ControlSocket discovery or an explicit socket; optional numeric loopback TCP.
- SAFECOOKIE with OS randomness and constant-time server-proof verification.
- Relay nickname, full fingerprint, exact Tor version, PID and process uptime.
- BW event rates, shared-scale RX/TX graphs, bounded history up to 24 hours,
  weighted averages when events are coalesced, and process byte totals.
- Descriptor observed and advertised bandwidth, consensus weight, consensus flags,
  configured rate/burst limits, bootstrap, liveness and accounting status.
- Bounded NOTICE/WARN/ERR history, text search, severity filters, consecutive
  repeat grouping and a recent warning indicator.
- Searchable, sortable OR snapshots with identities/nicknames Tor actually provides.
- Tor CPU, resident RAM and open file descriptors / soft process limit, with
  descriptor pressure indicators and explicit permission/availability reasons.
- Display pause across all pages while background collection continues.
- Automatic reconnect, command timeouts, terminal resize, keyboard navigation,
  ASCII/no-color modes and terminal restoration on normal errors, panics and SIGTERM.
- A noninteractive `--diagnose` command that requires a real BW event.
- A compact relay header with colored flags, full-width time-scaled graphs and
  automatically saved graph height/time-window preferences.
- Tor accounting period totals, quota/rule, remaining bytes and a UTC-derived
  reset countdown when accounting is enabled. Process totals remain separate.
- Tor-reported IPv4/IPv6, recommendation status next to the version, and separate
  download/upload totals for the current monitor run.

Missing values are `N/A`; press `h` for reasons. A disconnected snapshot is stale.
Network presence uptime is unavailable: a descriptor publication timestamp is not
continuous network uptime. OR connections are not circuits. Transit paths, local
circuit details and optional MetricsPort support are not implemented. Exit relays
use the same metrics but exit-specific behavior has not been validated.

## Build and install on Debian/Ubuntu

Use a current stable Rust toolchain with Cargo, rustfmt and Clippy. The manifest
declares Rust 1.88 as the minimum; tests have passed on Rust 1.88 and 1.99.

If a compiler/linker is missing, an administrator can install the prerequisites:

```sh
sudo apt update
sudo apt install build-essential pkg-config ca-certificates
```

Install Rust using the instructions at <https://www.rust-lang.org/tools/install>.
Then, from this source directory:

```sh
cargo build --release --locked
./target/release/trm --diagnose --socket /your/tor/control
cargo install --path . --locked --bin trm
trm
```

Installation commands above are user-run examples. The application never installs
packages, changes Tor configuration, restarts Tor, or changes permissions.

The primary command is `trm`. The `tor-relay-monitor` command remains available
in source builds and as a compatibility link in the Linux archive. Both use the
same application code. The existing preferences directory name is unchanged.

### Installing the Linux archive

For Linux amd64, copy the release archive to the server and extract it:

```sh
tar -xzf tor-relay-monitor-0.5.3-linux-x86_64.tar.gz
cd tor-relay-monitor-0.5.3
./trm --version
command -v trm
```

If the last command finds an existing executable, check whether it is your prior
TRM installation before replacing it. Command names cannot be guaranteed unique
across all systems. If `trm` belongs to another program, use the longer
`tor-relay-monitor` name instead. Do not overwrite unrelated tools.

Once the destination is appropriate, install the selected executable:

```sh
sudo install -m 0755 trm /usr/local/bin/trm
trm --diagnose
trm
```

For the alternative name, copy the compatibility link's target with
`sudo install -m 0755 tor-relay-monitor /usr/local/bin/tor-relay-monitor`.

This copies a standalone executable. No systemd unit, startup job, daemon,
background collector, web server or listener is installed. The process runs only
while the command is open; `q` stops it. No Tor restart is needed.

After copying the executable to `/usr/local/bin`, the extracted release directory
and archive can be removed: the installed binary does not use them. Display
preferences live separately under `~/.config/tor-relay-monitor/ui.toml` (or
XDG_CONFIG_HOME). Keep any manually supplied `--config` file outside the extracted
directory if you want to continue using it. A program launched directly as
`./trm`, without the copy step, still depends on that extracted executable.

## Connection and permissions

By default, the monitor looks for an existing Unix socket at `/run/tor/control`,
`/run/tor/control.sock`, `/var/run/tor/control`, and `/var/run/tor/control.sock`, in
that order. These are candidates, not universal paths. An existing socket that
cannot be authenticated produces an error; the monitor does not silently switch
to another Tor instance. No TCP port is probed automatically.

```sh
trm --socket /run/tor/control
trm --tcp 127.0.0.1:9051
trm --tcp '[::1]:9051' --diagnose
trm --socket /custom/control --cookie /custom/control_auth_cookie
trm --ascii --no-color
```

Tor must already expose a local control endpoint and offer SAFECOOKIE. The monitor
reads the cookie path from `PROTOCOLINFO` unless `--cookie` is given. It refuses
NULL, legacy COOKIE and password authentication. Cookies must be regular files of
exactly 32 bytes. Unix cookie symlinks and world-readable cookies are refused.

Your account needs access to the socket, search permission on its parent
directories, and read permission on the cookie. Debian installations often use
the `debian-tor` group, but inspect the actual unit, torrc, ownership and cookie
before deciding which group is appropriate. Grant only the necessary local
control access, and log in again after an administrator changes group membership.
Control access itself is powerful even though this monitor is read-only. Do not
make the cookie world-readable or the control socket publicly accessible. Running
the monitor as root is not required.

For an isolated test instance only, a minimal example is:

```text
DataDirectory /private/test/tor-data
SocksPort 0
DisableNetwork 1
ControlSocket /private/test/control
CookieAuthentication 1
```

Do not apply this example to a production relay: `DisableNetwork 1` disables its
network activity. The supplied test harness creates its own configuration and data
directory and never edits an existing instance.

## Configuration and controls

Configuration is optional and read only with `--config PATH`. See
[the example](config.example.toml). Command-line connection selection overrides
the file's endpoint. `NO_COLOR` disables colors.

```sh
trm --config ./monitor.toml
trm --timeout 10 --diagnose --socket /custom/control
```

| Key | Action |
| --- | --- |
| `q`, Ctrl-C | Quit |
| `1`, `2`, `3` | Dashboard, events, OR connections |
| `h`, `?` | Help, units and missing-metric reasons |
| `p` | Pause / resume all displayed data; collection continues |
| `/` on logs / OR | Edit a case-insensitive text search (128 characters max) |
| Enter / Esc during search | Apply / cancel the edit; ordinary shortcuts become search text |
| Esc outside search | Clear the current page's search |
| `e` on logs | Cycle all severities / WARN+ERR / ERR |
| `d` on logs | Expand / group consecutive identical events |
| `s` / `r` on OR | Cycle status / name / identity sorting; reverse direction |
| Up, Page Up | Older logs / lower rows |
| Down, Page Down | Newer logs / upper rows |
| Home | Reset scroll |
| `[` / `]` | Decrease / increase preferred height of each graph |
| `-` / `+` (or `=`) | Previous / next window: 30s, 60s, 90s, 120s, 1h, 24h |

Graph height defaults to 12 rows per graph, including borders, and is constrained
by the available terminal height. The preferred height is retained when resizing
the terminal. The time window ranges from 30 seconds to 24 hours. Both settings save
automatically in `$XDG_CONFIG_HOME/tor-relay-monitor/ui.toml`, or
`~/.config/tor-relay-monitor/ui.toml` when XDG_CONFIG_HOME is absent. Only display
preferences are persisted; no metrics, events or authentication material are saved.
Missing or invalid preferences use defaults; load/save errors remain visible.

The dashboard puts Bandwidth above the graphs and keeps OR/notice panels small.
Rate/burst settings and detailed source explanations are in help. The OR view
separates status, known nickname and identity/endpoint into columns, with source
information in a bottom panel. Normal control connectivity is omitted from the
header; disconnects still show a warning and mark data stale.

The header always includes `IPv4:` and `IPv6:`, using `not detected` when Tor has
no valid address for that family. These are Tor's external-address guesses, not
addresses fetched from another service. Fields wrap onto extra header rows when
needed. All flags reported by the local consensus are retained, comma-separated,
and wrapped, including additional/unknown flags.

The version badge uses `GETINFO status/version/current`: `(UP)` is green for
`recommended` and red for `old`, `obsolete`, or `unrecommended`. A version newer
than Tor's recommendation list gets `(NEW)`; missing/unknown/disconnected status
gets `(?)`. No-color mode spells out recommended/update states. This follows
Tor's local network recommendation, not an online check for the latest Debian
package or a guarantee of security support.

Graph titles show current rate, window average and `session` bytes. A session is
this invocation of the monitor: received BW event bytes accumulate from attachment
and survive control reconnects. They reset when the monitor is launched again.
Traffic during a disconnected control interval cannot be counted. No byte total
is extrapolated for gaps or persisted to disk. Process totals are independent.

Graphs span the available width using actual elapsed-time buckets, with a shared
automatic B/s scale and time-to-now labels in seconds, minutes or hours. Unknown intervals remain blank;
received zero traffic uses an empty, zero-height bar without decorative dots.
Sub-second scheduling jitter between adjacent BW intervals is aligned (up to
500 ms); larger interruptions remain gaps. A wider display does not fabricate
additional samples. The displayed average follows the selected time window.

The short windows retain at most 120 BW samples. The 1h and 24h views use at
most 1,441 minute aggregates of received traffic, including partial minutes.
Rates and averages divide by observed interval counts, excluding unknown traffic.
Empty minutes stay blank; interruptions inside one minute cannot be resolved on
these aggregated views. History fills from monitor attachment, survives control
reconnects and resets when TRM exits. No history is backfilled or saved to disk.

`p` freezes the current snapshot, graph time axis, uptime and countdowns across
all pages. Background collection, reconnects and bounded retention continue.
Resume jumps directly to the latest snapshot. The pause holds one extra bounded
snapshot; it does not create an unlimited backlog.

Log timestamps are UTC receipt times. Grouping applies only to consecutive equal
severity/message entries still in the 500-event ring. `[xN since HH:MM:SSZ]`
shows a run's count and first receipt time; the line timestamp is its latest
receipt. Filtering does not merge runs separated by hidden events. Search,
severity, grouping and OR sort settings are session-local and never written to disk.

Tor process resources are sampled every five seconds from `/proc/<Tor PID>`.
CPU includes user and system time; 100% means one logical CPU and threaded Tor
can exceed it. Two samples from the same PID/start time are required. RAM is the
kernel's approximate resident set, not virtual memory. The resource row reads
`Tor process usage: CPU ...% | RAM ... GiB | FD used/limit`, with RAM in GiB
to two decimal places. `Window | Dropped | Malformed` appears immediately below
it in both compact and large layouts. FD means open file descriptors
divided by the soft open-file limit: yellow `HIGH` at 60%, red `CRITICAL` at 90%.
Files, sockets and pipes use descriptors; this is unrelated to Tor's exit policy.
The dashboard shows FD only when both usage and limit are readable and fresh.
Otherwise the entire FD field is omitted, while CPU/RAM and saved graph settings
remain independent. A valid zero count is displayed. In diagnostics, `FDs: N/A/65536`
means the limit is known but usage could not be counted, often because the monitor
cannot list `/proc/<Tor PID>/fd`. Help and diagnostics retain availability reasons.
Use your normal account; FD never requires changing Tor permissions or elevating
the monitor. Display preferences belong to that account, so a different account
(including root via sudo) can have different saved graph heights/windows.
The scan is capped at 1,048,576 entries and reads at 64 KiB per file. Run TRM in
Tor's PID namespace: the sampler assumes Tor's reported PID identifies the local
process. Inaccessible files, process exit or unsupported platforms yield `N/A`;
help and diagnostics explain the reason. Values older than ten seconds
or from a disconnected session are unavailable. No privileges are requested or changed.

The header's `Port:` is the advertised IPv4 OR port from the own server descriptor,
falling back to the local consensus. IPv6 uses that port unless its advertised
port differs, in which case an additional `IPv6 port:` field appears. If only
IPv6 is available, its port is used. Missing data is `not detected`. This is not
the control port, not a probe of external reachability, and not a guess from a
local-only `ORPort ... NoAdvertise` listener. Configured ORPort remains in help.

Process RX/TX totals reset on Tor restart, not at midnight. When Tor accounting
is enabled, a separate period row shows its counters and configured quota/rule,
and a countdown uses Tor's own interval end. A period can be daily, weekly or
monthly. With accounting disabled no daily quota or midnight reset is invented,
and the monitor never enables accounting or changes its settings.

80x24 or larger is recommended. Below 44x14 a resize message replaces the panels.
Some labels are clipped in compact layouts; all availability details remain in
help. Compact dashboards put the OR connected count alongside process totals.
Window/dropped/malformed counts retain their own row below process resources.
No mouse is needed. The TUI uses at most four timed redraws per second, plus
input/resize events, with Ratatui's differential terminal output.

## Troubleshooting

- **No socket found:** discover the configured endpoint; use `--socket`. Discovery
  intentionally does not search every service instance or expose any endpoint.
- **Permission denied:** check directory traversal, socket group, cookie group and
  account membership. The program never changes these automatically.
- **SAFECOOKIE unavailable:** check the existing Tor authentication configuration.
  Do not enable a remote or unauthenticated ControlPort to work around this.
- **N/A fingerprint/descriptor/consensus:** a client-only Tor, a new relay, or a
  relay absent from Tor's current local consensus can legitimately lack these.
- **No BW:** a stale rate is N/A after three seconds; `--diagnose` exits unsuccessfully
  if no event arrives by its deadline. Check whether Tor is responsive.
- **Disconnected:** retained values are stale; reconnect delay grows to at most
  30 seconds. The next successful session reloads identity and process counters.
- **Dropped/malformed counters:** the event queue overflowed, an event exceeded
  the memory limit, or BW/OR fields were invalid. BW uses a separate accumulator;
  slow rendering does not silently drop the BW byte sums.

Normal exits, Ctrl-C, SIGTERM, and Rust panics restore the terminal. SIGKILL,
hardware failure, and terminal transport loss cannot be handled by a program;
`reset` or `stty sane` can restore a damaged shell terminal.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
python3 scripts/test_local_tor.py --tor /usr/bin/tor
python3 -m unittest discover -s scripts -p 'test_release.py'
```

The Tor test harness starts and stops only a private, offline Tor process. See
[CONTRIBUTING.md](CONTRIBUTING.md), [architecture](docs/ARCHITECTURE.md),
[protocol and metric sources](docs/TOR_PROTOCOL.md), and [technical research](docs/RESEARCH.md).

GitHub CI checks formatting, Clippy, Rust 1.88/current stable, release builds,
archive boundaries and a private offline Tor child. Dependabot proposes Cargo
and GitHub Actions updates. A separate scheduled RustSec audit checks the lockfile.
Maintainers can use the manual release workflow to build verified Linux and
source archives. The [release process](docs/RELEASING.md) describes validation,
artifacts and publication.

## License

MIT, see [LICENSE](LICENSE). This is an independent implementation. Nyx is
GPL-3.0; no Nyx source code is included or translated here.
Dependency-provided licenses and notices for the distributed Linux graph are
included in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
