---
title: "Clearer loading + error indicator: progress/state feedback while loading, and a distinct retryable-timeout vs hard-failure surface"
slug: clearer-loading-and-error-indicator
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [2]
---

## What to build

Give the user clearer feedback about what is happening during a load. FIELD FINDING (v0.2.2, human): "we should have a better indicator of loading or error." Today the trust indicator shows a neutral loading badge (from `chrome-loading-state-resets-trust-indicator`) and a failed load raises the prominent error banner (from `prominent-load-failure-and-ipns-resolution-diagnosis`), but a SLOW / PARTIAL load (common while the whole-DAG/timeout issues exist) has weak feedback: the user cannot tell it is still progressing vs stuck, and a TRANSIENT timeout (retryable — a reload works) is surfaced the same as a hard fail.

Improve two things:
- **Loading feedback**: a clearer loading indicator (progress/activity, e.g. which step — resolving name / fetching record / fetching content / rendering — or at least a live activity spinner + status text) so a slow load reads as "working", not frozen. Use the resolution/fetch steps the engine already goes through (ENS resolve -> IPNS record -> content fetch -> render).
- **Error clarity + retryability**: distinguish a TRANSIENT/timeout failure (retryable) from a HARD failure (unsupported protocol, verification failure, malformed) in the error surface, with an obvious retry affordance for the transient case (a timeout says "timed out, retry?" not just a scary red error). Keep the honest protocol-named reasons.

Apply on desktop and mobile (the loading/trust/error surfaces are cross-platform parity capabilities).

## Acceptance criteria

- [ ] A slow load shows clear ongoing activity/progress (a live indicator + a step/status hint), so it reads as working rather than frozen.
- [ ] A transient/timeout failure is surfaced distinctly from a hard failure, with an obvious retry affordance; hard failures keep their prominent protocol-named reason.
- [ ] The step/status reflects the real resolution/fetch pipeline (name -> record -> content -> render), driven by actual lifecycle events, not faked.
- [ ] Applied on desktop and mobile (or tracked per the parity guard); no re-meaning of the trust posture (loading/error are orthogonal to trust, as established by the prior tasks).
- [ ] Tests cover the loading/progress states and the transient-vs-hard error distinction (fake backend driving slow/timeout/hard-fail), network-isolated.

## Blocked by

- None — can start immediately. (Complements `ipfs-per-resource-car-scope-not-whole-dag` + `fetch-timeout-raise-and-split-for-ipns-and-content`, which reduce how often slow/timeout loads happen; this makes the ones that remain legible.)

## Prompt

> Goal: clearer loading + error feedback. A slow/partial load currently reads as frozen (weak loading feedback), and a transient timeout (retryable) looks like a hard failure. Add a live loading/progress indicator tied to the real pipeline steps (ENS resolve -> IPNS record -> content fetch -> render), and distinguish a transient/timeout failure (with a retry affordance) from a hard failure (unsupported/verification/malformed, keep the protocol-named reason).
>
> Where to look: `crates/werust-core/src/lib.rs` (the load lifecycle, `ChromeState`, `last_error`, the neutral loading badge from `chrome-loading-state-resets-trust-indicator`, the error banner from `prominent-load-failure-and-ipns-resolution-diagnosis`), the desktop chrome `crates/werust/src/main.rs`, and the mobile shells. The resolution steps are in `ens`/`ipns`/`ipfs`; surface their progress. Keep loading/error orthogonal to the trust posture.
>
> Done = a slow load shows real progress, a transient timeout is distinct + retryable, hard failures keep their reason, applied on desktop + mobile (or tracked), proven with a fake-backend slow/timeout/hard test. FIRST re-check the current loading/error surfaces. RECORD any UX decision durably.
