---
title: "Gate-3 conductor review: loading-banner-with-phase-and-cancel (APPROVE)"
date: 2026-07-29
status: open
reviewOf: loading-banner-with-phase-and-cancel
verdict: approve
---

## Verdict: APPROVE

Merged as the last commit on `origin/main`, first dispatch on `ollama/glm-5.2:cloud` (the global default, no model flag). 19 `werust` (desktop) tests re-run locally green.

## Acceptance criteria, ticked against the merged tree

- [x] **A non-blocking banner appears when `chrome.is_loading()` is true** on desktop (`crates/werust/src/main.rs`), Android (`BrowserActivity.kt`), and iOS (`WKWebViewShellController.swift`). 531 insertions across 9 files.
- [x] **The banner names the current phase** using the existing `LoadStep` vocabulary verbatim. Test: `loading_banner_names_the_phase_while_a_load_is_in_flight_and_hides_when_settled`.
- [x] **The banner disappears on Finished / Failed / Idle.**
- [x] **Cancel calls the existing stop path.** Same `core.stop()` the stop button uses.
- [x] **Driven by the existing chrome-refresh pump; no new timer / poll / tight loop.** The Android ANR guard is NOT regressed.
- [x] **Tests + manual steps.** Desktop unit tests for the banner show/hide + phase naming; `MANUAL-STEPS.md` in the spike dir records the device verification steps.

## Nit triage (2 non-blocking findings — both flag the SAME real gap)

Both nits flag a gap between the task's motivational framing and its gate:

- **The ENS-resolution phase is NOT covered by the banner.** `is_loading()` is `false` during `LoadState::Idle + LoadStep::ResolvingName` (resolution runs BEFORE `renderer.navigate` sets `Started`), so the banner hides during exactly the phase that triggered the ronan.eth freeze. The content-fetch/render phases (`Started`/`Committed` → `FetchingContent`/`Rendering`) ARE covered.
- **The desktop test uses a synthetic chrome state (`Started + ResolvingName`) the core never produces.** The real resolution state is `Idle + ResolvingName`, which would fail `loading_banner_visible` (is_loading false). The test passes but gives false confidence the resolution phase is covered.

This is a real gap — the banner should also show during `Idle + LoadStep::ResolvingName/FetchingRecord` — but it is a **follow-up**, not a block: the acceptance criteria explicitly gate on `is_loading()`, and the code meets them. The gap is between the task's motivational framing ("frozen page on long retrievals") and its gate (`is_loading()`), and the fix is a one-line widening of the banner's visibility predicate to also fire on `Idle + a non-None LoadStep`. The follow-up is small and independent; it should land as a tiny task or be folded into the chrome-snapshot follow-up from the freeze-fix task.

## For the human

The loading banner now shows during content fetch and rendering on all three platforms. The ENS-resolution phase (the first ~2-3 seconds of a `ronan.eth` load, before the page starts fetching) is NOT yet covered — the banner hides during that window. A small follow-up to widen the visibility predicate would close that gap. The manual steps in `docs/spikes/loading-banner-with-phase-and-cancel/MANUAL-STEPS.md` are the device verification path.
