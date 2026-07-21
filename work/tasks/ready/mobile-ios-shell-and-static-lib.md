---
title: Mobile — iOS app shell linking the Rust core, running on simulator
slug: mobile-ios-shell-and-static-lib
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [browser-shell-url-bar-and-live-interactive-view]
covers: [18]
---

## What to build

Build a real iOS app (Swift only at the forced OS edge — the app shell, URL bar,
back/forward over the seams) that links the werust Rust core cross-compiled for iOS,
and runs in the iOS Simulator (aarch64-ios-simulator). This is the iOS half of
mobile parity with wezig. Cross-compile the Rust core as a normal Xcode build phase,
mirroring wezig's real Xcode project structure.

## Acceptance criteria

- [ ] A real iOS app project (not a spike) builds a Simulator `.app` that launches and shows a browsing surface over the seams.
- [ ] The Rust core is cross-compiled for the iOS Simulator target as a normal Xcode build phase and linked into the app.
- [ ] Swift is confined to the OS edge (app shell); browsing logic stays in the Rust core behind the seams.
- [ ] A BUILD-leg check asserts the packaged `.app` contains the app bundle + binary.

## Blocked by

- Blocked by `browser-shell-url-bar-and-live-interactive-view`.

## Prompt

> Goal: iOS parity with wezig — a real app linking the cross-compiled Rust core,
> running in the iOS Simulator (see `CONTEXT.md`: Swift only at the forced OS edge).
>
> Mirror wezig's real Xcode project (app shell + URL bar + back/forward over the
> seams), but cross-compile the RUST core (not Zig) as a normal Xcode build phase for
> aarch64-ios-simulator. Simulator only — device/store builds need signing, out of
> scope. Part of the Zig-less build experiment (`docs/adr/0002`). Feeds the release
> job (`release-goreleaser-rust-desktop-and-mobile-artifacts`).
>
> Done = a real iOS app builds a Simulator `.app` carrying the Rust core and launches
> a browsing surface.
