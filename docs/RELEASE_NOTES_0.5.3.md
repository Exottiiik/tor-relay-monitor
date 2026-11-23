# TRM 0.5.3 — first public beta

TRM is an independent, read-only terminal monitor for one local Tor relay on
Linux. It is inspired by Nyx and is not an official Tor Project or Nyx release.

The project began as a personal effort to modernize the Nyx-style monitoring
experience after the maintainer encountered unexpected errors and behavior in
their Nyx setup and was concerned about the pace of upstream maintenance.
TRM is a separate implementation focused on read-only relay monitoring.

AI assistance was used
extensively for design, code, tests, debugging and documentation; the source
includes substantial AI-generated contributions. The maintainer has defined
requirements and tested the tool in real use, but has not performed a technical
source-code review. No independent human code review or security audit has been
performed. Automated checks and functional use are the available validation;
TRM does not call an AI service at runtime.

## Included

- SAFECOOKIE authentication via Unix socket or explicit numeric loopback TCP.
- Relay identity, addresses, advertised OR port, consensus flags and Tor version
  recommendation status, with explicit missing/stale data.
- Full-width download/upload graphs, monitor-session byte totals and in-memory
  history up to 24 hours; height/window preferences save per user.
- Tor accounting periods and reset countdowns when accounting is enabled.
- CPU/RSS and optional FD usage; inaccessible FD fields disappear automatically.
- Searchable severity-filtered logs, repeat grouping and sortable OR neighbors.
- Display pause while collection continues, reconnects, ASCII/no-color modes.

## Validation and limits

Automated checks and private offline Tor tests are documented in
[VALIDATION.md](VALIDATION.md). On 2026-11-23 an operator reported approximately
24 hours of use with no observed problem. This is qualitative feedback from one
setup, without recorded TRM memory/CPU measurements or a verified restart/fault
matrix. It does not establish universal production stability.

History/logs are not persisted. OR neighbors are not complete transit circuit
paths. Linux process resources depend on local permissions and Tor's PID namespace.
The supplied Linux x86_64 build targets Debian 12 / glibc 2.36 or compatible
systems. Other architectures need a source build.

Use the installation instructions in README.md. Verify the downloaded assets
with the supplied SHA256SUMS before running them. Checksums detect corruption;
they are not a release signature. Run TRM with your normal account.
