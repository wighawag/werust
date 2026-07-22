---
title: "Platform-capability parity guard: no feature silently lands on one context only (verify-enforced matrix + no-silent-no-op-seam rule)"
slug: platform-capability-parity-guard
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
needsAnswers: true
blockedBy: []
covers: []
---

<!-- open-questions -->

## Open questions

1. **Guard shape + strictness.** Which mechanism (one, or both)?
   - A checked-in **capability matrix** (one row per cross-cutting user-facing capability, columns = desktop / iOS / Android, each cell one of `implemented` / `stubbed` (with a REQUIRED linked follow-on task slug) / `n-a` (with a reason)), enforced by a `verify`-time test that FAILS the gate if any cell is `stubbed` without a resolvable linked task, or is missing/unknown.
   - An **ADR + lint rule** establishing "a seam/trait method may not be silently no-op'd on any backend": an empty stub impl must be explicitly marked (a known marker/attribute) AND linked to a tracked task, and a guard test greps for unmarked empty seam impls and reds the gate.
   DECIDE which (the matrix catches capability-level gaps; the no-op rule catches method-level gaps; they are complementary). Confirm before building.
2. **Capability granularity + initial rows.** What counts as a "cross-cutting capability" (address bar, back/forward, `ipfs://` render, ENS `.eth` resolution, EIP-1193 provider injection, trust indicator, ...)? Seed the matrix with the current real state (e.g. `ipfs://` render = desktop `implemented`, iOS/Android `stubbed` -> linked to `mobile-ipfs-scheme-interception-ios-and-android`). Confirm the seed list.
3. **Where enforcement runs.** Is the guard a normal `cargo test` (so it runs inside the existing `verify` = `fmt && clippy && build && test` gate automatically), or a separate script wired into `verify.yml`/`.goreleaser`? Prefer a plain test in the workspace so it rides the existing gate with no CI change; confirm.

<!-- /open-questions -->

## What to build

A durable mechanism guaranteeing that every cross-cutting feature is EITHER implemented on all shipped contexts (desktop, iOS, Android) OR has a tracked task (an exploration task if needed) covering its completion — so a capability can never again silently ship on one platform only and be forgotten. This exists because the `ipfs://`-render gap was invisible: a seam method (`register_scheme_handler`) was silently no-op'd on the mobile backend, a whole capability shipped desktop-only, the release looked green, and nothing flagged it.

Build the guard agreed in the open questions — a `verify`-enforced capability matrix and/or a no-silent-no-op-seam rule — and SEED it with the current true state so the existing known gaps (mobile `ipfs://` render) are recorded as `stubbed` with their follow-on task linked, turning "we forgot" into a gate failure. The guard must be cheap to keep honest: adding a new capability or a new platform forces a matrix row/column, and a bare unimplemented stub with no linked task reds the gate.

## Acceptance criteria

- [ ] A checked-in capability matrix (and/or seam-no-op rule) exists, listing each cross-cutting capability x {desktop, iOS, Android} with an explicit `implemented` / `stubbed`(+linked task) / `n-a`(+reason) state.
- [ ] A `verify`-enforced guard FAILS the gate when a capability is `stubbed`/unimplemented on a platform WITHOUT a resolvable linked follow-on task, or when a capability/platform cell is missing.
- [ ] The matrix is SEEDED with current reality, including the known gap (mobile `ipfs://` render = `stubbed`, linked to `mobile-ipfs-scheme-interception-ios-and-android`), so the guard passes today only because the gap is tracked, not because it is hidden.
- [ ] (If the no-op-seam rule is chosen) an empty/no-op seam impl on any backend must be explicitly marked and task-linked; an unmarked silent no-op reds the gate. The current mobile `register_scheme_handler` no-op is either made real (by its own task) or explicitly marked+linked.
- [ ] The guard runs inside the existing gate (a plain workspace test if possible) so a tag can never ship a silently-one-platform feature.
- [ ] Tests cover the guard itself (a fixture matrix with an untracked stub FAILS; a fully-implemented-or-tracked matrix PASSES), network-isolated.

## Blocked by

- None to START, but `needsAnswers: true` — the guard shape/strictness, capability granularity, and enforcement location must be settled first. Do not autonomously build until the flag is cleared.

## Prompt

> Goal: make "a feature silently shipped on one platform only" impossible to release unnoticed. Build a `verify`-enforced platform-capability parity guard (a checked-in capability matrix x {desktop, iOS, Android}, and/or a no-silent-no-op-seam rule) that reds the gate when a capability is unimplemented on a platform without a linked tracked task. This is the durable fix for the class of bug that hid the mobile `ipfs://` gap: a seam method no-op'd on one backend, a whole capability desktop-only, a green release, and nothing flagged it.
>
> Domain vocabulary: werust ships three contexts (desktop WebKitGTK, iOS WKWebView, Android System WebView), each an OS edge over the shared `werust-core` seams. A capability is a cross-cutting user-facing behaviour (address bar, back/forward, `ipfs://` render, ENS `.eth` resolution, EIP-1193 provider, trust indicator). The `verify` gate is `fmt && clippy && build && test`; a tag push cuts the release, so anything the gate misses ships.
>
> Where to look: the mobile no-op that motivated this is `register_scheme_handler` in `crates/werust-android`'s Rust backend (an empty `{}`), vs the real desktop `install_ipfs`. The `work/` contract already tracks tasks by slug in `work/tasks/{backlog,ready,done}/` — the guard's "linked follow-on task" should reference those slugs so a `stubbed` cell resolves to a real tracked task. Seed the matrix from current reality: `ipfs://` render is desktop-`implemented`, iOS/Android-`stubbed` linked to `mobile-ipfs-scheme-interception-ios-and-android`.
>
> Keep it cheap and self-enforcing: prefer a plain workspace `cargo test` so it rides the existing `verify` gate with no CI change; adding a capability or platform must force a matrix cell, and a bare untracked stub must red the gate.
>
> Done = the guard exists, is seeded with real state (known gaps tracked, not hidden), reds the gate on an untracked cross-platform gap, runs inside `verify`, and is itself tested. FIRST re-check current reality (the mobile no-op, the shipped platforms, the `work/` task layout) and route to needs-attention on drift. RECORD the guard's design (matrix vs no-op-rule, granularity, enforcement point) durably — it likely meets the ADR gate (a standing process rule with a real trade-off), so prefer an ADR in `docs/adr/`.
