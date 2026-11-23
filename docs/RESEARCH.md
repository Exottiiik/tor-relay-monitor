# Design decisions

AI-assisted design analysis: 2026-11-21. This is not an independent human review.

## Nyx

The operator's motivation was to modernize their relay-monitoring experience
after encountering unexpected errors and behavior in their Nyx installation.
Concern about upstream maintenance also contributed. A release-history check on
2026-11-23 found that [Nyx's official website](https://nyx.torproject.org/) listed
2.1.0 (January 12, 2019) as the latest release. This documents a release gap, not
proof that all upstream maintenance has stopped.

Inspected Nyx's [application module](https://github.com/torproject/nyx/blob/master/nyx/__init__.py)
and [header panel](https://github.com/torproject/nyx/blob/master/nyx/panel/header.py).
They separate controller access, cached state, panels and periodic sampling.
The header uses Stem and process utilities; monitoring concepts and compact
keyboard views inform this project, but implementation and source are independent.
Nyx's [license](https://github.com/torproject/nyx/blob/master/LICENSE) is GPL-3.0.
No Nyx code is copied or translated, and this new code is licensed MIT.

## Rust controller choices

| Candidate | Evidence | Decision |
| --- | --- | --- |
| `torut` 0.2.1 | [Published API](https://docs.rs/torut/0.2.1/torut/), [manifest](https://github.com/teawithsand/torut/blob/master/Cargo.toml), [connection source](https://github.com/teawithsand/torut/blob/master/src/control/conn/authenticated_conn.rs). Tokio-based; authentication, raw commands and events available. Manifest marks it passively maintained; old crypto/rand dependency generations; events require driving its foreground reader/callback API | Viable starting point, but no audited guarantee of required bounded framing/queues. Adapting and auditing its lower layer would still be necessary. Do not claim it is broken or unusable |
| `arti-client` 0.48 | [Official API](https://docs.rs/arti-client/latest/arti_client/): implements a Tor network client, not monitoring an existing C Tor relay's control socket | Wrong responsibility; would add a Tor client and unnecessary dependencies |
| `tor-control` / `tor-controller` names | No usable published API was established at the queried documentation endpoints | Not treated as verified maintained alternatives; absence of a docs endpoint is not proof that no such project exists |
| Limited protocol subset | Official [commands](https://spec.torproject.org/control-spec/commands.html) and [events](https://spec.torproject.org/control-spec/replies.html) | Selected: small bounded parser, SAFECOOKIE and read-only requests, tested through scripted control peers and an opt-in local Tor test |

This is a scoped decision, not a general preference for implementing protocols.
Revisit a maintained controller library if it supplies equivalent framing limits,
SAFECOOKIE verification and event behavior without a larger adaptation burden.

## Dependency compatibility

Checked current published Ratatui 0.30.2, Crossterm 0.29.0 and Tokio 1.54.1 API
documentation and Cargo registry resolution. Cargo.lock records exact versions.
TOML 1.2 and getrandom 0.4 avoid unnecessary older parser/random-generator stacks.
HMAC 0.12.1 and SHA-256 0.10.9 are a compatible established RustCrypto pair;
newer breaking releases were observed and intentionally not adopted in this MVP.
No dependency's maintenance status is inferred solely from its version number.
Build/test results, rather than documentation alone, establish compatibility for
the tested toolchain. Declared MSRV requires a separate release gate.

## Initial architecture and risks

The minimal design is one control connection, one dedicated bounded reader,
one collector, bounded domain state, and a terminal consumer of latest snapshots.
The [metric table](TOR_PROTOCOL.md#metric-contract) defines every implemented
source, update interval and absence behavior.

Main risks and responses:

- SAFECOOKIE spoofing: verify the server HMAC before disclosing a controller proof;
  reject legacy COOKIE and unauthenticated fallback.
- Reply/event confusion: dedicated reader, explicit response codes, interleaving
  and multiline tests, command deadlines that discard the whole session.
- Descriptor/consensus absence: N/A with reasons; no external lookup or substitute.
- Middle/Guard visibility: use OR connection snapshots, not invented transit paths.
- High event volume: byte-preserving BW accumulator, bounded drop-counted log queue,
  bounded history and parser inputs.
- SSH terminal failure: modest refresh, differential rendering, terminal guard,
  resize handling, ASCII/no-color options.
- Production confidence: a private offline Tor validates control interaction,
  but cannot validate published relay flags/bandwidth or live exit behavior.

Optional MetricsPort and local circuit details are deferred to preserve the MVP's
scope. No speculative transport/source abstraction is left unfinished.
