---
title: review-gate non-blocking nits for 'release-goreleaser-rust-desktop-and-mobile-artifacts' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: release-goreleaser-rust-desktop-and-mobile-artifacts
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'release-goreleaser-rust-desktop-and-mobile-artifacts' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: desktop targets are ONLY the two Linux triples (x86_64/aarch64-unknown-linux-gnu); no macOS/Windows desktop targets. Recorded in DECISIONS.md, matches criterion 1 verbatim (Linux binaries) and the WebKitGTK Linux-first backend. Reasonable default; human to ratify.
  (.goreleaser.yaml builds[].targets; DECISIONS.md 'Desktop targets are the two Linux triples')
- Ratify: the test seam is a dev-only serde_yaml shape test hosted in werust-core (not a new crate), parsing both config files rather than running GoReleaser. Recorded + justified in DECISIONS.md; dev-dep only, no shipped-binary impact.
  (crates/werust-core/tests/release_plumbing_shape.rs; werust-core/Cargo.toml [dev-dependencies] serde_yaml)
- GoReleaser rust builder in a multi-crate workspace: the config sets binary: werust + tool: cargo/command: zigbuild but no explicit package/dir. Resolution relies on the single werust bin. Not statically verifiable here and outside the pure-Rust verify gate; flag as a CI-runtime risk to watch on first real tag, not a block.
  (.goreleaser.yaml builds[].id werust-desktop)
