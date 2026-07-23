---
title: "Gate-3 conductor review: enable-web-inspector-devtools-all-platforms (APPROVE)"
date: 2026-07-23
status: approved
reviewOf: enable-web-inspector-devtools-all-platforms
gate: gate-3-conductor
mergedCommit: 8884e29
---

## Verdict: APPROVE

Conductor Gate-3 diff-vs-criteria pass. Gate-1 + Gate-2 passed before merge. Driven in place from `work/tasks/backlog/` via `dorfl do ... --allow-backlog --isolated --review --merge`.

## Done-move + landing

- `work/tasks/backlog/enable-web-inspector-devtools-all-platforms.md` -> `work/tasks/done/` on origin/main (squash merge `8884e29`).
- Files: desktop (`webview-renderer/src/backend.rs` +58, `webview-renderer/src/lib.rs` +45, `werust/src/main.rs` +105, `werust/Cargo.toml`), Android (`BrowserActivity.kt` +16), iOS (`WKWebViewShellController.swift` +15), `docs/platform-capability-matrix.toml` (+24), a spike README, a gating-decision note, the gate-2 nits note.

## Acceptance criteria (ticked against the diff)

- [x] Desktop: F12 opens the WebKitGTK Web Inspector in/over the window (real console REPL + network + DOM). `enable_developer_extras(developer_extras_enabled())` set on the WebView builder (debug-gated via `cfg!(debug_assertions)`), `show_inspector()` calls `WebView::inspector().show()`, wired to F12 in the shell. F12 chosen precisely to NOT collide with the GTK interactive debugger (Ctrl+Shift+I / Ctrl+Shift+D). The shortcut decision is a pure function `should_open_web_inspector(keyval, modifiers)` (F12 alone, lock-modifiers ignored, any real modifier rejected) — unit-testable and display-free.
- [x] iOS: `webView.isInspectable = true` gated on `#if DEBUG` + iOS 16.4+ availability check, so the page is inspectable via Safari Web Inspector over USB (always on the Simulator). Release build not silently inspectable.
- [x] Android: `WebView.setWebContentsDebuggingEnabled(true)` gated on `ApplicationInfo.FLAG_DEBUGGABLE`, so the page is inspectable via chrome://inspect over USB. Release build not silently inspectable.
- [x] Typeable console + network view on every platform via the platform's REAL inspector (WebKitGTK / Safari WebKit / Chrome DevTools) — no hand-built window. The earlier scoping correction (shift+F12 = GTK widget debugger, not web content; no custom window needed) is honoured.

Registered in `docs/platform-capability-matrix.toml` as capability `web-inspector`, state `implemented` on desktop/ios/android — satisfies the parity guard.

## Gating decision (made + recorded)

`work/notes/observations/web-inspector-devtools-gating-decisions-2026-07-23.md`: mobile inspectability + desktop developer-extras are DEBUG-BUILD gated (Decision 2) so a release build is not silently inspectable; F12 shortcut (Decision 1); capability name `web-inspector` (Decision 3). Consistent gating vocabulary across all three platforms.

## Forward-notes / drift honoured

Task carried the scoping correction (native inspector per platform, not a custom GTK window; F12 must avoid the GTK debugger key) and the "gate behind a debug build / setting and record the decision" instruction. Both honoured. Capability registered in the matrix as the task's "Where to look" block asked. No drift.

## Review-nits triage (Gate-2)

1. Frontmatter `covers: [2]` vs spec: spec story 2 is ENS name resolution (ronan.eth namehash->contenthash), unrelated to the web inspector — this task is the v0.2.2 human field-test request, not spec story 2. FLAGGED for the human: correct/remove `covers: [2]` so the spec-coverage map stays honest. Bookkeeping-only (the task is done + correct); does not affect the delivered code. Non-blocking. (Same field-test-vs-spec-story mismatch likely applies to the other v0.2.2 field-fix tasks carrying covers:[1]/[2]; worth one coverage-map cleanup pass by the human.)
2. Android gates on `ApplicationInfo.FLAG_DEBUGGABLE`, while the decision note named `BuildConfig.DEBUG`. The code comment explains the swap: FLAG_DEBUGGABLE is an equivalent debug signal and avoids extra buildConfig generation. RATIFIED — acceptable durable gate; both are the same "is this a debug build" signal.
3. Ratify Decisions 1/2/3 (F12 shortcut avoiding the GTK debugger; debug-build gate on all three; capability name `web-inspector`). All recorded, correct, coherent with the parity matrix. RATIFIED.

## Net effect

Real per-platform web devtools (typeable console + network) are now available on desktop (F12, in-window), iOS (Safari Web Inspector over USB), and Android (chrome://inspect over USB), all debug-gated. The human's v0.2.2 request ("a window that lets me see console + network, ideally type in the console") is satisfied using each platform's native inspector. One coverage-map hygiene item (nit 1) flagged for the human.
