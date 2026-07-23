---
title: "Gate-3 conductor review: clearer-loading-and-error-indicator (APPROVE)"
date: 2026-07-23
status: approved
reviewOf: clearer-loading-and-error-indicator
gate: gate-3-conductor
mergedCommit: aa7f2f8
---

## Verdict: APPROVE

Conductor Gate-3 diff-vs-criteria pass. Gate-1 + Gate-2 passed before merge. Driven in place from `work/tasks/backlog/` via `dorfl do ... --allow-backlog --isolated --review --merge`. Last of the 5 v0.2.2 field-fix tasks.

## Done-move + landing

- `work/tasks/backlog/clearer-loading-and-error-indicator.md` -> `work/tasks/done/` on origin/main (squash merge `aa7f2f8`).
- Files: core (`werust-core/src/lib.rs` +598), desktop (`werust/src/main.rs` +166), Android (`BrowserActivity.kt`, `WerustCore.kt`, `rust/src/ffi_json.rs`), iOS (`WKWebViewShellController.swift`, `WerustCore.swift`, `rust/src/ffi_json.rs`), `docs/platform-capability-matrix.toml` (+33, two new rows), a DECISIONS.md, the gate-2 nits note. +1083/-29 across 12 files.

## Acceptance criteria (ticked against the diff)

- [x] A slow load shows clear ongoing activity + a step/status hint. New `LoadStep` axis on `ChromeState`: Idle / ResolvingName / FetchingRecord / FetchingContent / Rendering, shown WHILE `is_loading()`. A slow load reads as "working: fetching content", not frozen. Human-readable hint (`"fetching content"`) + stable wire name (`"fetching-content"`).
- [x] Transient/timeout failure surfaced distinctly from hard, with an obvious retry affordance; hard failures keep their prominent protocol-named reason. New `FailureKind::classify(reason) -> Transient|Hard` (pure classifier over the existing `last_error` string). Hard markers checked FIRST (`did not verify` / `hash mismatch` / `expired`) so a verification failure is never falsely retryable; then transient markers (`timeout` / `timed out` / `transport error` / `connection` / `io error`). Transient banner reads "timed out — reload to retry"; retry reuses the existing Reload (a transient ENS failure re-resolves from the pinned name — dovetails with T3). Hard wording/behaviour unchanged (protocol-named reason preserved, `prominent-load-failure` parity kept).
- [x] Step reflects the REAL pipeline (name -> record -> content -> render) driven by actual lifecycle events, not faked. `LoadStep` is set through the real `ens::resolve` -> `ipns::resolve_ipns_name` -> ipfs:// path -> backend `Started`/`Committed`/`Finished` lifecycle.
- [x] Applied desktop + mobile; no re-meaning of trust posture. `LoadStep` + `FailureKind` are a THIRD/FOURTH independent axis alongside `LoadState` (lifecycle) and `TrustPosture` (load path) — deliberately NOT new `LoadState`/`TrustPosture` variants (loading/error stay orthogonal to trust, per the prior two tasks). Both facts reach all four edges over the shared chrome / FFI chrome JSON (`loadStep`, `failureKind` — additive fields). Two capability matrix rows added (`loading-progress`, `retryable-error-distinction`) `implemented` on desktop/ios/android.
- [x] Tests cover loading/progress states + the transient-vs-hard distinction (fake backend, slow/timeout/hard-fail), network-isolated. Core + both `ffi_json` crates carry the tests; the string classifier and the step-derivation are unit-pinned.

## Forward-notes / drift honoured

Task carried the "no re-meaning of the trust posture; loading/error orthogonal to trust" constraint and the "driven by actual lifecycle events, not faked" constraint. Both honoured (the DECISIONS.md coherence argument is explicit that `LoadStep`/`FailureKind` do not overlap `TrustPosture`/`LoadState`). No drift.

## Review-nits triage (Gate-2)

1. Ratify Decision 1 (LoadStep is a new orthogonal axis, not a LoadState/TrustPosture variant, driven by the real pipeline). RATIFIED — coherent with the glossary, correct layering.
2. Ratify Decision 2 (transient-vs-hard is a pure classifier over the last_error STRING, not a typed retryable flag threaded through core; retry reuses reload). RATIFIED — keying on the string is the RIGHT call: the webview `LoadEvent::Failed` path hands back no typed error, so the string is the only universal denominator across both failure paths. A typed flag would classify the two paths incoherently.
3. CLASSIFIER-COVERAGE GAP (the one real finding, non-blocking): `IpnsError::Source` also covers a non-2xx gateway status / empty body, whose surfaced reason `"IPNS record fetch failed: <detail>"` carries NO transient marker, so it classifies as Hard and offers no retry — yet a 500 / empty-body is often retryable. Direction is SAFE (a conservative no-false-retry-promise; no acceptance criterion is violated — a genuinely-retryable case merely isn't OFFERED retry), so non-blocking. But DECISIONS.md's claim that "a record-fetch source failure is Transient" slightly OVERSTATES coverage: only the transport-worded subset actually classifies transient. FLAGGED for the human as a small follow-on candidate: either widen the classifier to treat an IPNS record-fetch source failure (5xx / empty-body) as transient, or tighten the DECISIONS.md claim to match the code. Safe to ship as-is.
4. `covers: [2]` maps this loading/error-UX task to spec story 2 (honest trust labelling); the mapping is loose (loading/error are orthogonal to trust, which the task correctly preserves). Same coverage-map looseness flagged on `enable-web-inspector-devtools-all-platforms` (nit 1 there). FLAGGED for the human: one coverage-map cleanup pass across the v0.2.2 field-fix tasks would tighten the spec-coverage honesty. No code impact.

## Net effect

A slow load now reads as progressing (step hint), and a transient timeout is surfaced distinctly from a hard failure with a reload-to-retry affordance, on all four edges — closing the weak-feedback half of the v0.2.2 field finding. Two off-path items for the human: the IPNS-record 5xx/empty-body classifier-coverage gap (nit 3, safe as-is), and the `covers:` coverage-map looseness across the field-fix tasks (nit 4).
