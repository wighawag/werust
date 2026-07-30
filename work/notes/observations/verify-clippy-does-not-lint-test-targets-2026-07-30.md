# `verify`'s clippy does not lint test targets

2026-07-30, noticed while building `one-derivation-close-the-aggregate-and-tooltip-gaps`.

The gate runs bare `cargo clippy` (`dorfl.json`), which lints lib/bin targets only, so lint debt in `#[cfg(test)]` code never reds it: `cargo clippy --all-targets` today reports a pre-existing `unnecessary use of copied` in `crates/werust-core/src/debug.rs:1977` and nine `field_reassign_with_default` in `crates/werust-macos/src/paint.rs`'s tests. Not fixed here (outside this task); the question is whether `verify` should say `--all-targets`.
