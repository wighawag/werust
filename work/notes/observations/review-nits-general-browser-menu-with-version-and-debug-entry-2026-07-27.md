---
title: review-gate non-blocking nits for 'general-browser-menu-with-version-and-debug-entry' (Gate 2 approve)
date: 2026-07-27
status: open
reviewOf: general-browser-menu-with-version-and-debug-entry
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'general-browser-menu-with-version-and-debug-entry' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Four doc sites still describe version() as the Rust workspace version / CARGO_PKG_VERSION, which is exactly the claim the requeue existed to kill. Should they be corrected to say build-time-resolved WERUST_VERSION?
  (docs/platform-capability-matrix.toml:413 (the Rust workspace CARGO_PKG_VERSION), crates/werust-ios/Sources/werust_mobile.h:171, crates/werust-ios/App/Sources/WerustCore.swift:219, crates/werust-android/.../WerustCore.kt:139 (all say the Rust workspace version). Code: lib.rs version() = env!(WERUST_VERSION).)
- RATIFY: the fix added fetch-depth: 0 to the android-apk and ios-simulator-app checkouts (previously default shallow) and pinned that in the shape test. That is a cross-task change to the release job's checkout cost/behaviour, not recorded in the Decisions block.
  (.github/workflows/release.yml android-apk + ios-simulator-app checkout steps; release_plumbing_shape.rs checkout_fetch_depth assertion.)
- RATIFY: nothing enforces that the workspace Cargo version tracks the release. At v0.2.7 the last-resort path silently reports 0.2.6 for a no-git tarball build. Is a comment (Bump it with the release it names) enough, or should a follow-up task add a guard?
  (Cargo.toml workspace.package version = 0.2.6 with a prose comment only; no test ties it to the newest tag.)
- On the dispatch dry-run WERUST_VERSION is empty and the version comes from git describe, but build.rs only re-runs on a WERUST_VERSION change, while CI restores a cached target dir keyed on Cargo.lock. A dry-run artifact can therefore carry a stale describe string. Accept, or add a cheap cache-buster?
  (crates/werust-core/build.rs rerun-if-env-changed only; release.yml actions/cache on ~/.cargo + target keyed by hashFiles Cargo.lock.)
- The Kotlin dispatch comment claims an unknown menu id fails visibly rather than silently doing nothing, but returning false from OnMenuItemClickListener produces no visible effect. Reword or make it actually visible?
  (BrowserActivity.kt onBrowserMenuItem: else -> false, with the doc comment above it.)
- The Kotlin and Swift menu code is never compiled by the pure-Rust gate; the browser-menu parity row is implemented on the strength of a source-shape guard, and a release is being cut off this work immediately. Worth one manual/emulator pass before tagging?
  (docs/platform-capability-matrix.toml browser-menu row = implemented on all three; README manual steps recorded as Not yet executed; gate is cargo fmt/clippy/build/test only.)
