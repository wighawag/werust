---
title: review-gate non-blocking nits for 'webview-follow-os-color-scheme' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: webview-follow-os-color-scheme
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'webview-follow-os-color-scheme' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the matrix marks follow-os-color-scheme as implemented on all three platforms, but only DESKTOP was device-verified (real before/after portal flag flip). Android (theme resources) and iOS (default-follow, no UIUserInterfaceStyle pin) are code+reasoning only, no real-device visual check. Is claiming ios/android = implemented (vs a tracked/planned state) acceptable without a device run?
  (docs/platform-capability-matrix.toml follow-os-color-scheme sets desktop/ios/android = implemented; DIAGNOSIS Real-visual-check section only reproduces desktop dark + reasons light.)
- Ratify the in-scope design choice: desktop LIVE OS tracking via a leaked D-Bus SettingChanged proxy (std::mem::forget). The task allowed load-time-only as the minimum; this goes further and intentionally leaks one proxy for the process lifetime. The comment justifies it (bounded, one-time, webview outlives the session) so it looks correct, just worth a human nod.
  (crates/webview-renderer/src/backend.rs:396-399 std::mem::forget(proxy).)
- Cosmetic: the done task's acceptance checkboxes are still unchecked ([ ]) in work/tasks/done/webview-follow-os-color-scheme.md even though the work landed and each criterion is met. No functional impact.
  (work/tasks/done/webview-follow-os-color-scheme.md Acceptance criteria block.)
