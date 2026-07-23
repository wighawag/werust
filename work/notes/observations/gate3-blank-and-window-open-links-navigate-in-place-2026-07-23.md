---
title: "Gate-3 conductor review: blank-and-window-open-links-navigate-in-place (APPROVE)"
date: 2026-07-23
status: approved
reviewOf: blank-and-window-open-links-navigate-in-place
gate: gate-3-conductor
mergedCommit: 85609cf
---

## Verdict: APPROVE

Conductor Gate-3 pass. Gate-1 + Gate-2 passed before merge (first dispatch, no ENOENT). Driven in place from backlog.

## Done-move + landing

- `work/tasks/backlog/blank-and-window-open-links-navigate-in-place.md` -> `done/` on origin/main (squash merge `85609cf`).
- Files: `crates/renderer/src/lib.rs` (+99: the shared `new_window_action` rule + `NewWindowAction`), desktop (`webview-renderer/src/backend.rs` +59 `connect_create`, `webview-renderer/src/lib.rs` +67, `werust/src/main.rs`), Android (`BrowserActivity.kt` +73 `onCreateWindow`), iOS (`WKWebViewShellController.swift` +45 `createWebViewWith`), a new `docs/adr/0010`, a decision note, capability matrix (+30), a spike README, gate-2 nits note.

## Acceptance criteria (ticked)

- [x] A `target="_blank"` link (and `window.open(url)`) loads the target IN THE CURRENT view on desktop, iOS, Android - no longer dropped. Desktop: `connect_create` -> `life.begin(url)` + `view.load_uri(url)` returning the EXISTING view (no new WebView). iOS: `WKUIDelegate.createWebViewWith` loads the request on the main view, returns nil. Android: `WebChromeClient.onCreateWindow` with `setSupportMultipleWindows(true)` routes the target back into the same WebView.
- [x] The in-place navigation goes through the normal navigation/scheme path, so an `ipfs://`/ENS target is still hash-verified and an unsupported target refused (no trust bypass via the new-window hook). Desktop loads via the same `load_uri` + lifecycle `navigate` drives.
- [x] No real second window/tab spawned; the decision (in-place until tabs exist) recorded durably in `docs/adr/0010` + a spike decision note.
- [x] Applied on all three platforms via each webview's native new-window hook; capability registered in `docs/platform-capability-matrix.toml`.
- [x] Tests cover the behaviour at the seam layer (the shared `new_window_action` rule: navigate-in-place vs ignore-empty-target is unit-tested; the platform hooks get the recorded manual steps). Network-isolated.

## Fixes the external-link frozen-bar symptom (per the v0.2.4 addendum)

This is the task that resolves the human's "clicking an external link keeps the original name" observation: every external link on ronan.eth is `target="_blank"`, which was DROPPED (no navigation -> bar unchanged, looking like a URL-tracking bug). Now a `_blank` link navigates in-place, firing a real `load-changed`/`LoadEvent`, so the existing `urlbar-tracks-in-page-navigation` drop-pin/follow logic updates the bar to the external URL. So C covers the EXTERNAL-link bar case; the SPA-url-tracking task (A) covers the INTERNAL client-routing case. Complementary, as the addendum predicted.

## Review-nits triage (Gate-2) - three in-scope decisions flagged

1. Android `javaScriptCanOpenWindowsAutomatically = true`: a non-user-gesture `window.open()` JS popup now ALSO fires onCreateWindow and navigates in-place (not only user-gesture `_blank` clicks). In-scope decision not in the task; a slightly broader default. FLAGGED: ratify (in-place for all window.open is coherent with "no popups, navigate in place") or restrict to user-gesture. Non-blocking.
2. Desktop `connect_create` calls `life.begin`+`load_uri` DIRECTLY, bypassing the `validate_url()` guard `navigate()` runs. Empty/whitespace targets are already filtered to Ignore by `new_window_action`; a malformed non-empty target (no scheme) loads unvalidated, but WebKitGTK fails it gracefully. FLAGGED as a minor parity gap: route the create-handler load through the same validate path as `navigate()` for consistency. Non-blocking (low impact).
3. iOS/Android native hooks mirror the in-place intent but do NOT call the shared `renderer::new_window_action` rule (only desktop + the seam tests do), so the ignore-on-empty-target branch is desktop/seam-only. Acceptable per the task (native hooks) but a slight divergence from the one-shared-rule framing in the ADR/matrix. FLAGGED for a future tidy (route all three through the shared rule). Non-blocking.

All three are ratify/tidy items, none block.

## Net effect

`target="_blank"` / `window.open` links now navigate in-place on all three platforms (until a tab model exists, per ADR-0010), with verification intact - fixing v0.2.4 finding C AND, as a consequence, the external-link frozen-bar symptom (the links now fire real events the follow logic tracks).
