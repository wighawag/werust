---
title: Real WebViewRenderer test aborts with SIGABRT at process teardown
date: 2026-07-21
kind: observation
---

Running the `#[ignore]`d `real_webview_backend_qualifies` test (constructs a real `WebViewRenderer` via GTK init on a desktop session) the test assertion PASSES but the test *process* then aborts with SIGABRT during teardown (`crates/webview-renderer/src/lib.rs`). Looks like a GTK/WebKit shutdown-on-exit issue, not a test-logic failure, and it is why that test stays `#[ignore]` (kept out of the headless `verify` gate). Out of scope for the qualification-gate task; captured for whoever wires real end-to-end webview tests.
