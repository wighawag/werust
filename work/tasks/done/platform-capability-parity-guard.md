---
title: "Platform-capability parity guard: no feature silently lands on one context only (verify-enforced matrix + no-silent-no-op-seam rule)"
slug: platform-capability-parity-guard
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: []
---

## Settled decisions (from the design discussion — DECIDED, build to them)

1. **Build BOTH complementary mechanisms.** (a) A checked-in **capability matrix** (one row per cross-cutting user-facing capability; columns desktop / iOS / Android; each cell `implemented` / `stubbed`+REQUIRED-linked-follow-on-task-slug / `n-a`+reason), and (b) a **no-silent-no-op-seam rule**: an empty/no-op seam-trait impl on any backend must be explicitly marked AND task-linked. (a) catches capability-level gaps; (b) catches method-level gaps.
2. **Enforcement = a plain workspace `cargo test`** so it rides the existing `verify` gate (`fmt && clippy && build && test`) with NO CI change. The guard test FAILS when any capability cell is `stubbed`/unknown without a resolvable linked task, or when an unmarked no-op seam impl exists.
3. **Seed the matrix with current real state:** address bar, back/forward, ENS `.eth` resolution, EIP-1193 provider injection, trust indicator = `implemented` on all three; `ipfs://` render = desktop `implemented`, iOS/Android `stubbed` -> linked to `mobile-ipfs-scheme-interception-ios-and-android`. (Confirm/extend the row list against the code when building.) So the guard passes today ONLY because the known gap is tracked, not hidden.

## What to build

A durable mechanism guaranteeing that every cross-cutting feature is EITHER implemented on all shipped contexts (desktop, iOS, Android) OR has a tracked task (an exploration task if needed) covering its completion — so a capability can never again silently ship on one platform only and be forgotten. This exists because the `ipfs://`-render gap was invisible: a seam method (`register_scheme_handler`) was silently no-op'd on the mobile backend, a whole capability shipped desktop-only, the release looked green, and nothing flagged it.

Build the guard settled above — a `verify`-enforced capability matrix AND a no-silent-no-op-seam rule — and SEED it with the current true state so the existing known gaps (mobile `ipfs://` render) are recorded as `stubbed` with their follow-on task linked, turning "we forgot" into a gate failure. The guard must be cheap to keep honest: adding a new capability or a new platform forces a matrix row/column, and a bare unimplemented stub with no linked task reds the gate.

## Acceptance criteria

- [ ] A checked-in capability matrix (and/or seam-no-op rule) exists, listing each cross-cutting capability x {desktop, iOS, Android} with an explicit `implemented` / `stubbed`(+linked task) / `n-a`(+reason) state.
- [ ] A `verify`-enforced guard FAILS the gate when a capability is `stubbed`/unimplemented on a platform WITHOUT a resolvable linked follow-on task, or when a capability/platform cell is missing.
- [ ] The matrix is SEEDED with current reality, including the known gap (mobile `ipfs://` render = `stubbed`, linked to `mobile-ipfs-scheme-interception-ios-and-android`), so the guard passes today only because the gap is tracked, not because it is hidden.
- [ ] (If the no-op-seam rule is chosen) an empty/no-op seam impl on any backend must be explicitly marked and task-linked; an unmarked silent no-op reds the gate. The current mobile `register_scheme_handler` no-op is either made real (by its own task) or explicitly marked+linked.
- [ ] The guard runs inside the existing gate (a plain workspace test if possible) so a tag can never ship a silently-one-platform feature.
- [ ] Tests cover the guard itself (a fixture matrix with an untracked stub FAILS; a fully-implemented-or-tracked matrix PASSES), network-isolated.

## Blocked by

- None to START, the design forks are settled above. Do not autonomously build until the flag is cleared.

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
