---
title: review-gate non-blocking nits for 'trust-indicator-verified-vs-served' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: trust-indicator-verified-vs-served
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'trust-indicator-verified-vs-served' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify decision 1: new TrustPosture two-state enum + Renderer::trust_posture() seam method with a safe UnverifiedOrigin default. Touches every Renderer implementor but is non-breaking (native + android inherit the untrusted default, only WebViewRenderer overrides). Load-bearing seam shape, reversible.
  (crates/renderer/src/lib.rs TrustPosture + trust_posture; observations note decision 1)
- Ratify decision 3: install_ipfs registers the ipfs scheme DIRECTLY on the web context (not via the seam register_scheme_handler) so the closure can capture the non-Send Rc<RefCell<LoadLifecycle>> and mark it verified. Sound because the handler runs only on the single GTK thread, mirroring install_provider.
  (crates/webview-renderer/src/backend.rs install_ipfs; observations note decision 3)
- Ratify decision 4 user-visible default: nothing-loaded-yet shows the untrusted badge (glyph ✓ verified / ⚠ unverified origin, green/amber CSS). werust does not claim verification it has not proven.
  (crates/werust/src/main.rs trust_indicator; observations note decision 4)
- Doc drift: trust_indicator doc-comment says a shield vs a plain-globe glyph, but the actual label glyphs are the check mark and warning sign. Harmless comment mismatch, worth a one-line fix.
  (crates/werust/src/main.rs trust_indicator doc-comment vs the returned literals)
