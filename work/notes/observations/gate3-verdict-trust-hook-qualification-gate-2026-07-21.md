---
title: Gate-3 (conductor) verdict — renderer-seam-trust-hook-qualification-gate — APPROVE (with fail-open design flag)
date: 2026-07-21
kind: observation
reviewOf: renderer-seam-trust-hook-qualification-gate
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 4912a21)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review.

### Acceptance criteria — all met

- ✅ Trust-hook capability is a required, checkable CONTRACT (a `TrustHook` enum
  {ProviderInjection, IpfsScheme}, a `TrustHooks` set, `Renderer::trust_hooks()`,
  and a `qualify(&dyn Renderer)` gate) — a value, not a comment.
- ✅ Conformance tests: qualify ACCEPTS a both-hooks backend, REJECTS a render-only
  backend (`qualification_gate_rejects_a_render_only_backend`), and rejects a
  missing-one-hook backend. `Disqualified` names the missing hook.
- ✅ WebKitGTK backend passes qualification (`trust_hooks() -> TrustHooks::all()`).
- ✅ Tests under the verify gate, repo style (10 passed, 1 ignored).

### Nit triage

1. **Fail-OPEN default** (`trust_hooks()` defaults to `all()`) — see the design
   flag below. KEEP for now (correct + reversible), but ACTIONED: (a) forward-note
   planted on `native-renderer-t0-subset-path-behind-seam` telling it to declare
   `trust_hooks()` honestly (not fail-open), and (b) raised as a stuck-set design
   question for human ratify (fail-open vs fail-closed default).
2. Runtime declared-capability value vs compile-time trait bound — KEEP. Sound:
   structural method presence cannot express the "renders but cannot" case; the
   seam already uses `dyn Renderer` at runtime (benchmark harness + T0).

### Design flag carried to the stuck-set (reversible, non-blocking)

`Renderer::trust_hooks()` defaults to `TrustHooks::all()` (fail-OPEN): a backend
that stubs the hook methods and does NOT override `trust_hooks()` PASSES the gate.
This subtly weakens the thesis the gate exists to enforce ("a backend qualifies
only if it can satisfy the trust hooks") for FUTURE backends — a native renderer
that merely stubs the hooks would silently qualify without wiring them. It is fine
today (the only real backend, the webview, genuinely satisfies both; sibling tasks
assert real hook behaviour) and reversible (flipping to `none()` default only
tightens). I APPROVED the task (meets every criterion, reversible) and mitigated
the immediate downstream risk with the forward-note on the T0 native task, but the
DEFAULT POSTURE (fail-open vs fail-closed) is a genuine design judgement worth a
conscious human ratify — surfaced in the end-of-run stuck-set.

### What this unlocks

Landing this + renderer-seam unlocks the native-renderer branch:
`native-renderer-t0-subset-path-behind-seam` (once its other deps are met) and,
downstream, `native-renderer-benchmark-harness-capability-and-trust-hooks`.
