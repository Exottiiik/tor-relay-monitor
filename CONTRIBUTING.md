# Contributing

Use stable Rust and keep all code, comments, documentation, UI labels and commits
in English. Read [AGENTS.md](AGENTS.md) and [architecture](docs/ARCHITECTURE.md).
The parser, authentication, collector, domain state and presentation have separate
responsibilities; document a technical reason before changing these boundaries.

Describe the scope of AI assistance and the checks actually performed in the
pull request. Do not present AI-assisted analysis or automated checks as an
independent human code review. Do not include private relay data,
authentication material or private development conversations in a contribution.

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
cargo +1.88.0 test --locked
python3 -m unittest discover -s scripts -p 'test_release.py'
```

Avoid production unwrap/expect, unjustified Clippy allowances, unbounded input,
unbounded task/channel creation, and speculative frameworks. Test behavior and
failure modes rather than mirroring private implementation details. Scripted
peers in `tests/control.rs` are explicitly test-only; never introduce a mock data
fallback in the application.

Keep resource sampling outside terminal drawing. Never resolve OR peers or read
process environments, command lines, user traffic or socket endpoints for these
resource fields. Exercise unavailable `/proc` data and process replacement as well
as normal reads. For history changes, test partial minutes, retained byte sums,
real gaps, clock jitter, window clipping and bounded work on delayed aggregates.
Pause tests must show continued snapshot delivery while the displayed snapshot
and its clocks remain frozen. Search tests must check input bounds and that typed
shortcut letters do not navigate or quit. Do not persist queries, logs or metrics.

With an already installed Tor binary, run:

```sh
python3 scripts/test_local_tor.py --tor /usr/bin/tor
python3 scripts/test_local_tor.py --tor /usr/bin/tor --release --measure-seconds 5
python3 scripts/test_local_tor.py --tor /usr/bin/tor --release --accounting
```

This harness creates a private temporary Tor configuration, disables network and
SOCKS, starts a child process, runs the ignored real-Tor test, then stops its own
child and removes its temporary directory. It tests Unix and TCP on Linux; TCP on
Windows. It never connects to a production service. Alternatively, explicitly
select an isolated instance yourself:

```sh
TRM_TEST_SOCKET=/private/test/control cargo test --test control real_local_tor -- --ignored
TRM_TEST_TCP=127.0.0.1:19051 cargo test --test control real_local_tor -- --ignored
```

Do not point these tests at production without authorization. Never modify Tor
configuration, permissions or services on behalf of a contributor or user without
explicit authorization. Do not add mutating control commands, DNS lookups, remote
control endpoints, telemetry, or persistent sensitive logs.

For stress/performance work, use the release flood test and record hardware,
toolchain, event count, elapsed wall time and peak RSS, then compare like-for-like:

```sh
cargo test --release --test control event_flood_is_bounded -- --nocapture
```

The test verifies exactly 20,000 valid events, queue bounds, drop accounting and
BW sums. It is not a production CPU/RAM benchmark. Do not advertise unmeasured
performance. Include a sustained real-relay soak test before claiming production
readiness; use an operator-authorized deployment and retain no user data.

GitHub CI repeats the lint/test/build checks on Linux and runs the private offline
Tor harness. Missing FD usage is valid under Tor's default process hardening on
an unprivileged runner; an explanation is required, not privilege elevation.
RustSec auditing runs separately on pushes, pull requests and a weekly schedule.
Review advisory warnings as well as vulnerability failures; do not silently add
ignores or raise the minimum Rust version for convenience.

Use [SECURITY.md](SECURITY.md) for vulnerabilities and the sanitized GitHub issue
forms for ordinary defects. Release packaging uses Python 3.11 or newer and the
checked-in scripts documented in [RELEASING.md](docs/RELEASING.md). Keep the
source allowlist explicit; `.tools`, `target`, `dist` and local credentials must
not enter published source archives. Workflow actions are pinned to full commits;
review Dependabot's updates and keep workflow tokens read-only.
After changing dependencies, regenerate `THIRD_PARTY_NOTICES.md` using
`python3 scripts/generate_notices.py`, then run the script with `--check`.
The collected texts cover the distributed Linux x86_64 graph, including test/build
dependencies. A new binary platform needs a corresponding notice review.

Update README.md, docs/ARCHITECTURE.md, docs/TOR_PROTOCOL.md, and CHANGELOG.md
when behavior changes. Keep research and validation claims accurate. Submit small
reviewable changes with relevant validation and limitations. No Nyx code reuse
without a license review and an explicit decision about GPL obligations.
