---
title: "A loading banner that names the phase ('resolving name', 'fetching content', 'verifying') and offers a cancel — the user is staring at a frozen page on long retrievals"
slug: loading-banner-with-phase-and-cancel
spec: in-app-debug-menu-console-and-network
blockedBy: []
covers: []
---

## What to build

HUMAN FIELD TEST (v0.2.7, Android, mobile network): on `ronan.eth`, navigating around (especially the blog list) periodically triggers Android's "kill app / wait" dialog. The user repeatedly hits "wait" and continues. The navigation IS working (the next page renders), but the UI thread is starved for seconds at a time. The freeze is most likely the SvelteKit client router doing its own page-side work on the *page*'s main thread (we cannot speed that up), but the user has no signal that anything is happening — the chrome shows the previous page with no affordance, so they don't know whether to wait, reload, or give up.

Fix: show a clear **loading banner** in the chrome while a load is in flight, naming the phase and offering a cancel. Right now the chrome carries a `load_state` and a `load_step` over the FFI (Started / Committed; load_step: name-fetch / content-fetch / content-rendering / verifying / settled), and an `is_loading()` predicate, but nothing in the shell USES it visibly during a load (the existing amber banner appears only on a failed load). Add a passive, non-blocking banner that appears as soon as `is_loading()` is true, disappears on `Finished`/`Failed`/`Idle`, shows the current `load_step` ("Resolving name…", "Fetching content…", "Verifying…", "Rendering…"), and includes a Cancel that calls the existing stop / reload-cleanup path.

Scope + coherence:
- The chrome JSON already carries everything needed (`loading`, `load_state`, `load_step`); this task is a UI-ONLY addition at the shell layer (desktop + Android + iOS), not a core change.
- The banner must NOT block the UI thread; it is a passive view update driven by the existing chrome-refresh pump.
- Cancel calls the same `core.stop()` the stop button already uses.
- Phase names match the existing `LoadStep` vocabulary verbatim so the debug Network tab and the banner cannot disagree.

What this does NOT fix: it does not speed up a slow retrieval; it does not fix the periodic "kill app" dialog; it does not fix the off-MainFrame network capture's per-envelope refresh on iOS.

## Acceptance criteria

- [ ] Desktop, Android, and iOS show a non-blocking banner as soon as a load is in flight (when `chrome.is_loading()` is true).
- [ ] The banner names the current phase (one of the existing `LoadStep` values), and updates as the phase advances.
- [ ] The banner disappears on Finished / Failed / Idle.
- [ ] The banner has a Cancel that calls the existing stop / reload-cleanup path; no new mechanic.
- [ ] The banner is driven by the existing chrome-refresh pump; no new timer / poll / tight loop; the Android ANR guard is NOT regressed.
- [ ] Network-isolated tests for the chrome-state -> phase-name mapping where testable + recorded manual steps for the visible banner on each platform.

## Blocked by

- None. The chrome JSON / `LoadStep` / `is_loading` already exist on main.

## Prompt

> Goal: when a load is in flight, show the user a non-blocking loading banner in the chrome that names the current phase and offers a cancel.
>
> Where to look: the existing chrome JSON in `crates/werust-android/rust/src/ffi_json.rs`, `crates/werust-ios/rust/src/ffi_json.rs`, `crates/werust-core/src/lib.rs` (the `ChromeState::load_state` + `load_step` + `is_loading()` predicate already exposed). The shell refresh pumps on each platform already read the chrome — add the banner as a view update driven by those pumps (NOT a new timer, NOT a new poll, NOT a tight loop on the UI thread — the Android ANR guard is not regressed). Phase names come from the existing `LoadStep` variants verbatim. Cancel calls the same `core.stop()` the stop button uses.
