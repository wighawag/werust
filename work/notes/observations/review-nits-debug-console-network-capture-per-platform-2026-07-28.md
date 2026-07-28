---
title: review-gate non-blocking nits for 'debug-console-network-capture-per-platform' (Gate 2 approve)
date: 2026-07-28
status: open
reviewOf: debug-console-network-capture-per-platform
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'debug-console-network-capture-per-platform' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify Decision 2: a NEW page-reachable script-message channel 'werustDebug' was introduced alongside the provider channel. It is one-way and fail-quiet, but it is a new cross-task surface the follow-on debug-view tasks will depend on.
  (crates/werust-core/src/debug.rs CAPTURE_BRIDGE; DECISIONS.md Decision 2)
- Ratify Decision 7: Android passed-through https requests are recorded with status/mime/size honestly NULL, so the Network tab will show rows with unknown status for every non-intercepted request. A user-visible default the view tasks will render.
  (BrowserActivity.kt passed-through branch; DECISIONS.md Decision 7)
- Ratify Decision 9: the parity-matrix row debug-capture-console-and-network is marked implemented on all three platforms even though nothing is user-visible yet (the tabs are follow-on tasks). The row comment disambiguates, but a reader of the matrix alone could over-read it.
  (docs/platform-capability-matrix.toml; DECISIONS.md Decision 9)
- Unrecorded in-scope decision: both shims are injected into ALL frames (desktop UserContentInjectedFrames::AllFrames, iOS forMainFrameOnly: false), so iframe console output and iframe fetch/XHR are also captured. Reasonable, but DECISIONS.md does not say it; worth a line so the view tasks know iframe entries are expected.
  (crates/webview-renderer/src/backend.rs inject_script; WKWebViewShellController.swift addUserScript)
