# Tor Control Protocol and metric sources

Primary references: [control commands](https://spec.torproject.org/control-spec/commands.html),
[replies/events](https://spec.torproject.org/control-spec/replies.html),
[server descriptors](https://spec.torproject.org/dir-spec/server-descriptor-format.html),
and [directory specification](https://spec.torproject.org/dir-spec/).
Reviewed on 2026-11-21. Tor feature availability, not a guessed version string,
determines what can be shown.

## Wire subset

`PROTOCOLINFO 1` discovers SAFECOOKIE and a C-escaped COOKIEFILE path. The cookie
is read locally. `AUTHCHALLENGE SAFECOOKIE <32-byte-client-nonce-hex>` returns a
server nonce and HMAC. The server HMAC must verify before the monitor sends
`AUTHENTICATE <controller-hmac-hex>`.

Both HMACs cover `cookie || client_nonce || server_nonce`, using SHA-256. The
distinct keys are the protocol's exact ASCII server-to-controller and
controller-to-server hash strings. Verification uses RustCrypto's constant-time
MAC comparison. No secret or authentication payload is printed.

After authentication, `GETINFO events/names` identifies supported event types.
`SETEVENTS BW NOTICE WARN ERR` subscribes to the available subset; BW is required.
Only GETINFO, GETCONF and these connection-local subscription/authentication
commands are sent. The monitor never asks Tor to resolve, fetch user traffic,
change configuration, take ownership, or alter circuits.

The decoder accepts single-line, continuation (`-`), and data (`+`) replies, CRLF,
dot termination and dot unstuffing. It separates 650 events from command replies
and tolerates future event fields. It also handles an event inserted between
reply continuation lines, although the current spec forbids that placement.
Malformed or incomplete frames fail the session. Bounds are documented in
[ARCHITECTURE.md](ARCHITECTURE.md).

## Metric contract

"Poll" means an initial read and a refresh every `poll_seconds` (default 15 s).
No entry below requires a third-party request, DNS lookup, or MetricsPort.

| Metric / UI field | Exact source | Update | Limits and N/A behavior |
| --- | --- | --- | --- |
| Exact Tor version | `GETINFO version` | Poll | Local daemon version, not a guessed package version |
| Version recommendation | `GETINFO status/version/current` | Poll | Green UP: recommended; red UP: old/obsolete/unrecommended; NEW: new/new in series; ?: unknown, unavailable or disconnected. A local Tor recommendation, not the newest package version |
| IPv4 / IPv6 | `GETINFO address/v4`, `address/v6` | Poll | Tor's external-address guesses; 551, malformed, wrong-family or unspecified values display `not detected`. No external lookup |
| Nickname | `GETCONF Nickname` | Poll | Current configured value; absent values are N/A |
| Full fingerprint | `GETINFO fingerprint` | Poll | Must be 40 hex characters; often absent on client-only Tor |
| PID | `GETINFO process/pid` | Poll | N/A if unsupported or invalid; identifies the process sampled below |
| Tor CPU | Linux `/proc/PID/stat` user/system ticks and start time; `sysconf(_SC_CLK_TCK)` | Every 5 s | Two samples from the same process; 100% = one logical CPU; missing permissions/PID changes give N/A |
| Tor RAM | Linux `/proc/PID/stat` RSS pages; `sysconf(_SC_PAGESIZE)` | Every 5 s | Approximate resident bytes, not virtual memory; no smaps scan |
| Open FDs / limit | Linux `/proc/PID/fd` entry count; `/proc/PID/limits` Max open files soft limit | Every 5 s | HIGH >=60%, CRITICAL >=90%; dashboard omits the FD field if either value is missing/stale; diagnostics retain N/A and reasons; no privilege changes |
| Process uptime | `GETINFO uptime` | Poll, monotonic interpolation while connected | Seconds reported by Tor; interpolation between reads is local, freezes on disconnect |
| Continuous network uptime | No reliable implemented source | Never | N/A; no approximation is represented as official uptime |
| Descriptor publication | `GETINFO desc/id/<own fingerprint>`, `published` field | Poll | UTC publication timestamp, not time in consensus or continuous presence |
| Flags | `GETINFO ns/id/<own fingerprint>`, `s` line | Poll | Local consensus opinion; absent relay/status gives N/A; future flags preserved |
| Operational state | Control connectivity; `network-liveness`, `status/bootstrap-phase`, `status/circuit-established`, `accounting/hibernating` | Poll; connection state immediately | These are separate signals, not proof that a relay is reachable from every client. Details appear in diagnostics |
| Instantaneous RX/TX | `650 BW <read> <written>` | Approximately once per second | Counts Tor bytes during each event interval, not bits. A delayed group is a labeled aggregate; stale after 3 s |
| History / average | Received BW byte sums and interval counts | Collector tick, 250 ms | 120 short samples plus <=1,441 minute aggregates for 1h/24h; partial minutes use observed counts, empty minutes absent; sub-minute gaps unresolved in long views; no disk history |
| Process RX/TX totals | `GETINFO traffic/read`, `traffic/written` | Poll | Tor process totals, never "since monitor started"; no double addition of BW events |
| Monitor session RX/TX | Sum of received BW event byte deltas | Collector tick and connection close | Starts at attachment in this invocation, survives control reconnects, resets on application restart. Disconnected intervals are unknown; no persisted history or invented traffic |
| Accounting enabled | `GETINFO accounting/enabled` | Poll | Period-only queries are made when enabled; disabled accounting never becomes a fabricated daily quota |
| Period RX/TX and remaining bytes | `GETINFO accounting/bytes`, `accounting/bytes-left` | Poll when enabled | Separate read/write pairs supplied by Tor; not process totals, not necessarily daily |
| Period interval and reset | `GETINFO accounting/interval-start`, `accounting/interval-end` | Poll when enabled; countdown every render | UTC parsed explicitly; disconnected countdown N/A; exact period comes from Tor rather than assuming midnight |
| Accounting quota and rule | `GETCONF AccountingMax`, `AccountingRule`, `AccountingStart` | Poll | Tor's configured period/limit/rule; `sum`, `max`, `in` and `out` have distinct meanings; no automatic configuration changes |
| Observed bandwidth | Own descriptor `bandwidth` third field | Poll | Tor's descriptor estimate in B/s; cache absence gives N/A |
| Advertised bandwidth | Minimum of the own descriptor's avg, burst, observed values | Poll | Explicit descriptor-derived capacity in B/s, not instantaneous traffic |
| Consensus bandwidth | Own router status `w Bandwidth=` multiplied by 1000 | Poll | Consensus units are decimal KB/s; a selection weight, not guaranteed throughput. May be unmeasured; no measured status is claimed |
| Global rate / burst | `GETCONF BandwidthRate`, `BandwidthBurst` | Poll | Tor normalized B/s; burst is a token-bucket parameter, not observed traffic |
| Relay rate / burst | `GETCONF RelayBandwidthRate`, `RelayBandwidthBurst` | Poll | Zero means inherit global settings; values displayed explicitly, not guessed effective limits |
| OR states / neighbors | `GETINFO orconn-status` lines: peer identity and OR status | Poll | Full snapshot counts and CONNECTED count; first 2048 rows retained. No inference of unknown names/identities. No reverse DNS |
| Advertised OR port | Own descriptor `router` / first valid IPv6 `or-address`; fallback own consensus `r` / `a` | Poll | Descriptor preferred; separate IPv6 port shown if different; absent/zero/invalid values remain not detected; no reachability probe |
| Configured OR listeners | First `GETCONF ORPort` value, shown in help/diagnostics | Poll | Can include local-only or NoAdvertise settings; never substituted for the advertised external port |
| Local circuit details | Not queried | Never | Deferred; control circuits would only describe circuits exposed by Tor, not arbitrary transit paths |
| Transit circuit paths | Not available through this monitoring subset | Never | Middle/Guard OR neighbors do not reveal complete transit circuit paths |
| NOTICE/WARN/ERR | Corresponding 650 events | On receipt | In-memory last 500, receipt UTC timestamp; local substring/severity filtering and consecutive repeat grouping do not change subscriptions. No backfill or persistent file; drops counted |
| MetricsPort | Not implemented | Never | Application works without it; complementary exporter source deferred |

## Interpretation and omissions

The resource row labels descriptors as `FD used/limit` and RAM as GiB to two
decimal places. FD includes files, sockets and pipes, not Tor's exit policy.
A known limit with unavailable usage (for example `N/A/65536`) is partial data;
it does not mean zero usage or an exhausted limit. `/proc/PID/fd` can be restricted
even when `/proc/PID/limits` is readable. The dashboard hides such partial FD data
and shows the field again when both counters are fresh; zero usage is valid.
Diagnostics continue to expose partial values and reasons. Window/drop/malformed counters appear
in a separate row after resources in every dashboard layout.

Ratatui graphs and rate labels use binary byte units: KiB = 1024 B and MiB =
1,048,576 B. The underlying BW event represents roughly one second; it is not a
high-precision sampling clock. Consensus conversion first uses 1000, then display
converts to KiB/s. Bits/s are not displayed, so no implicit factor of eight is used.

Observed, advertised and consensus values describe different things. Their
differences do not by themselves prove overload. Global and relay bandwidth
limits, accounting state, bootstrap and current traffic provide administrator
context; descriptor pressure indicators are threshold comparisons, not an inferred
network saturation diagnosis. Linux resource fields follow the
[kernel's proc documentation](https://docs.kernel.org/filesystems/proc.html).
Their availability depends on local permissions and the PID namespace; they
expire after ten seconds or disconnect. No extra Tor commands are used for them.

The optional own descriptor lookup uses local Tor cache/descriptor information.
Absence is normal before publication or consensus inclusion. No global consensus
or all-relay descriptor download is requested. OR counts are a periodic snapshot,
not an event-perfect live connection count or a circuit count. Only `CONNECTED`
states contribute to the explicitly named CONNECTED count.

`--diagnose` verifies authentication, supported subscriptions, metric requests,
and reception of a live BW event. Missing relay-only values do not make a client
test instance fail, but lack of a BW event does. Diagnostics print metric values
and absence reasons; they do not dump Tor event logs or authentication material.
