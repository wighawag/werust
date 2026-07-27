---
title: "A general browser menu (like other browsers' ⋮ menu) on every platform: shows the version, and a Debug entry that opens the debug view"
slug: general-browser-menu-with-version-and-debug-entry
spec: in-app-debug-menu-console-and-network
blockedBy: []
covers: [2]
needsAnswers: true
---

## What to build

HUMAN REQUEST (v0.2.6): werust should have a GENERAL browser MENU, like other browsers' ⋮ / hamburger menu, that will grow to hold all the usual browser menu items later. FOR NOW it shows: the werust VERSION number, and a DEBUG entry (a button/item) that opens the debug view. This task is the MENU CONTAINER + those two entries; the debug VIEW it opens is separate tasks (`debug-view-console-network-tabs-desktop` / `-mobile`). Design context: `work/notes/observations/idea-in-app-debug-menu-console-and-network-2026-07-26.md`.

READ-FIRST / drift check: confirm there is NO menu on any platform today (desktop `crates/werust/src/main.rs` is a horizontal toolbar of back/forward/reload/stop + URL bar, no HeaderBar/MenuButton/PopoverMenu; mobile shells are a toolbar of buttons). The version is `env!("CARGO_PKG_VERSION")` (desktop already uses it in the startup line).

Build a general menu affordance + a menu surface on each platform:
- **Desktop (GTK)**: add a menu button (e.g. a `MenuButton` with a `⋮` / hamburger icon) in the toolbar (next to the existing controls, or a HeaderBar if that is cleaner), opening a `PopoverMenu` / popover. The menu shows a non-interactive "werust <version>" item (from `CARGO_PKG_VERSION`) and a "Debug" item. The Debug item triggers OPENING the debug view (a callback the debug-view task wires; for THIS task, a stub hook / signal the debug-view task fills - or, if landing after the debug-view task, wire it). `crates/werust/src/main.rs`.
- **Android**: add a menu button (⋮) to the toolbar opening a `PopupMenu` (or a simple menu surface); items "werust <version>" (version via the FFI or BuildConfig) + "Debug". Debug opens the debug view. `crates/werust-android/.../BrowserActivity.kt`.
- **iOS (WKWebView shell)**: add a menu button opening a menu (a `UIMenu` / action sheet / SwiftUI menu) with "werust <version>" + "Debug"; Debug opens the debug view. `crates/werust-ios/...`.

Design + coherence:
- The version string should come from ONE source. `CARGO_PKG_VERSION` is the Rust workspace version; expose it to the mobile edges over the FFI (a small `werust_version()` accessor in the core / FFI) so all three menus show the SAME version, rather than each hardcoding. Record the choice.
- The menu is a GENERAL container meant to grow (bookmarks, settings, etc. later), so structure it so adding items later is trivial - do not hardcode a debug-only menu. Name/structure it as the browser's primary menu.
- The Debug entry OPENS the debug view; the debug view itself is the `debug-view-console-network-tabs-*` tasks. This task can land with the Debug item wired to a placeholder ("debug view coming") OR be sequenced after the debug-view tasks so it opens the real view - decide + record. RECOMMENDED: land the menu with the version + a Debug item that calls an OPEN-DEBUG-VIEW hook; the debug-view tasks provide the view + fill the hook (they are blockedBy this + the store).
- The menu is a USER-FACING feature (not debug-build-gated): it is the general browser menu. (Network CAPTURE is a separate gating question handled in the store/toggle tasks; the MENU itself is always available.)

## Acceptance criteria

- [ ] Every platform has a general browser menu (a ⋮/menu affordance in the shell) that opens a menu surface, structured to grow (not a debug-only menu).
- [ ] The menu shows the werust VERSION number, sourced from ONE place (CARGO_PKG_VERSION, exposed to the mobile edges via the FFI so all three show the same version).
- [ ] The menu has a Debug entry that opens the debug view (wired to an open-debug-view hook the debug-view task fills, or the real view if sequenced after it - recorded).
- [ ] The menu is a user-facing feature, always available (not debug-build-gated); adding future items is structurally trivial.
- [ ] Applied on desktop, Android, iOS (the menu is a cross-platform shell affordance), or tracked per the parity guard.
- [ ] Tests cover what is testable (the shared version accessor; the menu-open / item wiring where a seam exists) + recorded manual steps for the native menu surfaces. Network-isolated.

## Blocked by

- None. (The menu CONTAINER; the debug view it opens is `debug-view-console-network-tabs-{desktop,mobile}`, which are blockedBy this + the store.)

## Prompt

> Goal: add a GENERAL browser menu (like other browsers' ⋮ menu) on every platform, meant to grow into the usual menu items later. FOR NOW it shows the werust VERSION + a Debug entry that opens the debug view. This is the menu CONTAINER; the debug view is separate tasks.
>
> Where to look: desktop `crates/werust/src/main.rs` (toolbar today; add a GTK `MenuButton` + `PopoverMenu`, or a HeaderBar), Android `crates/werust-android/.../BrowserActivity.kt` (toolbar; add a ⋮ button + PopupMenu), iOS `crates/werust-ios/...` (add a menu button + UIMenu/SwiftUI menu). Version from ONE source: `CARGO_PKG_VERSION`, exposed to mobile via a small FFI `werust_version()` accessor so all three menus agree. Structure the menu to grow (not debug-only). The Debug item calls an open-debug-view hook (filled by the debug-view tasks) - recommended: land the menu + version + Debug-item-hook now; the debug-view tasks (blockedBy this + the store) provide the view. The menu is user-facing, always available (not debug-gated).
>
> Done = a growable general menu on all 3 showing the version (one source via FFI) + a Debug entry that opens the debug view (hook or real view, recorded), user-facing, parity-tracked, tested where testable + recorded manual steps. FIRST re-check no menu exists on any platform.
