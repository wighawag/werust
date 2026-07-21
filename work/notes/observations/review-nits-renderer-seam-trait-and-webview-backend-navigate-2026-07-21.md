---
title: review-gate non-blocking nits for 'renderer-seam-trait-and-webview-backend-navigate' (Gate 2 approve)
date: 2026-07-21
status: open
reviewOf: renderer-seam-trait-and-webview-backend-navigate
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'renderer-seam-trait-and-webview-backend-navigate' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: current_url returns owned Option<String> instead of Option<&str> so a signal-driven RefCell-backed backend can implement the seam. Recorded in the Decisions block and at the doc-comment site; sound and load-bearing-but-reversible.
  (crates/renderer/src/lib.rs current_url doc; spike README Decisions)
- Ratify: new backend crate webview-renderer added as a peer of native-renderer (both depend on renderer). Recorded in Decisions; reuses the established backend-crate shape, coherent with the CONTEXT.md webview-now/native-later term.
  (Cargo.toml members; spike README Decisions)
- Un-recorded in-scope decision: the script-message bridge is one-directional (register_script_message_handler = page->browser; inject_script = browser->page at document-start only). The EIP-1193 round-trip task needs a browser->page response push (e.g. evaluate_javascript). Fine to defer since that task owns wiring the hook, but flag so the seam gap is a conscious hand-off, not a surprise.
  (crates/renderer/src/lib.rs trait; work/tasks/ready/eip1193-provider-injection-via-script-bridge.md acceptance requires round-trip back to page)
- send_pointer/send_key/send_scroll are no-ops on the WebKitGTK backend (documented as relying on GTK's own input routing to the focused widget). Recorded at the method sites; reasonable for a webview but confirm the live-interactive-view task (story 2) is aware these hooks do nothing here.
  (crates/webview-renderer/src/backend.rs send_* impls)
