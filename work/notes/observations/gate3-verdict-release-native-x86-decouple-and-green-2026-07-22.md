---
title: Gate-3 — release B+C (native x86_64 desktop, no Zig; mobile decoupled) — APPROVE; dry-run GREEN
date: 2026-07-22
kind: observation
reviewOf: fix-release-native-x86-desktop-and-decouple-mobile
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ — full release dry-run GREEN, all artifacts produced

Human chose B+C after the cargo-zigbuild-vs-WebKitGTK link failure. `do` ran Gate-1 +
Gate-2 green; Gate-3 confirmed on the actual runner via a `workflow_dispatch` dry-run.

### What landed

- **B — native x86_64-only desktop, no Zig.** `.goreleaser.yaml` now uses
  `builder: rust` with `command: build` (plain native `cargo build`, NOT zigbuild),
  `flags: [--package=werust]`, single `x86_64-unknown-linux-gnu` target; the
  cargo-zigbuild install + arm64 target + "Set up Zig" step are all removed. The native
  system linker links WebKitGTK/GTK/glib fine (the same way the verify job + local builds
  do). werust is now Zig-less in the FULL sense — no Zig language, no Zig linker.
- **C — mobile decoupled.** `android-apk` + `ios-simulator-app` are now `needs: verify`
  (not `needs: goreleaser`), so a desktop failure can never block the APK/.app. On a tag
  each mobile leg idempotently `gh release create "$TAG" --generate-notes || true` before
  `gh release upload --clobber`, guaranteeing the Release exists without a desktop-BUILD
  dependency.

### Verified GREEN (dry-run run 29904198014)

All four jobs `success`: verify, goreleaser (native x86_64 snapshot), android-apk,
ios-simulator-app. Artifacts produced:
- `werust-desktop-dist` — 3.06 MB (native x86_64 Linux binary tarball)
- `werust-android-apk` — 1.37 MB
- `werust-ios-simulator-app` — 2.07 MB

### The CI-fix chain that got the release green (all landed)

1. system-deps (`libwebkitgtk-6.0-dev`) in the verify workflows (+ corrected the
   goreleaser desktop leg's wrong-ABI webkit2gtk-4.1 -> webkitgtk-6.0).
2. gtk4 feature pin v4_18 -> v4_14 (Ubuntu 24.04 runner has GTK 4.14).
3. setup-zig pinned to a real version (was `latest` -> unreleased 0.16.0, 404).
4. goreleaser `--package=werust` (multi-crate workspace selector).
5. B+C: native x86_64 desktop (drop Zig entirely) + decouple mobile — the root fix that
   made 3 and 4's Zig-path issues moot for good.

### Note

The desktop is now x86_64-only by DESIGN (arm64 desktop Linux dropped; arm64 lives in
mobile via NDK/Xcode). If arm64 desktop is ever wanted, the clean path is native
`ubuntu-24.04-arm` runners, NOT cargo-zigbuild (which cannot link the system-WebKitGTK
binary) — see why-zig-in-release-and-rust-native-alternatives note.
