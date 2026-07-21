---
title: Gate-3 (conductor) verdict — t0-server-web-floor-golden-fixtures — APPROVE
date: 2026-07-21
kind: observation
reviewOf: t0-server-web-floor-golden-fixtures
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit d7fac77)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review.

### Acceptance criteria — all met

- ✅ Committed golden fixtures: article/headings/inline-styles/lists `.html`
  (v0-allowlist fragments) + their `.golden.txt` references, under
  `crates/native-renderer/tests/fixtures/t0-server-floor/`.
- ✅ Native T0 path renders each fixture and asserts stability vs its golden
  (`tests/t0_server_floor_goldens.rs`).
- ✅ Subset-doc-drift guard: `css::SUPPORTED_PROPERTIES`/`is_supported_property`/
  `is_supported_selector` + `tree::ELEMENT_ALLOWLIST` keep fixtures in the v0 set.
- ✅ Runs under the `verify` gate, fails on regression.

### Nit triage

1. New public css API (`SUPPORTED_PROPERTIES`, `is_supported_property`,
   `is_supported_selector`) added without a Decisions block — KEEP. Sensible
   machine-readable companion to the element allowlist, test-covered; a
   traceability nit only.
2. **Golden transcript captures bold/italic/underline marks but NOT colour**,
   though `transcribe()`'s doc claims colour and the fixtures are colour-heavy
   (universal `*`, `.class` colour, inline colour, cascade order). A colour-cascade
   regression would NOT turn a golden red — the guard under-covers what the
   fixtures exercise, and the doc overclaims. APPROVED (criteria met by the
   structural/text/mark goldens; colour is a pre-existing paint limitation, not
   introduced here), captured as the follow-up below.
3. `is_supported_selector` does its own single-token validation rather than reusing
   `parse_selector` (which accepts malformed `.class`/`#id`) — KEEP. Correct for a
   drift guard whose job is to reject; already recorded in
   `parse-selector-accepts-malformed-class-id.md`. Fork-divergence risk noted.

### Follow-up captured (not tasked here)

The T0 golden transcript does not assert COLOUR, so the colour-cascade behaviour the
fixtures declare is unguarded, and `paint::transcribe()`'s doc-comment overclaims
that it captures colour. Two cheap fixes for a later CSS task (e.g. t1-core-css or a
paint-hardening pass): (a) extend the golden transcript to encode resolved colour so
colour-cascade regressions turn a golden red, and (b) correct the `transcribe` doc
to state exactly what it captures. Low priority now (structure/text/marks ARE
guarded); flagged so the T0/T1 colour path gets a real regression net.

### What this unlocks

This is the "server floor" half of T0. T0 is only fully "reached" when the
content-addressed floor (`t0-content-addressed-floor-parity`) also lands. Landing
this does not by itself unlock new tasks (t0-content-addressed-floor-parity also
needs the ipfs path).
