---
title: Gate-3 (conductor) verdict — browser-shell-url-bar-and-live-interactive-view — APPROVE
date: 2026-07-21
kind: observation
reviewOf: browser-shell-url-bar-and-live-interactive-view
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 85851de)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review.

### Acceptance criteria — all met

- ✅ URL bar: typed URL navigates through the seam and updates the chrome.
- ✅ Back / forward / reload / stop work and reflect navigation state (10 shell
  tests, incl. mid-history navigation dropping forward entries).
- ✅ Live interactive view: input (scroll/click/focus/keyboard) reaches the page.
- ✅ Chrome reflects load lifecycle (started/committed/finished/failed → status
  line, with failure precedence).
- ✅ Tests cover shell<->seam wiring at the seam boundary (via a fake backend).

### FORWARD-NOTE HONOURED (conductor value confirmed)

The input-no-op forward-note I planted after the renderer seam landed was followed
exactly: `shell.rs` docs cite the forward-pointer and make the embedded page
interactive by embedding the live `ViewHandle` widget + focus (OS/GTK routes raw
input natively); the webview's `send_*` forwarders are correctly left as deliberate
no-ops, NOT wired. Input-forwarding is tested at the seam boundary (focus through
the seam), not by asserting the no-ops move anything.

### Nit triage

1. Seam EXTENDED with session-history verbs (`go_back`/`go_forward`/`can_go_back`/
   `can_go_forward`, no-op/false defaults) rather than a shell-owned URL stack —
   RATIFY/KEEP. Coherent: defaults keep the native T0 backend compiling without
   override (verified); matches the seam layer. Load-bearing but clean; the
   default-based non-breaking extension pattern is now established for future
   backends (they inherit no-op/false and override if they have history).
2. Real WebKitGTK back/forward walk only pinned by an `#[ignore]`d test (needs a
   display) — KEEP. Inherent CI constraint (no display headless); explicitly
   acceptable per the forward-pointer ("test at the seam boundary via a fake").
   The seam contract IS tested; the display-only real-webview walk is correctly
   `#[ignore]`d. Residual coverage risk noted, unavoidable, correctly handled.

### What this unlocks

browser-shell landing unlocks: `eip1193-provider-injection-via-script-bridge`,
`mobile-android-shell-and-static-lib`, `mobile-ios-shell-and-static-lib`, and (as
a co-dep) `ipfs-scheme-resolution-through-renderer-seam`.
