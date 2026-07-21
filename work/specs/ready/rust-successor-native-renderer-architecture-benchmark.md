---
title: "werust: decide the native-renderer architecture by a capability + trust-hook benchmark"
slug: rust-successor-native-renderer-architecture-benchmark
needsAnswers: true
taskedAfter: [rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack]
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/ready/` tasks. (The technical-detail sections below are trimmed by `to-task` once the work is tasked — they move into tasks/ADRs and this spec settles to its durable framing: Problem / Solution / User Stories / Out of Scope.)

<!-- open-questions -->

## Open questions

These are the deliberately-deferred decisions this exploration exists to resolve.
They BLOCK autonomous tasking (`needsAnswers: true`) until a human resolves them —
because each is a decision with a real trade-off whose *why* must come from a
human, and each is best decided on the EVIDENCE this exploration produces, not
guessed now.

1. **Native-renderer architecture (the central question).** Is werust's native
   renderer (a) our own from-scratch Rust engine, (b) reused **Servo** behind the
   `Renderer` seam, or (c) a **Blitz/Stylo-component assembly**? To be decided
   EMPIRICALLY by the capability + trust-hook benchmark (does the candidate reach
   the tier's page checklist AND satisfy provider-injection + `ipfs://`-scheme?),
   NOT by preference now. → resolved into an ADR when the benchmark evidence and
   the human's why are in.
2. **TLS trust-store / pinning policy**, and whether content-addressed fetches
   **relax origin trust** because verification moves to the hash. → resolved into
   an ADR (a trust-boundary decision; security-sensitive).
3. **The wallet broker security model** — an own-process signing broker where the
   page never holds keys. What process boundary, what approval UX, what the page
   is allowed to see. → resolved into an ADR (security-critical).
4. **The user-facing product / display name.** Deliberately undecided; the code
   slug `werust` is used for the work/ identity so a rename never churns
   cross-references. → resolved when chosen (not architecture-blocking; recorded
   here so it is not forgotten).

<!-- /open-questions -->

## Problem Statement

werust's native renderer must eventually be one concrete architecture, and that
choice is **hard to reverse, surprising without context, and a real trade-off** —
exactly an ADR-worthy decision. The idea deliberately does NOT force it now: it is
to be decided **empirically, at native-renderer spec time, by a capability +
trust-hook benchmark**, using the evidence produced by climbing to T1 on an
assembled pure-Rust stack (the committed build spec,
`rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack`).

This is therefore an **EXPLORATION spec** (its "done" = CONFIDENCE + a de-risked,
sliced build plan for the chosen architecture — NOT shipped capability). Writing
build tasks now would be fiction: the load-bearing choice (own engine vs Servo vs
Blitz/Stylo assembly) is unpicked, and it must be picked on evidence.

Alongside the central architecture question, three further deferred decisions are
carried here so they are surfaced and resolved deliberately rather than drifting:
the TLS trust-store / pinning policy (and content-addressed trust relaxation), the
wallet broker security model, and the (name-independent) display name.

## Solution

Run the benchmark harness (built as story 21 of the committed build spec) over the
candidate architectures, evaluate each against BOTH rendering capability (the
pinned conformance-ladder page checklists) AND the trust hooks (provider injection
+ `ipfs://` scheme + hash-verified fetch), gather the human's *why* on the four
open questions, and EMIT:

- an **ADR** picking the native-renderer architecture, with the rejected
  alternatives and the benchmark evidence that decided it;
- ADRs for the TLS trust-store / pinning policy and the wallet broker security
  model;
- a **de-risked, sliced build plan** (a follow-on build spec) for the chosen
  architecture — the exploration's real deliverable.

## User Stories

> Confidence-scoped (exploration) stories — the deliverable is CONFIDENCE + a
> build plan, not shipped capability. Not build-taskable until the open questions
> are resolved; hence `needsAnswers: true`.

1. As a maintainer, I want each candidate architecture (own from-scratch Rust
   engine, reused Servo behind the seam, Blitz/Stylo-component assembly) run
   through the capability + trust-hook benchmark on the SAME pinned page
   checklists, so that they are compared on evidence, not preference.
2. As a maintainer, I want each candidate scored explicitly on the **trust hooks**
   (can it inject the EIP-1193 provider AND serve `ipfs://` via the seam's
   custom-scheme hook with hash verification?), so that a renderer that renders
   well but cannot satisfy the thesis is correctly disqualified.
3. As a maintainer, I want the T1 climb evidence from the committed build spec
   (effort, code volume, DOM object-graph friction vs wezig's Zig arm) folded into
   the decision, so that "does Rust give a simpler/faster path?" is answered with
   real data.
4. As a maintainer, I want to resolve open question 2 (TLS trust-store / pinning;
   content-addressed trust relaxation) and emit an ADR, so that the fetch/trust
   boundary is a recorded decision before native networking hardens.
5. As a maintainer, I want to resolve open question 3 (wallet broker security
   model — own-process signing broker, page never holds keys) and emit an ADR, so
   that key custody is designed deliberately, not defaulted.
6. As a maintainer, I want to resolve open question 4 (the display name) or
   confirm it stays deferred, so that the name-independent code slug remains the
   stable identity until then.
7. As a maintainer, I want the exploration to END by emitting an ADR for the
   chosen architecture AND a follow-on BUILD spec (a de-risked, sliced plan for
   building the native renderer on that architecture), so that the exploration's
   definition of done — confidence + a build plan — is met.

### Autonomy notes (the two gate axes)

- **`humanOnly`:** effectively enforced via `needsAnswers` — an agent may not
  auto-task this until the open questions (which require human *why*s, several
  security-critical) are resolved and the flag cleared.
- **`needsAnswers: true`:** set. The four open questions above BLOCK tasking; they
  are resolved by a human (informed by the benchmark evidence) and the flag
  cleared, at which point the exploration's confidence-scoped stories become
  taskable.

## Implementation Decisions

> Trimmed at tasking-time.

- **This is an exploration, not a build.** The spikes are prototypes scoped to one
  question each (the ANSWER is the deliverable, not the code). The benchmark
  harness itself is built by the committed build spec (story 21); this spec RUNS
  it and DECIDES.
- **The decision procedure is capability AND trust-hooks** — a candidate that
  cannot satisfy provider-injection + `ipfs://`-scheme is disqualified regardless
  of rendering quality (the `Renderer` seam's qualifying rule, per `docs/adr/0001`).
- **Outputs are ADRs + a follow-on build spec** — never a silent choice. The
  architecture, TLS/trust, and wallet-broker decisions each become an ADR with a
  human-supplied why.

## Testing Decisions

> Also trimmed at tasking-time.

- The benchmark is scored against the SAME pinned conformance-ladder page
  checklists and WPT subsets used by the build spec, so capability scores are
  comparable across candidates and against wezig.
- Trust-hook satisfaction is tested as behaviour through the `Renderer` seam
  (provider injected + reachable; `ipfs://` resolved + hash-verified) — a
  pass/fail qualifying gate, not a graded score.

## Out of Scope

- **Actually BUILDING the native renderer on the chosen architecture** — that is
  the follow-on build spec this exploration emits (story 7), ordered after this
  one resolves.
- **T2/T3 capability** — beyond the T1-anchored benchmark; the chosen
  architecture's build spec carries the higher tiers.

## Further Notes

- This spec exists so the deferred decisions are **surfaced and resolved
  deliberately** (each into an ADR whose why comes from a human), not
  force-resolved at idea time nor left to drift. It is the honest home for the
  "decided empirically at native-renderer spec time" deferral in the original
  idea.
