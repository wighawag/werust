---
title: Gate-3 (conductor) verdict — native-renderer-t0-subset-path-behind-seam — APPROVE
date: 2026-07-21
kind: observation
reviewOf: native-renderer-t0-subset-path-behind-seam
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 2a418ee)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review.

### Acceptance criteria — all met

- ✅ A native (non-webview) `Renderer` backend (`NativeRenderer`) renders the T0
  v0 subset via the seam.
- ✅ Full pipeline end-to-end: tokenizer → allowlist tree builder → css cascade →
  block/inline flow layout → software-text paint (modules tokenizer/tree/css/
  layout/paint/pipeline/backend + a `render_subset` example).
- ✅ Plugs into the SAME `Renderer` seam (hot-swappable second backend), subject
  to the trust-hook qualification gate.
- ✅ Tests cover the subset render path: unit tests + `tests/subset_render.rs`
  integration render assertion through the seam.
- ✅ `Tokenizer | TreeBuilder` swap seam defined so T1 can swap in html5ever.

### FORWARD-NOTE HONOURED (conductor value confirmed)

The forward-note I planted after the trust-hook gate landed was followed exactly:
`NativeRenderer::trust_hooks()` declares `TrustHooks::none()` (NOT the fail-open
`all()` default), explicitly citing the note, and `tests/…` asserts
`r.trust_hooks() == TrustHooks::none()` so `qualify()` legitimately reports the T0
subset backend as NOT-yet-qualifying. The thesis is preserved: a subset renderer
that does not wire the trust hooks does not silently qualify.

### Nit triage

1. T0 navigates ONLY `data:text/html` and rejects fetch-requiring schemes
   (http(s)/ipfs) with `InvalidUrl` — RATIFY/KEEP. Same honesty discipline as the
   trust hooks: T0 does not fail-open-claim a fetch capability it lacks (that is
   stories 8/9/12). Coherent, reversible, documented.
2. **`percent_decode` maps `+` -> space** (form-urlencoding semantics), which is
   NOT correct for `data:` URIs per RFC 2397 (`+` is a literal plus there). Low
   impact today (tests encode `+` as `%2B`; T0 fixtures are authored), but a latent
   bug: a T0 `data:` doc with a literal `+` would be silently corrupted to a space.
   APPROVED (no acceptance criterion violated, contained to an internal helper),
   captured as the follow-up below rather than expanding this landed task.

### Follow-up captured (not tasked here)

`native-renderer` `data:`-URL `percent_decode` treats `+` as space (RFC 2397 says
literal `+`). Cheap fix: drop the `+`->space branch so the `data:` decoder matches
`data:` semantics. Low priority (T1 swaps html5ever in for tokenizing, not this
`data:` decoder; fixtures are authored). Recorded for whoever hardens the T0/T1
`data:` path.

### What this unlocks

Landing T0 native unlocks: `t0-server-web-floor-golden-fixtures` and
`t1-whatwg-parser-html5ever-behind-tokenizer-seam` (both blockedBy the T0 native
path). It is also a co-dep of `t0-content-addressed-floor-parity`.
