---
title: "Load failures (esp. IPNS like ronan.eth) must be PROMINENTLY visible, and diagnose why ronan.eth's IPNS resolution fails"
slug: prominent-load-failure-and-ipns-resolution-diagnosis
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [2]
---

## What to build

Two coupled problems the human hit loading `ronan.eth` (an ipns-ns name): (a) the IPNS resolution FAILED, and (b) the failure "was not easily seen" in the chrome. Fail-closed is only honest if the user can actually SEE the reason.

### Part 1 — make a failed load prominently visible (the honesty fix)
A fail-closed load stores its reason in `ChromeState::last_error`, but it is surfaced too weakly (a subtle status line the human missed). A load that fails (unresolvable ENS/IPNS name, unsupported/absent contenthash, retrieval/verification failure) must show its reason PROMINENTLY — an in-view error state / error page, not a barely-visible status line. The whole point of fail-closed is that the user understands why nothing rendered. Apply on desktop AND mobile (a cross-platform capability), consistent with the trust-honesty stance.

### Part 2 — diagnose why ronan.eth's IPNS resolution fails
`ronan.eth`'s contenthash is ipns-ns; resolving it failed on the real build. Determine WHY and fix if it is a real bug: capture the actual `IpnsError` (unresolvable name / record fetch failure / malformed record / signature-or-validity failure / unsupported-or-invalid target). Candidates: the `/ipns/{name}?format=ipns-record` request against the default endpoint not returning a verifiable record, an endpoint that does not serve IPNS records, a record-decode/verify mismatch, or a target the resolver rejects. Record the real failure and either fix the resolution path or, if it is a genuine environmental limitation (e.g. the default gateway does not serve IPNS records), surface that as a clear, correct reason AND note the fix direction (e.g. a delegated IPNS endpoint).

## Acceptance criteria

- [ ] A failed load (ENS/IPNS resolution failure, unsupported/absent contenthash, retrieval/verification failure) shows its reason PROMINENTLY in the chrome/view (an error state the user cannot miss), on desktop and mobile — not only a subtle status line.
- [ ] The specific `ronan.eth` IPNS failure is diagnosed: the actual error is captured, and either the resolution is fixed so a resolvable IPNS name renders, or the real limitation is identified with a correct user-facing reason and a recorded fix direction.
- [ ] The error text is accurate and protocol-named (mirrors the existing decoder/resolution error taxonomy); a mutable-name/IPNS failure reads clearly (not a generic "failed").
- [ ] Tests cover the prominent-failure surfacing (a fake backend failing a load asserts the visible error state) and the IPNS failure taxonomy mapping to a clear message, network-isolated.

## Blocked by

- None — can start immediately. (Builds on `ipns-name-resolution-and-render` in `tasks/done/`.)

## Prompt

> Goal: (1) make a fail-closed load's reason PROMINENTLY visible (the human loaded `ronan.eth`, it failed, and the error "was not easily seen" — fail-closed is only honest if the user sees why), and (2) diagnose why `ronan.eth`'s IPNS resolution fails and fix it or surface the real reason.
>
> Where to look: `crates/werust-core/src/lib.rs` (`ChromeState::last_error`, `navigate`/`navigate_ens_name`, the load lifecycle), the desktop chrome `crates/werust/src/main.rs` (how `last_error` is shown — too weakly today) and the mobile shells. IPNS resolution is `crates/werust-core/src/ipns.rs` (`resolve_ipns_name`, `IpnsError`, the `/ipns/{name}?format=ipns-record` source); the ENS front door dispatches ipns-ns to it. `ronan.eth` is a real name whose contenthash is ipns-ns — reproduce its failure and capture the actual `IpnsError`.
>
> Done = a failed load shows a prominent, accurate, protocol-named reason on desktop + mobile; the ronan.eth IPNS failure is diagnosed and fixed (or its real cause surfaced with a fix direction); proven with tests for both the visible-failure surfacing and the error taxonomy. FIRST re-check the current error-surfacing + the ipns path. RECORD the diagnosis (the real ronan.eth error + cause) as a finding, and any UX decision durably.
