---
title: review-gate non-blocking nits for 'renderer-seam-trust-hook-qualification-gate' (Gate 2 approve)
date: 2026-07-21
status: open
reviewOf: renderer-seam-trust-hook-qualification-gate
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'renderer-seam-trust-hook-qualification-gate' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the fail-open default: Renderer::trust_hooks defaults to TrustHooks::all() (qualifying), so a new backend that stubs the hook methods PASSES the gate unless it explicitly overrides to drop a hook. Is a fail-open default the posture you want, vs fail-closed (none()) forcing each backend to opt in?
  (Recorded in the PR Decisions block. Reversible (flipping to none() only tightens); the only real backend today is the webview which satisfies both hooks, and real hook behaviour is asserted by the sibling provider/ipfs tasks. Documented at the Renderer::trust_hooks / TrustHooks::default doc sites. TOUCHES native-renderer-t0-subset-path-behind-seam and native-renderer-benchmark-harness tasks, which inherit this default.)
- Ratify the design choice: qualification is a runtime declared-capability value (TrustHooks) checked by qualify(&dyn Renderer), layered on top of the mandatory hook methods, rather than a compile-time trait bound.
  (PR Decisions block. Justified: structural presence of the mandatory methods cannot express the 'renders but cannot' case criterion 2 requires; the seam already uses dyn Renderer at run time (benchmark harness + T0 evaluate trait objects), so a runtime pass/fail gate that names the missing hook is what downstream consumers need. Coherent with the benchmark spec's pass/fail (not graded) framing.)
