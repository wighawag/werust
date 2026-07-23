# Mobile EIP-1193 provider injection + trust indicator: non-obvious decisions

2026-07-23, task `mobile-provider-injection-and-trust-indicator` (spec `ens-to-ipfs-resolution-phase1-rpc-skeleton`). Recorded per the task's "record non-obvious mobile-bridge decisions durably" instruction. Linked from the done record.

## Context re-check (drift gate)

Before building, I re-checked current reality (the prompt's FIRST step): the blocker `mobile-ipfs-scheme-interception-ios-and-android` HAS landed (in `work/tasks/done/`; the `ipfs-render` + `retrieval-backend` matrix cells are `implemented`, and both mobile backends now wire `register_scheme_handler` + `install_ipfs` through the shared core). The two seam methods this task targets were still empty `{}` no-ops (`register_script_message_handler` / `inject_script`), and `trust_posture` / `mark_ens_origin` / `mark_mutable_name` still inherited the seam defaults. The matrix cells for `eip1193-provider` + `trust-indicator` were already pointed at THIS task slug (the parity-guard seed did that), so "repoint off the ipfs task" was already done; this task flips them from `stubbed` to `implemented`. No drift: the premise held, so I built.

## Decision 1 — one shared two-axis posture rule (`TrustPosture::after_verify`), not three copies

The two-axis "show the LOUDEST applicable warning" rule (ENS-trusted-RPC beats mutable-name beats plain content-verified) lived ONLY inside `webview-renderer::LoadLifecycle::mark_content_verified` (a GTK crate the mobile backends deliberately do NOT depend on). Rather than fork that load-bearing trust-security rule into each mobile backend's `Inner` (three copies to drift apart), I lifted the pure rule into `renderer::TrustPosture::after_verify(ens_origin, mutable_name)` next to the `TrustPosture` enum, and made desktop's `LoadLifecycle::mark_content_verified` delegate to it too. So all three backends compute the surfaced posture from ONE source of truth.

- Touches: `crates/renderer` (new pure fn + test), `crates/webview-renderer` (desktop `LoadLifecycle` now delegates — behaviour-preserving, its 20 tests stay green), both mobile backends.
- Alternative considered: duplicate the `if ens_origin {..} else if mutable_name {..}` ladder in each mobile `Inner`. Rejected: a muddled/duplicated trust rule that compiles is exactly the class of debt the coherence check warns against.

## Decision 2 — mobile posture lives in the backend `Inner`, not a shared `LoadLifecycle`

Desktop shares a single `LoadLifecycle` (GTK crate) between the backend and the webview's signal closures. The mobile backends already roll their own `Inner` (an `Rc<RefCell>` session-history + load-lifecycle) and cannot pull GTK. So the trust-axis state (`posture`, `ens_origin`, `mutable_name`) lives in the mobile `Inner`, reset on `begin` (mirroring `LoadLifecycle::begin`), with a `mark_content_verified` the session's `resolve_ipfs` calls on a verified resolution — the exact structural twin of the desktop `install_ipfs` scheme handler calling `life.borrow_mut().mark_content_verified()`. The seam methods (`trust_posture` / `mark_ens_origin` / `mark_mutable_name`) read/write that state, and `trust_hooks` now returns `TrustHooks::all()` so the backend qualifies (the no-ops previously left it fail-closed disqualified — asserted by the new `the_backend_opts_into_both_trust_hooks` tests).

## Decision 3 — the provider response push is QUEUED (mobile owns no live view)

Desktop's `evaluate_javascript` runs the EIP-1193 response JS immediately on the GTK loop via a captured `WebView` clone. The mobile backends own no live view, so `evaluate_javascript` QUEUES the response JS into an eval queue the OS edge drains (`take_pending_eval` -> `handle_provider_message` returns it) and runs via `WebView.evaluateJavascript` / `WKWebView.evaluateJavaScript`. This keeps the shared `werust_core::provider` round-trip (`provider_shim` / `route_provider_message` / the keyless read-only `ProviderBridge`) unchanged — mobile does not fork the provider path.

### Sub-decision 3a — the eval queue is `Arc<Mutex<Vec<String>>>`, not part of the `!Send` `Inner`

The seam's `ScriptMessageHandler` is `Box<dyn FnMut(..) + Send>` (so a generic backend could move it across threads), but the mobile `Inner` is `!Send` (it holds `Rc`s). So the provider handler cannot capture the whole backend handle. It captures a `Send` clone of JUST the eval queue (`Arc<Mutex<Vec<String>>>`), the mobile twin of the desktop `install_provider` closure capturing a cloneable view handle. This is why `pending_eval` is an `Arc<Mutex<_>>` field inside the otherwise-`Rc<RefCell>` `Inner`.

## Decision 4 — the provider posts synchronously through a per-platform bridge

The shared shim posts to `window.webkit.messageHandlers.werustProvider.postMessage(...)`. The native resolve is synchronous (a keyless read-only stub), so:

- iOS: `WKWebView` natively exposes `window.webkit.messageHandlers.<name>` for a registered `WKScriptMessageHandler`, so the Swift `ProviderBridgeHandler` receives the envelope and pushes the response with `evaluateJavaScript` — no page-side preamble needed.
- Android: `WebView` has no `window.webkit`, so a tiny document-start PREAMBLE defines `window.webkit.messageHandlers.werustProvider.postMessage` to call a `@JavascriptInterface` (`werustProviderBridge.postMessage`) that returns the response JS synchronously, which the preamble `eval`s inline. The preamble + the shared shim are injected together at `onPageStarted` (the earliest edge hook without androidx's document-start user-script API).

Alternative considered (Android): androidx `WebViewCompat.addDocumentStartJavaScript`. Rejected for now to avoid adding an androidx dependency to the minimal app module; `onPageStarted` injection is sufficient for the simulator/dev app. If a page's own inline `<head>` script races the injection on-device, revisit with the androidx document-start API (noted here so a later task has the pointer).

## Decision 5 — the trust posture is a new stable chrome-JSON wire field `trustPosture`

The chrome JSON both mobile edges decode gained a `trustPosture` field with stable lower-kebab names (`unverified-origin` / `content-verified` / `name-via-trusted-rpc` / `mutable-name`), identical across the Android + iOS `ffi_json` encoders, so both edges paint the SAME four-state indicator the desktop chrome shows (`✓ verified` / `◈ name via trusted RPC` / `◇ content verified, mutable name` / `⚠ unverified origin`). The field is additive; existing chrome fields are unchanged.
