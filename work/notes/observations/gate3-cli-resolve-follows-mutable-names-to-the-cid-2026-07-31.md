---
title: "Gate-3 verdict: cli-resolve-follows-mutable-names-to-the-cid (APPROVE) — one resolution path, and a wire break worth a human nod"
date: 2026-07-31
status: open
reviewOf: cli-resolve-follows-mutable-names-to-the-cid
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main`. A new `werust_core::name_resolution` module (553 lines), `crates/werust-core/src/lib.rs` reworked to consume it, the CLI rewritten onto it, and a spike with decisions.

## Criteria, ticked

1. **`werust resolve <ens-name>` prints the `ipfs://<cid>` the GUI would load, including through an `ipns-ns` pointer, with the record fetched and client-verified first.** MET.
2. **The verification is the SAME core path the GUI uses — one implementation, not a CLI copy.** MET, and met the right way round. The task said lifting the name-to-CID part into a callable core function was the right shape "and the GUI should then use it too"; that is what happened. `navigate_ens_name` now calls the same `name_resolution` entry point the CLI calls, so there is one chain, not two that agree today. This is the criterion most easily faked (copy the logic, call it "shared") and it was not faked.
3. **The mutable-name fact is not lost.** MET in `--json` (which carries `mutable` and `pointer` alongside `cid`) and met-with-a-caveat in the human-readable output: the warning goes to STDERR, so a user who redirects stderr away sees a bare CID with nothing marking it mutable. See below.
4. **An immutable `ipfs-ns` name behaves as today: one CID, no record fetch, no extra network call.** MET, and pinned by a FETCH-COUNTER test rather than by assertion-in-prose. That is the right instinct: "no extra network call" is a claim that rots silently, so counting is the only honest guard.

## The two things I am raising rather than deciding

- **The `--json` object changes shape AND values in one release.** `kind` goes from `ipfs`/`ipns` to the ENSIP-7 `ipfs-ns`/`ipns-ns`; `reference` for a mutable name goes from `ipns://<name>` to `ipfs://<cid>`; `cid`, `mutable` and `pointer` are new. Any external consumer pinned to the v0.2.9 shape breaks. The agent searched `.github/`, `docs/` and `dorfl.json` and found no in-repo consumer, and the `reference` change IS the task, so batching the vocabulary change with it is defensible and arguably kinder than two breaks. But this is a shipped wire format changing under anyone outside this repo, so it goes to the human rather than being ratified by me.
- **The mutability warning is stderr-only.** Decision 1 argues, correctly, that `headless-cli-mode` made stdout the RESULT and stderr the commentary, so a CID on stdout that a script can consume unadorned is the point. The counter-argument is criterion 3's own wording: "the human-readable output makes clear the CID came from a mutable name", and `werust resolve x.eth 2>/dev/null` makes nothing clear. Both readings are honest; the trade is scriptability against an unmissable warning. Human call.

## Review-nit triage (6 raised, all non-blocking)

**Acted on — cut `cli-resolve-help-and-comment-accuracy-and-the-lost-refusal-test`** for three same-class accuracy items: `--help` claims stdout is ALWAYS the bare `ipfs://<cid>` (false under `--json`, and that line exists precisely to tell a script author what to parse); a comment in `navigate_ens_name` claims a failure surfaces the stage it failed at, while `fail_ens_load` clears the step and the existing test asserts `Idle` (a comment repair, explicitly NOT a behaviour change); and the CLI's fail-closed arm lost its direct test when `resolve_output` stopped returning `Result` — the refusal is still pinned in core, so only the thin print-and-exit wrapper is uncovered, but that wrapper is the fail-closed promise at the surface a user touches.

**Ratified:**

- **The new public core surface** (`name_resolution`, `resolve_name`, `resolve_name_with_progress`, `ResolvedName`, `NameResolutionError`), with progress reported through the chrome's existing `LoadStep` rather than a new enum. Coherent with the repo's established language (`version_resolution`, `retrieval`, `wire_name`), and it re-means no glossary term. Every future name-to-content caller inherits these names, which is exactly why getting them boring and consistent was the right move.
- **The headless `resolve` now reads the persisted retrieval-backend setting** and, for a mutable name, makes a gateway HTTP call — so the CLI's output now depends on the settings file where it previously touched only the ENS RPC. Ratified: access is read-only (no shared-write isolation owed), construction makes no network call, and the immutable path still does zero record fetches (the fetch counter proves it). It is also the CORRECT behaviour: a CLI that ignored the user's chosen backend would be a second, quieter disagreement with the GUI, which is the class of bug this whole task removes.
