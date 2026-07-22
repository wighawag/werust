---
title: Gate-3 (conductor) verdict — trust-indicator-verified-vs-served — APPROVE
date: 2026-07-22
kind: observation
reviewOf: trust-indicator-verified-vs-served
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 7dce1db)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review.

### Acceptance criteria — all met

- ✅ The chrome shows a clear indicator: content-verified (✓, green) vs unverified
  served origin (⚠, amber), via `BrowserShell::is_content_verified()` +
  `werust`'s `trust_indicator`.
- ✅ Driven by the ACTUAL load path, NOT the URL scheme: `trust_posture` flips to
  content-verified ONLY when a load's bytes came back through the hash-verified
  content-addressed path (`resolve_ipfs_request` returning verified bytes); a plain
  served load never flips it.
- ✅ Two states visually distinct + legible (green/amber, ✓/⚠).
- ✅ Tests assert the indicator tracks the real verification path (only a
  verified-path load flips the posture).

### Nit triage

1. New `TrustPosture` enum + `Renderer::trust_posture()` seam method, safe
   `UnverifiedOrigin` default — RATIFY/KEEP. Non-breaking (native + android inherit
   the untrusted default; only WebViewRenderer overrides); reversible; same
   established default-based seam-extension pattern.
2. `install_ipfs` registers the ipfs scheme DIRECTLY on the web context (not via the
   seam `register_scheme_handler`) so the closure can capture the non-Send
   `Rc<RefCell<LoadLifecycle>>` to mark verified — KEEP. Sound: handler runs only on
   the single GTK thread, mirrors `install_provider`.
3. Nothing-loaded shows the UNTRUSTED badge — RATIFY/KEEP. Correct fail-CLOSED
   default: werust does not claim verification it has not proven. (Nicely the
   OPPOSITE posture to the trust-hook `trust_hooks()` fail-OPEN default flagged
   earlier — the indicator is fail-closed, as a trust surface should be.)
4. Doc drift: `trust_indicator` doc says "shield vs plain-globe glyph" but the actual
   glyphs are ✓/⚠ — benign comment mismatch (captured, not fixed; landed code).

### What this unlocks

Leaf task (covers story 7). Makes the trust posture a product surface — the
user-facing payoff of the verify-don't-trust thesis.
