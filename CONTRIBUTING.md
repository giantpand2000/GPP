# Contributing

Thanks for helping improve GPP.

## Development setup

Install Rust 1.85 or newer, Xcode, and GStreamer as described in the
[README](README.md#requirements). Then run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
```

Keep pull requests focused and include tests for behavior changes where practical.

## Building a macOS release

```bash
./scripts/package-macos.sh --zip
```

The archive is written to `dist/GPP-<version>-macOS-<architecture>.zip`. The
matching `.sha256` file can be used to verify it. The bundle is ad-hoc signed;
official distribution signing and notarization require an Apple Developer
certificate and are not performed by the public CI workflow.

## Publishing a release

1. Update `version` in `Cargo.toml` and run `cargo check` to refresh the lockfile.
2. Commit the version change and create a matching tag, for example `v0.1.0`.
3. Push the commit and tag. CI verifies that the tag matches `Cargo.toml`, builds
   the app archive, and creates a GitHub Release with generated notes.

All contributions to the main GPP application are accepted under the project's
MIT OR Apache-2.0 dual license unless explicitly stated otherwise.
