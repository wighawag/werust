---
title: "Gate-3 verdict: macos-spike-doc-accuracy-and-harness-guard (APPROVE, after a requeue the conductor diagnosed) — the red gate was the TEST, not the guard"
date: 2026-07-31
status: open
reviewOf: macos-spike-doc-accuracy-and-harness-guard
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main` after one requeue. Round 1 went RED on Gate 1 (the acceptance gate, exit 101) on the rebased tip.

## The diagnosis, because a red gate is not automatically the agent's fault

The failing test was the task's OWN new guard test:

```
crates/macos-renderer/tests/typecheck_harness_guard.rs:60:5:
the harness deleted a SCRATCH_DIR outside a temp root: it must refuse instead
```

Read naively, that says the guard does not work and the task failed. It was not that. I reproduced both halves on the exact branch tip `37ed77c`:

- cloned to `/tmp/werust-gate-check` → **FAILS**
- cloned to `~/scratch-gate/werust-gate-check` → **PASSES** (2 passed)

The test built its victim as `repo_root().join("target/…")` and asserted the harness must refuse to delete it, on the assumption that the repository is never under a temp root. dorfl's `freshWorktreeGate` (on by default) runs `prepare`+`verify` in a clean THROWAWAY worktree cut under a temp root — so there, `repo_root()` IS inside a temp root, the guard CORRECTLY allowed the delete, and the test read that correct behaviour as failure. It passed in the build worktree (`~/.dorfl/work/…`) and failed on the rebased tip for that reason alone, and it would fail on any CI runner checking out under a temp path.

So the guard was right and the test's environmental assumption was wrong. That is a fixable-on-retry problem, i.e. a conductor move, not a human question: requeued with the diagnosis, both reproductions, two concrete options, and two explicit do-nots (do not loosen the guard, do not `#[ignore]` it). The agent took option 2 and asserted the CONVERSE when no non-temp location exists, so the test keeps teeth in both environments instead of being skipped.

**The lesson worth keeping:** a test that encodes "where the repo lives" will disagree with itself between the build worktree and the fresh-worktree gate. This is the second time this drive that a gate result had to be diagnosed rather than obeyed.

## The second thing the requeue caught, which no gate would have

Round 1 also made `macos-renderer.yml`'s `pull_request` filter DELIBERATELY IDENTICAL to its `push` filter — adding `crates/renderer/**`, `crates/fetcher/**` and both spike docs paths to the PR trigger. Item 2 only asked that the README and the workflow AGREE, which the docs paths achieve on their own. The extra two crates were a silent widening of exactly the leg whose PR cost the human has explicitly flagged as an open question, in the same week the Windows sibling deliberately went the other way and PINNED its narrowness in a test.

I put that in the requeue note. Round 2 kept the docs paths (with the good argument that re-recording a verdict is the PR that most needs re-measuring), dropped `renderer` and `fetcher` back to push-only, pinned the choice with `the_readme_claim_about_when_the_leg_runs_matches_the_pull_request_trigger`, and left `crates/werust-core/**` exactly as it already was — because that one is the human's question to answer, not the agent's to quietly resolve. That is the right disposition.

## Criteria, ticked

1. **`SCRATCH_DIR` outside a temp root is REFUSED, legibly; the safe default still works.** MET, now provably in both environments.
2. **The README's statement about when the leg runs matches the workflow's actual triggers.** MET, and fixed in the honest direction (make the trigger true rather than weaken the claim), pinned by a test.
3. **The `webview-shared` test description matches reality.** MET the better way: the `LoadLifecycle` tests MOVED to `crates/webview-shared/src/lifecycle.rs`, next to the code they cover. The task preferred moving over rewording, and so did the agent; the point of the shared crate is that its guarantees travel with it.
4. **Item 0 (planted by me at the previous Gate-3): the harness works again after the `desktop-paint` extraction.** MET in substance, weakly evidenced — see nit 2 below.
5. **Item 4 (planted by me at the packaging Gate-3): the Sequoia Gatekeeper instructions.** MET, and pinned by a `release_plumbing_shape.rs` assertion that "Open Anyway" precedes any right-click mention.
6. **Gate green.** MET.

## Review-nit triage (6 raised, all non-blocking)

**Acted on — cut `macos-harness-guard-teeth-and-paint-path-residue`** carrying four of them:

- **The guard test only ever runs ONE branch, and the gate never EXECUTES the repaired harness.** The best nit of the drive. The allow-path assertion is reachable only from the no-candidate fallback, so on any ordinary host (the gate included) the harness's repaired body is never run — only string-matched. That is precisely the blind spot that let item 0 sit unnoticed in the first place, reproduced one level up. Fixing it is cheap and makes criterion 4's evidence real.
- **Criterion 4 is backed by PROSE.** I asked for "proven by running it, not by reading it"; what landed is a sentence saying it ran clean. Not a fabrication, and I believe it, but it is not enforced. The fix above converts it into something the gate checks on every run, which is better than a committed transcript and needs no Mac.
- **The `$HOME` probe directory has a FIXED name** while its sibling helper suffixes the pid, so two concurrent runs sharing a `HOME` delete each other's victim and produce a failure that looks like the guard breaking. Small, real, and exactly the kind of flake that costs an hour later.
- **Docs still name the deleted `crates/werust-macos/src/paint.rs`** in four places, one of them in a file this very task edited. Same defect class the task exists to fix.

**Ratified, no action:**

- **The item-2 filter resolution** (above). The human's actual question — should `crates/werust-core/**` gate PRs on `macos-14` minutes at all? — is correctly left OPEN rather than answered by a side-effect.
- **A new spike directory holding only a `DECISIONS.md` with no README**, and the new README-ORDER constraints in `release_plumbing_shape.rs` (a future README edit must keep "Open Anyway" before any right-click mention). Both are small precedents rather than problems; noted here so they are findable if either becomes annoying.

## An off-path finding the build filed correctly

`macos-typecheck-stand-in-core-drifts-unwatched-2026-07-31.md` — the harness's stand-in core can drift from the real one with nothing watching. Filed, not fixed, correctly.
