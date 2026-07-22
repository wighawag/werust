---
title: Gate-3 (conductor) verdict — release-goreleaser-rust-desktop-and-mobile-artifacts — APPROVE
date: 2026-07-22
kind: observation
reviewOf: release-goreleaser-rust-desktop-and-mobile-artifacts
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 827a0b3)

The final task. Un-blocked once iOS landed (both mobile deps done). `do` ran Gate-1 +
Gate-2 (pure-Rust) green; the actual release build runs on CI. Conductor review.

### Acceptance criteria — all met

- ✅ `.goreleaser.yaml` uses `builder: rust` (cargo-zigbuild) → desktop Linux binaries
  (amd64 + arm64) + checksums on a tag.
- ✅ Changelog generated from conventional-commit git history (`changelog: use: git`);
  no changeset files.
- ✅ Three jobs mirror wezig's `release.yml`: `goreleaser` (Linux, `needs: verify`),
  then `android-apk` (Linux; `./gradlew :app:assembleDebug` + `check-apk-abis.sh`) and
  `ios-simulator-app` (macos-14; `rustup target add aarch64-apple-ios-sim`,
  `BUILD_ONLY=1 build-and-run.sh` + `check-app-bundle.sh`, zips the `.app`) — both
  `needs: goreleaser`, gated after the desktop build.
- ✅ `workflow_dispatch` dry-run: goreleaser `--snapshot` (publishes nothing), every
  leg uploads workflow artifacts via `actions/upload-artifact` instead of attaching.
- ✅ The SAME verify gate runs first (`verify` job identical to dorfl.json; all jobs
  `needs: verify`), so a tag can't ship a red tree.

### Forward-notes honoured

Both mobile forward-note contracts were reused exactly: the Android job runs the
Gradle build + `check-apk-abis.sh`; the iOS job runs on macos-14 with the
`BUILD_ONLY` packaging path + `check-app-bundle.sh`. Also authored ADR-0002 (the
Zig-less GoReleaser-rust-builder decision).

### Nit triage

1. Desktop targets are ONLY the two Linux triples (no macOS/Windows) — RATIFY/KEEP.
   Matches criterion 1 verbatim + the WebKitGTK Linux-first backend. Recorded.
2. Test seam is a dev-only serde_yaml SHAPE test in werust-core (parses both config
   files, does not run GoReleaser) — RATIFY/KEEP. The config shape is verifiable in the
   pure-Rust gate; running GoReleaser is a CI concern. Dev-dep only.
3. **GoReleaser rust builder in a multi-crate workspace**: config sets `binary: werust`
   + `tool: cargo`/`command: zigbuild` but no explicit `package`/`dir`; resolution
   relies on the single `werust` bin. NOT statically verifiable offline and outside the
   pure-Rust gate. APPROVED (config is well-formed; there is exactly one bin crate so
   resolution should succeed), captured as a CI-RUNTIME WATCH item below — confirm on
   the first real tag / a `workflow_dispatch` dry-run.

### CI-runtime watch item captured (not a defect)

On the FIRST tag or a `workflow_dispatch` dry-run, confirm GoReleaser's rust builder
resolves the `werust` bin in the workspace (no explicit `package`/`dir` set). If it
mis-resolves, add `dir: crates/werust` (or the package selector) to the build. Only
verifiable at CI runtime; flagged so the first release run is watched.

### Milestone

This lands the deliberately Zig-less build path end-to-end (ADR-0002): a tag cuts one
GitHub Release with desktop Linux binaries + checksums + a conventional-commit
changelog + the Android APK + the iOS Simulator `.app` zip, with a dispatch dry-run
that validates all artifacts without publishing.
