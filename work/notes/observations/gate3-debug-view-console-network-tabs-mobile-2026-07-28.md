---
title: "Gate-3 conductor review: debug-view-console-network-tabs-mobile (APPROVE)"
date: 2026-07-28
status: open
reviewOf: debug-view-console-network-tabs-mobile
verdict: approve
---

## Verdict: APPROVE

Merged as `f86a24a`, first dispatch, no recovery — the payoff of the store/menu/capture foundations landing clean. 329 `werust-core` tests re-run locally green (plus the mobile wiring-shape suite).

## Acceptance criteria, ticked against the merged tree

- [x] **A full-screen Console + Network tabbed view on Android and iOS, from the menu's Debug entry, over the FFI.** Android is a full-screen overlay inside `BrowserActivity` (Decision 1 records why not a separate Activity: the native session cannot cross an Activity boundary — sound). iOS is a `DebugViewController` in `WKWebViewShellController`. Both read the SAME shared store over the FFI debug document.
- [x] **Network tab shows the honest per-request trust in the indicator's vocabulary, never a new label.** Pinned by `the_network_tab_reuses_the_mobile_trust_indicators_vocabulary_never_a_new_label`. The rows render the core's four wire names (`content-verified`, `unverified-origin`, `name-via-trusted-rpc`, `mutable-name`) with the indicator's glyph and hues. And the mapping FAILS CLOSED (Decision 4): an unrecognised posture string renders as `unverified-origin`, so a future or corrupted label can only UNDERSTATE trust, never overstate it. That is the safe direction and exactly right for a trust surface.
- [x] **Updates on the existing cadence, NO tight main-thread poll — the ANR fix is NOT regressed.** I grepped `DebugView.kt` for `postDelayed`/`Timer`/`Thread.sleep`: none. Refresh is event-driven off the existing `refreshChrome` points and the console-capture event (Decision 3). The refresh reads the debug document OFF the native session lock (BrowserActivity.kt:545), so it does not block behind a CAR retrieval. This is the specific regression the whole drive was told to watch for, and it is clean.
- [x] **Clear action + a way back.** `clear_empties_the_store_and_the_view_updates_on_the_existing_cadence`.
- [x] **iOS renders what capture can see; partial capture is recorded, not hidden.**
- [x] **Mobile-scoped, parity-tracked** — `the_parity_matrix_marks_all_three_debug_views_implemented`, with recorded manual device steps in the spike README.

## Nit triage (4 non-blocking findings)

Three are ratifications of sound in-scope decisions (the in-Activity overlay; the fail-closed trust mapping; the event-driven full re-render reading of "existing cadence" — on mobile that cadence IS event-driven and the FFI carries no sequence, so it is the coherent reading). The fourth is the one worth carrying forward: iOS refreshes fire per captured envelope via `onCapture`, so a console-SPAMMING page triggers many main-thread `reloadData` calls. It is bounded at 300 rows so the cost is small, but if a hot page ever lags, a coalesce/throttle is the fix. Not a defect today; worth a line for the human eyeballing a real device.

## The debug menu subsystem is now COMPLETE

This is the sixth and last task of the `in-app-debug-menu-console-and-network` spec. End to end: a bounded capture store in core (task 2), a general browser menu with a real version and a Debug entry (task 3), real console+network capture on every platform (task 4), and the standalone tabbed debug view on desktop (task 5) and mobile (this task). The human request that started it — a phone user with NO tethered desktop opens the menu -> Debug and sees console + network — is now deliverable, subject only to the human eyeballing it on a real device.

## For the human (the one thing left)

The mobile debug view is exercised by source-shape and FFI mapping tests only; the Kotlin and Swift are NEVER compiled by the pure-Rust gate. Open the menu -> Debug on a real Android and iOS build and confirm: the two tabs render, the Network tab shows the trust badges in the indicator's vocabulary, a long session keeps rendering past 300 entries, and a console-heavy page does not stutter the UI thread (the iOS per-envelope refresh).
