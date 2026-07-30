---
title: "Gate-3 conductor review: windows-ipfs-origin-probe-on-ci (APPROVE — the verdict reproduces INSIDE werust)"
date: 2026-07-30
status: open
reviewOf: windows-ipfs-origin-probe-on-ci
verdict: approve
---

## Verdict: APPROVE

Merged as `8a3afe4` on `origin/main`. Gate-1 and Gate-2 green, 4 non-blocking nits. Local full gate green; CI `verify` green (run 30552988185).

## The thing I checked that mattered most

Gate 2's sharpest nit was that the whole `#[cfg(windows)]` half had never been built or run INSIDE werust: the measurement happened in a throwaway scratch repo, and the Ubuntu gate compiles only the host-independent half. Its own words: "if that first in-repo run is red, the recorded verdict must be re-decided before `windows-webview2-backend-and-window` claims it."

**It ran, and it is green.** The merge to `main` tripped the workflow's path filter, and run **30552987642** (`windows-origin-probe`, 1m26s) reproduced the verdict in this repo, on WebView2 Runtime 150.0.4078.65, asserting against the committed `expected.json`. So the recorded verdict is not a claim imported from a repo we can no longer see; it is a result this repository regenerates on demand.

## The verdict

**`registered-ipfs-scheme`.** On WebView2 Runtime 150.0.4078.65, an `ipfs://` scheme registered with `HasAuthorityComponent = TRUE` + `TreatAsSecure = TRUE` gives the document the real tuple origin `ipfs://<cid>`, a secure context, a same-origin `fetch` that resolves AND fires `WebResourceRequested`, and a working `pushState`. A SvelteKit-shaped client navigation works. **`origin_map.rs` is NOT promoted; Android stays the only mapped platform.**

## Acceptance criteria, ticked

- [x] **Runs on `windows-latest`, no hardware, no network.** Canned bytes only.
- [x] **Both cases exercised, with origin, fetch result INCLUDING whether the handler fired, and `pushState` asserted.** Case B (internal https) also works, and is simply not needed.
- [x] **Runtime version recorded** with the result, in both the report and `expected.json`.
- [x] **A recorded VERDICT the shell task can build to**, plus ADR-0011 Amendment 2 closing the ADR's one open question.
- [x] **Re-runnable:** `workflow_dispatch` plus a path-filtered push, which is what self-verified the merge.
- [x] **No core, no IPFS, no shell.** A new `crates/windows-origin-probe` workspace crate, target-gated exactly like the mobile crates, with 23 host-independent tests running in the ordinary Ubuntu gate.

## What the agent did BETTER than the task asked

**It added a negative control**, and that is the difference between evidence and a tautology. The same URL and bytes with `HasAuthorityComponent` flipped off reproduced the Android failure verbatim, Blink's own sentence included ("URL scheme \"ipfs\" is not supported"), with the handler never firing and `pushState` throwing `SecurityError`. A probe where everything passes has measured nothing; this one can fail, and demonstrably does when the mechanism is wrong. The control is asserted on every re-run.

It also proved BOTH directions of the regression guard (green against the recorded baseline, red and naming the moved field when the baseline was tampered with), and it happened to answer two things nobody asked: ES module import works from the custom scheme (SvelteKit needs it), and the CSS `url()` subresource handler DOES fire on this runtime, which is the corner of WebView2Feedback #4362.

One incidental refinement: `service_worker` is rejected in BOTH cases here (`InvalidStateError` on A, `TypeError` on B, the latter because `.invalid` cannot resolve the script fetch). That does not contradict the earlier service-worker observation, whose claim was about Android's real internal-https origin, but it does mean the synthetic probe is not the place that question gets settled.

## Nit triage (4 non-blocking findings)

**Needs YOU (I cannot): delete the leftover scratch repo.** `wighawag/werust-windows-origin-probe-scratch` was created to get a Windows runner, because a worker may not push to `werust` (the runner owns git state). It is private, archived, with Actions and issues disabled, and nothing depends on it. My token has `repo` + `workflow` but not `delete_repo`, so deletion is yours. The route itself (an external side effect on your account that the task never authorised) is disclosed only in an observation, not in the spike DECISIONS block; ratify or object.

**The general signal underneath it is worth a decision, not just a note:** a task whose deliverable is a CI MEASUREMENT fits the work/ contract awkwardly, because the worker cannot reach CI on the repo it is working in. The detour worked but left litter. Worth settling once, properly, rather than re-improvised per task.

**Tasked by me:** nothing in the Ubuntu gate asserts the two COMMITTED evidence files against each other, so `expected.json` and `probe-report-2026-07-30.json` can silently drift in a later edit and only a Windows runner would notice. A single host-independent test closes it, in the spirit of the 23 already shipped. Filed as `windows-probe-evidence-files-agree-test`.

**Ratify (I would):** `expected.json` pins case B and the control as HARD assertions, so a change in the fallback mechanism werust does not use, or in the control's exact strings, reddens the workflow. That is deliberate falsification-guard brittleness at a cost of 1m26s on an on-demand workflow. Cheap insurance, but it was not recorded as a decision.

## What this unblocks

`windows-webview2-backend-and-window` loses its first `blockedBy`. It may now build to the registered-scheme mechanism as settled fact, and must not re-litigate it.
