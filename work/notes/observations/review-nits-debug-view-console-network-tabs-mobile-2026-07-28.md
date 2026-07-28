---
title: review-gate non-blocking nits for 'debug-view-console-network-tabs-mobile' (Gate 2 approve)
date: 2026-07-28
status: open
reviewOf: debug-view-console-network-tabs-mobile
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'debug-view-console-network-tabs-mobile' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- RATIFY: Android debug view is a full-screen overlay inside BrowserActivity, not the separate Activity/Fragment the task body floated. Rationale (session cannot cross an Activity boundary) is sound and recorded as DECISIONS.md Decision 1.
  (crates/werust-android/.../BrowserActivity.kt debugView overlay on a FrameLayout root; docs/spikes/debug-view-console-network-tabs-mobile/DECISIONS.md Decision 1)
- RATIFY: fail-closed trust mapping — an unrecognised posture string renders as '⚠ unverified-origin' rather than verbatim, on both platforms. A new user-visible default the task did not specify; safe direction (can only understate trust), recorded as Decision 4.
  (DebugView.kt networkTrustLabel/trustColor else-arms; WKWebViewShellController.swift networkTrustLabel/trustColor default arms)
- RATIFY: refresh is event-driven (existing refreshChrome points + console capture event) with a FULL re-render per refresh, not a poll and not the desktop's sequence-anchor. The task said 'poll on the existing chrome-refresh cadence'; on mobile that cadence is event-driven, and the FFI document carries no sequence, so this is the coherent reading. Recorded as Decision 3.
  (DECISIONS.md Decision 3; refresh calls in BrowserActivity.refreshChrome/onConsoleMessage and refreshChrome/DebugCaptureHandler.onCapture)
- iOS refreshes (full JSON parse + reloadData) fire per captured envelope via onCapture; a console-spamming page triggers many main-thread reloads. Bounded at 300 rows so cost is small, but a coalesce/throttle may be wanted if a hot page ever lags. Not a defect today.
  (WKWebViewShellController.swift DebugCaptureHandler.didReceive -> onCapture?() -> DebugViewController.refresh())
