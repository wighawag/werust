---
title: Release — native x86_64-only desktop (drop Zig) + decouple mobile jobs
slug: fix-release-native-x86-desktop-and-decouple-mobile
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: []
covers: [19]
---

## What to build

Two related release-pipeline changes the human explicitly chose (B+C), to end a chain of
cargo-zigbuild-vs-WebKitGTK link failures and stop the desktop leg from blocking the
mobile artifacts:

**B — Desktop becomes native x86_64-only (no Zig, no cross-compile).** The desktop binary
dynamically links system WebKitGTK/GTK/glib. cargo-zigbuild uses `zig cc` as the linker,
which does NOT search the host's system library paths, so it fails to link glib-2.0 /
gobject-2.0 / cairo even though `libwebkitgtk-6.0-dev` is installed (dry-run 29902579536:
`error: unable to find dynamic system library 'glib-2.0'`). Zig-as-linker is the wrong
tool for a system-lib-linking binary. Fix: drop the `aarch64` desktop target and
cargo-zigbuild entirely; build ONLY `x86_64-unknown-linux-gnu` with the NATIVE system
linker (plain `cargo build`), which already works (the `verify` job + local builds link
WebKitGTK fine). arm64 stays where it belongs (mobile, via NDK/Xcode) — desktop arm64
Linux is dropped.

**C — Decouple the mobile jobs from the desktop leg.** `android-apk` and
`ios-simulator-app` currently `needs: goreleaser` only for ORDERING (so a tag's Release
exists to upload into). That coupling makes a desktop failure block the APK. Remove
`needs: goreleaser` from both mobile jobs (or make them `needs: verify` instead) so they
build independently. On a tag, ensure the Release still exists before `gh release upload`
(see below).

## What to change

1. **`.goreleaser.yaml`**:
   - In `builds: - id: werust-desktop`: remove `tool: cargo` + `command: zigbuild`, remove
     the `aarch64-unknown-linux-gnu` target, keep ONLY `x86_64-unknown-linux-gnu`. Use the
     rust builder's NATIVE cargo build (no zigbuild) — i.e. GoReleaser's `builder: rust`
     with the default `cargo build` tool/command (drop the `command: zigbuild`), still
     `flags: [--package=werust]`. If GoReleaser's rust builder cannot do a plain native
     build cleanly for a single host target, it is acceptable to replace the GoReleaser
     rust-build of the binary with a `before` hook / a prebuilt binary GoReleaser just
     packages — but prefer keeping `builder: rust` with native cargo if it works.
   - Remove the `cargo install --locked cargo-zigbuild` and the arm64 `rustup target add`
     from the before-hooks (keep `rustup target add x86_64-unknown-linux-gnu` only, or drop
     it since the host is already x86_64).
   - Archives: now one archive (x86_64) instead of two — update accordingly. Keep checksums
     + the conventional-commit changelog.
2. **`.github/workflows/release.yml`**:
   - `goreleaser` job: DELETE the "Set up Zig" step (no longer needed). Keep the WebKitGTK
     system-deps install (the native build links them).
   - `android-apk` + `ios-simulator-app`: change `needs: goreleaser` -> `needs: verify` (or
     remove the `needs` on goreleaser) so they do NOT wait on / get blocked by desktop.
   - Tag-path Release existence: with the mobile jobs no longer `needs: goreleaser`, on a
     tag the GitHub Release may not exist yet when a mobile job runs `gh release upload`.
     Make the upload robust: either have each leg `gh release create <tag> --generate-notes
     || true` before upload (idempotent create), or keep a lightweight `needs` on whichever
     single job creates the Release. Do NOT reintroduce a desktop-BUILD dependency; only a
     Release-EXISTENCE guarantee. Simplest: each mobile job does
     `gh release create "$GITHUB_REF_NAME" --generate-notes 2>/dev/null || true` then
     `gh release upload "$GITHUB_REF_NAME" <artifact> --clobber`.
   - Keep the `workflow_dispatch` dry-run path (snapshot desktop + upload-artifact for
     APK/.app, publishes nothing).
3. Update the ADR-0002 / DECISIONS notes + the release task done-record to reflect: desktop
   is native x86_64-only (Zig-less in the FULL sense now — no Zig linker either); arm64 is
   mobile-only; mobile jobs are decoupled from desktop. Record WHY (the zigbuild/WebKitGTK
   link incompatibility).

## Acceptance criteria

- [ ] `.goreleaser.yaml` builds ONLY `x86_64-unknown-linux-gnu`, natively (no `command: zigbuild`, no cargo-zigbuild install, no arm64 target); still selects `--package=werust`; checksums + conventional-commit changelog intact.
- [ ] The `goreleaser` workflow job has NO "Set up Zig" step; it still installs WebKitGTK system deps and builds the native x86_64 desktop binary.
- [ ] `android-apk` and `ios-simulator-app` no longer `needs: goreleaser` (they build independently; a desktop failure cannot block them).
- [ ] On a tag, the mobile jobs still attach their artifacts to the Release (idempotent `gh release create ... || true` before `gh release upload --clobber`, or an equivalent Release-existence guarantee that is NOT a desktop-build dependency).
- [ ] The `workflow_dispatch` dry-run still builds everything and uploads workflow artifacts without publishing.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` pass locally (this is CI-YAML + goreleaser-config; no Rust code change).

## Prompt

> Goal (human chose B+C): make the desktop release native x86_64-only with NO Zig, and
> decouple the mobile jobs from the desktop leg. cargo-zigbuild's `zig cc` linker cannot
> link the desktop binary's system WebKitGTK/GTK/glib (it doesn't search system lib paths),
> causing repeated link failures; the native system linker already links them fine (the
> verify job proves it). So: in `.goreleaser.yaml` drop `command: zigbuild` + the
> cargo-zigbuild install + the `aarch64` target, build only `x86_64-unknown-linux-gnu`
> natively with `flags: [--package=werust]`; in `release.yml` delete the "Set up Zig" step
> and change `android-apk`/`ios-simulator-app` from `needs: goreleaser` to `needs: verify`
> so the APK/.app are never blocked by desktop. On a tag, guarantee the Release exists for
> `gh release upload` via an idempotent `gh release create ... --generate-notes || true`
> (a Release-existence guarantee, NOT a desktop-build dependency). Keep the dispatch
> dry-run. Update ADR-0002/DECISIONS: desktop is now fully Zig-less (native x86_64),
> arm64 is mobile-only, mobile decoupled from desktop, with the zigbuild/WebKitGTK reason
> recorded. YAML/config only; no Rust changes.
>
> Done = a release dry-run builds the native x86_64 desktop dist + the APK + the .app with
> no Zig, and the mobile jobs are independent of the desktop leg.
