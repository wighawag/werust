---
title: Mobile — Android app shell linking the Rust core static lib
slug: mobile-android-shell-and-static-lib
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [browser-shell-url-bar-and-live-interactive-view]
covers: [18]
---

## What to build

Build a real Android app module (Kotlin only at the forced OS edge — Activity, URL
bar, back/forward over the seams) that links the werust Rust core cross-compiled as
a static library, and runs on a device/emulator. This is the Android half of mobile
parity with wezig. Cross-compile the Rust core for the Android ABIs as a normal
build step (Gradle task), mirroring wezig's real app module structure.

## Acceptance criteria

- [ ] A real Android app module (not a spike) builds an installable (unsigned debug) APK that launches and shows a browsing surface over the seams.
- [ ] The Rust core is cross-compiled to a static lib and packaged for the floor ABIs (arm64-v8a + x86_64) as a normal Gradle build step.
- [ ] Kotlin is confined to the OS edge (Activity/shell); browsing logic stays in the Rust core behind the seams.
- [ ] A BUILD-leg check asserts the APK carries the Rust core lib for both ABIs.

## Blocked by

- Blocked by `browser-shell-url-bar-and-live-interactive-view`.

## Prompt

> Goal: Android parity with wezig — a real app module linking the cross-compiled Rust
> core, running on a device/emulator (see `CONTEXT.md`: mobile with Kotlin only at the
> forced OS edge).
>
> Mirror wezig's real Android app module (Activity + URL bar + back/forward over the
> seams), but cross-compile the RUST core (not Zig) as a static lib via a normal
> Gradle step for arm64-v8a + x86_64, packaged into an unsigned debug APK. This is a
> load-bearing part of the Zig-less build experiment (`docs/adr/0002`). Signing/store
> is out of scope. Feeds the release job (`release-goreleaser-rust-desktop-and-mobile-artifacts`).
>
> Done = a real Android app module builds an installable debug APK carrying the Rust
> core for both floor ABIs and launches a browsing surface.
