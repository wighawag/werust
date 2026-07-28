---
title: "Mobile debug view (Android + iOS): a full-screen tabbed view (Console + Network) opened from the menu's Debug entry, rendering the core capture store"
slug: debug-view-console-network-tabs-mobile
spec: in-app-debug-menu-console-and-network
blockedBy: [debug-capture-store-console-and-network-in-core, general-browser-menu-with-version-and-debug-entry]
covers: [1, 3]
---

## What to build

The MOBILE debug view (Android + iOS) for werust's in-app debug menu (design: `work/notes/observations/idea-in-app-debug-menu-console-and-network-2026-07-26.md`). The general menu's Debug entry opens a full-screen tabbed view showing the CONSOLE log and NETWORK requests captured into the core store. This is the mobile payoff of the human's request: a phone user with NO tethered desktop can open the menu -> Debug and see console + network (the native remote inspector - Safari over USB / chrome://inspect - stays as the deep devtools for those who tether).

READ-FIRST / drift check: confirm the store exposes console + network over the FFI JSON (blockedBy store task) and the mobile menus have a Debug entry with an open-debug-view hook (blockedBy menu task).

Build a full-screen debug view on each mobile platform, rendering the SAME shared store over the FFI:
- **Android** (`crates/werust-android/.../BrowserActivity.kt` + as needed a new Activity/Fragment): a debug screen with a tab bar (`TabLayout` + a pager, or two toggled lists) - a Console tab (level-distinguished list of entries: level + message + source:line) and a Network tab (list of requests: method, url, status, mime, size + the honest per-request trust posture). A Clear action + back to the page. Reads the store via the FFI (the chrome/debug JSON).
- **iOS** (`crates/werust-ios/...`): a debug screen (SwiftUI `TabView` or a UIKit tab controller) with the same Console + Network tabs, reading the store over the FFI. Note iOS network capture may be partial (per the capture task); render what is captured and it improves as capture does.
- Both: render werust's HONEST per-request trust posture in the Network tab using the SAME vocabulary the mobile trust indicator uses (ADR-0006; do NOT invent a new label). A Clear affordance (calls the store clear via the FFI). The view UPDATES as entries are captured (poll on the existing chrome-refresh cadence the ANR fix established - do NOT reintroduce a busy/tight main-thread loop). READ-ONLY render (no typeable REPL - that is the native remote inspector's job).

Coherence: render the ONE shared store; reuse the FFI chrome/debug surface + the existing refresh cadence (respect the Android ANR fix - no tight main-thread poll); honest per-request trust; open from the menu Debug hook.

## Acceptance criteria

- [ ] On Android and iOS, the menu's Debug entry opens a full-screen debug view with a Console tab and a Network tab, rendering the core capture store over the FFI.
- [ ] Console tab shows captured console entries (level + message + source:line, level-distinguished); Network tab shows captured requests (method, url, status, mime, size) each with werust's honest per-request trust posture using the SAME vocabulary as the mobile trust indicator (ADR-0006, not a new label).
- [ ] The view updates as entries are captured, reusing the existing chrome-refresh cadence (NO tight/busy main-thread poll - the Android ANR fix is respected), and has a Clear action + a way back to the page.
- [ ] iOS renders whatever the capture task can see (partial network capture is acceptable + recorded); the view is read-only.
- [ ] Mobile-scoped (desktop debug view is a separate task); tracked per the parity guard. Tests cover the FFI render-from-store mapping where testable + recorded manual device steps.

## Blocked by

- `debug-capture-store-console-and-network-in-core` (the store it renders, over the FFI).
- `general-browser-menu-with-version-and-debug-entry` (the mobile menu Debug entry that opens it).

## Prompt

> Goal: the MOBILE debug view (Android + iOS) - the menu's Debug entry opens a full-screen tabbed screen (Console + Network) rendering the core capture store over the FFI. The payoff: a phone user with no tether sees console + network in-app (the native remote inspector stays for those who tether).
>
> Where to look: Android `crates/werust-android/.../BrowserActivity.kt` (+ maybe a debug Activity/Fragment): a TabLayout+pager, Console tab (level list of entries) + Network tab (request list). iOS `crates/werust-ios/...`: a SwiftUI TabView / UIKit tabs, same two tabs. Both read the store over the FFI chrome/debug JSON, render the HONEST per-request trust posture in Network using the SAME vocabulary as the mobile trust indicator (ADR-0006, no new label), have a Clear action, and UPDATE on the existing chrome-refresh cadence - do NOT add a tight main-thread poll (respect the Android ANR fix). READ-ONLY (no REPL). iOS network capture may be partial (record it). Open from the menu Debug hook.
>
> Done = Console+Network full-screen tabs on Android+iOS rendering the store, honest per-request trust, Clear + live-update via the existing cadence (no busy loop), opens from the menu, read-only, mobile-scoped + parity-tracked, tested where testable + manual steps. FIRST re-check the FFI store surface + the mobile menu Debug hook exist.
