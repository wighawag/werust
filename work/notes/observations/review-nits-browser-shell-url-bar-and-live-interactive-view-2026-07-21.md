---
title: review-gate non-blocking nits for 'browser-shell-url-bar-and-live-interactive-view' (Gate 2 approve)
date: 2026-07-21
status: open
reviewOf: browser-shell-url-bar-and-live-interactive-view
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'browser-shell-url-bar-and-live-interactive-view' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the recorded decision to EXTEND the Renderer seam with session-history verbs (go_back/go_forward/can_go_back/can_go_forward) rather than a shell-owned URL stack. This touches every Renderer implementor; it is coherent (defaults are no-op/false so the native T0 backend keeps compiling, verified) and matches the seam layer, but it is a load-bearing shape another author will build on.
  (docs/spikes/browser-shell-url-bar-and-live-interactive-view/DECISIONS.md; crates/renderer/src/lib.rs adds 4 trait methods with defaults; native-renderer/src/backend.rs relies on the defaults (no override).)
- The end-to-end walk of the REAL WebKitGTK back/forward list is only pinned by an #[ignore]d test (needs a display); interactive back/forward over real pages is never asserted in CI. Acceptable per the task forward-pointer (test at the seam boundary via a fake), but worth noting as residual coverage risk.
  (crates/webview-renderer/src/lib.rs real_webview_history_starts_empty is #[ignore]; the display-free contract only asserts empty history.)
