# Development rules

- Build a real, read-only monitor for one local Tor relay on Linux. No web UI, telemetry, simulated production data, or user traffic analysis.
- Keep protocol framing and SAFECOOKIE independent of collection, domain state, and terminal presentation. Preserve these boundaries unless a technical reason is documented.
- Write source, comments, documentation, UI labels, diagnostics, and commit messages in English.
- Never modify torrc, systemd, permissions, production services, circuits, or relay configuration without explicit user authorization. Never take ownership of Tor. No remote control endpoints.
- Send only the allowlisted authentication, GETINFO, GETCONF, and SETEVENTS commands. Never log authentication payloads or cookies.
- Bound input, queues, histories, and caches. Count dropped events. Show missing or stale data honestly; OR connections are not transit circuit paths.
- Persist display preferences only. Accounting periods come from Tor; never label process totals as daily traffic or invent a quota/reset time.
- Use idiomatic stable Rust, meaningful errors, rustfmt, and Clippy. Avoid production unwrap/expect. Test framing, authentication, malformed input, reconnects, metrics, and resource limits.
- Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo build --release` before delivery.
- Keep README.md, docs/ARCHITECTURE.md, docs/TOR_PROTOCOL.md, CONTRIBUTING.md, and CHANGELOG.md synchronized with implemented behavior. Record untested limits honestly.
- Do not copy Nyx code. Follow its GPL obligations if reuse is ever explicitly proposed.
