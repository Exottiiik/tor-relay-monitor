# Release process

Releases are built from version tags and validated before publication. Versions
below 1.0 are published as pre-releases. The version in Cargo.toml, Cargo.lock,
release notes and tag must agree.

## Validation

Run on Linux with Python 3.11 or newer, Rust and a C linker:

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
python3 -m unittest discover -s scripts -p 'test_release.py'
python3 scripts/generate_notices.py --check
cargo audit
```

CI covers the declared minimum Rust version, current stable, archive boundaries
and a private offline Tor instance. Review CI results and RustSec advisories for
the tagged commit. The security workflow installs the pinned cargo-audit version.
Regenerate THIRD_PARTY_NOTICES.md after dependency changes.

For local integration testing with an installed Tor binary:

```sh
python3 scripts/test_local_tor.py --tor /usr/bin/tor --release --accounting
```

The harness uses its own offline Tor child. Do not select a production instance.
Record checks and remaining limitations in [VALIDATION.md](VALIDATION.md).

## Artifacts

Create the version tag from the validated commit. Run the **Prepare release
artifacts** workflow with that tag as its input. The workflow checks the tag
against Cargo.toml, builds inside Debian 12 with Rust 1.88, runs tests and verifies
the archives. It uploads artifacts for review; publication remains manual.

For example, with an existing `v0.5.3` tag:

```sh
gh workflow run release.yml -f tag=v0.5.3
```

Download the `trm-release-linux-x86_64` workflow artifact. It contains:

- `tor-relay-monitor-<version>-linux-x86_64.tar.gz`
- `tor-relay-monitor-<version>-source.zip`
- `SHA256SUMS`

The Linux binary targets x86_64 and glibc 2.36 (Debian 12) or compatible systems.
A new binary platform requires its own build validation and dependency notices.

The equivalent local packaging commands are:

```sh
python3 scripts/package_release.py --output dist/release
python3 scripts/verify_release.py --directory dist/release
```

The packager honors CARGO_TARGET_DIR and accepts `--binary-dir PATH`. The binary
must be runnable on the packaging host and match Cargo.toml. `--source-only`
produces the source ZIP without a Linux binary. Verification executes the locally
built binary; do not use it to run an untrusted downloaded executable.

## Publication

Attach the three matching assets to the release for the validated tag. Use the
version's release notes and mark beta versions as pre-releases. Verify the
downloaded checksums before announcing the release.

Keep published tags and release assets immutable; corrections receive a new
version. SHA-256 checksums detect corruption but are not signatures. No signing
identity, artifact attestation or reproducible-build guarantee is configured.
