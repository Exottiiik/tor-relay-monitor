# Security policy

## Supported versions

TRM is currently beta software. Security fixes target the latest published
version; older versions are not maintained as separate release branches.
There is no response-time or security-audit guarantee.

The implementation includes substantial AI-generated code. The maintainer has
tested its behavior but has not performed a technical source-code review. No
independent human code review or security audit has been performed; passing
automated checks and functional tests does not establish security.

## Reporting a vulnerability

Use [GitHub private vulnerability reporting](https://github.com/Exottiiik/tor-relay-monitor/security/advisories/new).
Do not report exploitable details through a public issue or pull request.
If private reporting is not enabled, open an issue requesting a private contact
without including the vulnerability, credentials or reproduction payload.

Provide the affected TRM version, Linux/Tor versions, impact and a minimal
reproducer using a private, offline Tor instance or scripted control peer.
Remove relay identities, addresses, connection peers and sensitive paths.
Never send a Tor authentication cookie, nonce/proof exchange, full torrc,
private keys or traffic information. Coordinate disclosure with the maintainer.

## Scope and trust boundary

TRM reads one local Tor control endpoint and optional Linux process resources.
It requires SAFECOOKIE and only allows authentication, GETINFO, GETCONF and
connection-local SETEVENTS commands. It does not manage Tor or expose a server.
Socket and cookie access still grants powerful control privileges to the account;
read-only behavior is an application boundary, not a reduced-privilege Tor account.
Use a normal account with the existing required local access. Do not make the
cookie world-readable, disable Tor protections or run as root to obtain FD data.
Unavailable FD usage is hidden in the dashboard.

Protocol input, queues and histories are bounded. Terminal control characters
are removed from untrusted display strings. Logs/metrics remain in memory;
only display preferences are saved. This is not a claim that process memory,
dependencies or the host operating system have received an independent audit.
