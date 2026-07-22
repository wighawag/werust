---
title: Decisions — trust-indicator-verified-vs-served
date: 2026-07-22
kind: observation
reviewOf: trust-indicator-verified-vs-served
---

## Decisions taken while surfacing the trust posture in the chrome

Recorded for reviewer/human ratification (per the durable-decision rule). None are load-bearing/hard-to-reverse; the build proceeded. The done record for `trust-indicator-verified-vs-served` should link here.

1. **New `TrustPosture` concept + `Renderer::trust_posture()` on the seam** (a new named concept at the seam layer; TOUCHES every `Renderer` implementor). `TrustPosture` is a two-state enum `{ UnverifiedOrigin, ContentVerified }` reported by the backend and read by the shell into `ChromeState`, exactly as `load_state` is. Coherence: the names come verbatim from `docs/adr/0001` ("this was content-verified" vs "this was served by an unverified origin"), so nothing existing is re-meant; it does not overlap `TrustHook`/`TrustHooks` (those are the backend-QUALIFICATION capability set, a different axis: "can this backend satisfy the hooks" vs "how did the CURRENT page load"). The trait method carries a safe default (`UnverifiedOrigin`), so every existing implementor keeps compiling and a backend with no verified path is never mislabelled verified. This mirrors the earlier recorded seam-extension decisions (session-history verbs, `trust_hooks`) — a load-bearing seam shape another author will build on, hence recorded, but reversible. Choice sites: `renderer::TrustPosture`, `renderer::Renderer::trust_posture`.

2. **The posture is driven by the ACTUAL load path, not the URL string** (the task's core AC). `LoadLifecycle::begin` resets the posture to `UnverifiedOrigin` on every fresh load; it is upgraded to `ContentVerified` ONLY by `LoadLifecycle::mark_content_verified`, which the `ipfs://` scheme handler calls after `resolve_ipfs_request` returns hash-verified bytes. A hash mismatch fails the load (the resolver returns an error) and never reaches the mark, so a page whose URL merely looks like `ipfs://` but did not actually verify is never reported verified. Choice sites: `webview_renderer::LoadLifecycle::{begin,mark_content_verified,posture}`.

3. **`WebViewRenderer::install_ipfs` registers the `ipfs` scheme DIRECTLY on the web context** (not through the seam's `register_scheme_handler`) so its closure can capture the `Rc<RefCell<LoadLifecycle>>` and mark it verified. Reason: the seam's `SchemeHandler` is `Send` (a generic backend may move it across threads), but the shared lifecycle is `Rc`-based and NOT `Send`. The webview runs this handler only on its single GTK thread, so this is sound — it is exactly the same non-`Send` wiring `install_provider` already uses for its live-page response push. The seam's `register_scheme_handler` is left unchanged for generic backends. Choice site: `webview_renderer::WebViewRenderer::install_ipfs`.

4. **Chrome presentation: a `✓ verified` / `⚠ unverified origin` badge in the toolbar**, with a tooltip explaining each state and a green/amber CSS class so the two states are visually distinct and legible (`docs/adr/0001`: the trust posture is a product surface). The label text carries a glyph so the states read even before colour. Pure `trust_indicator` / `trust_indicator_detail` functions of `ChromeState` keep it testable without a display. USER-VISIBLE default: nothing-loaded-yet shows the untrusted badge — werust does not claim verification it has not proven. Choice sites: `werust::{trust_indicator,trust_indicator_detail,install_trust_indicator_css}` in `crates/werust/src/main.rs`.
