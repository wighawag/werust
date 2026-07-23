---
title: "Chrome resets to a neutral loading state on navigation: trust indicator becomes a spinner, not the stale previous-page posture"
slug: chrome-loading-state-resets-trust-indicator
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [2]
---

## What to build

On navigation, the trust indicator must NOT keep showing the previous page's trust posture while a new (potentially differently-trusted) page loads. Today the stop control appears (loading is detected), but `refresh_chrome` unconditionally paints `trust_indicator(state)` from `state.trust_posture`, so the user sees the OLD page's badge (e.g. a stale "content-verified") while a NEW, possibly unverified site is loading. That is a trust-honesty problem, not just cosmetic: the indicator asserts a trust level that does not apply to the page being loaded.

Make the chrome show a NEUTRAL loading state for the trust indicator while a load is in flight: while `ChromeState::is_loading()` is true, the trust indicator becomes a loading/spinner ("loading…", no trust claim) instead of the carried-over posture; the REAL posture is revealed only when the load settles. Apply it consistently on desktop AND the mobile shells (the trust indicator is a cross-platform capability in the parity matrix, so all shipped platforms must honour the loading state or be tracked). A fresh navigation should also clear any stale name/posture/error so nothing from the previous page lingers into the new load.

## Acceptance criteria

- [ ] While a load is in flight (`is_loading()`), the trust indicator shows a neutral loading/spinner state (no trust claim), NOT the previous page's posture.
- [ ] When the load settles, the indicator shows the NEW page's real posture (content-verified / name-via-trusted-rpc / mutable-name / unverified), driven by the actual load path as today.
- [ ] A failed load shows its failure state, not a stale success posture.
- [ ] Applied on desktop and the mobile shells (or explicitly tracked per the parity guard); the parity matrix stays honest.
- [ ] Tests assert the loading state hides the prior posture and that the real posture appears only after settle (a fake backend driving load-start -> load-settle across two pages with different postures), mirroring the existing trust-posture tests.

## Blocked by

- None — can start immediately.

## Prompt

> Goal: on navigation the trust indicator must become a neutral loading/spinner state, not keep showing the previous page's (stale) trust badge while a new, possibly differently-trusted site loads. This is a trust-honesty fix: never assert a trust level for a page that is not the one being displayed.
>
> Where to look: `crates/werust-core/src/lib.rs` (`ChromeState`, `is_loading()`, `navigate`/`refresh_chrome`; the `LoadState` already models loading and the docs mention a spinner) and the desktop chrome `crates/werust/src/main.rs` (`refresh_chrome` -> `trust_indicator(state)` is painted unconditionally) plus the mobile shells (`crates/werust-android`, `crates/werust-ios`). The trust postures live in `TrustPosture` (renderer crate). The precedent for load-path-driven posture is the existing trust-indicator work in `tasks/done/`.
>
> Done = while loading, the trust indicator is a neutral loading state on all shipped platforms; the real posture appears only on settle; a fresh navigation clears stale name/posture/error; proven with a fake-backend two-page test. FIRST re-check the chrome still paints the posture unconditionally during load. RECORD any non-obvious display-rule decision durably.
