---
title: review-gate non-blocking nits for 'blank-and-window-open-links-navigate-in-place' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: blank-and-window-open-links-navigate-in-place
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'blank-and-window-open-links-navigate-in-place' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Android enables settings.javaScriptCanOpenWindowsAutomatically = true, an in-scope decision the task did not specify and no Decisions block records. It changes a user-visible default: non-user-gesture window.open() JS popups now also fire onCreateWindow and navigate in place (not just user-gesture _blank clicks). Ratify or reverse.
  (crates/werust-android/.../BrowserActivity.kt:203; task only named onCreateWindow + setSupportMultipleWindows(true).)
- The desktop connect_create handler calls life.begin(url)+view.load_uri(url) directly, bypassing the validate_url() guard that navigate() runs. Empty/whitespace targets are already filtered to Ignore by new_window_action, but a malformed non-empty target (no scheme) is loaded unvalidated. WebKitGTK fails such a load gracefully so impact is low; worth a note for parity with navigate().
  (crates/webview-renderer/src/backend.rs:390 vs backend.rs:642 navigate().)
- iOS/Android native hooks mirror the in-place intent but do not call the shared renderer::new_window_action rule (only desktop + the seam tests do), so the Ignore-on-empty-target branch is desktop/seam-only. Acceptable per the task (native new-window hooks) but a slight divergence from the one-shared-rule framing in the ADR/matrix.
  (crates/werust-ios WKUIDelegate loads request unconditionally on nil targetFrame; Android transport loads request.url verbatim.)
