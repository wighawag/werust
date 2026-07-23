---
title: "Gate-3 conductor review: eth-name-with-path-routes-to-ens-and-subpath (APPROVE)"
date: 2026-07-23
status: approved
reviewOf: eth-name-with-path-routes-to-ens-and-subpath
gate: gate-3-conductor
mergedCommit: 52e235d
---

## Verdict: APPROVE

Conductor Gate-3 pass. Gate-1 + Gate-2 passed before merge (landed on the second dispatch after an intermittent integration-git ENOENT recovery, see below). Driven in place from backlog. Re-ran the .eth/path tests locally.

## Done-move + landing

- `work/tasks/backlog/eth-name-with-path-routes-to-ens-and-subpath.md` -> `done/` on origin/main (squash merge `52e235d`).
- Files: `crates/werust-core/src/lib.rs` (+433: `eth_name_and_path_from_entry`, path-threaded `navigate_ens_name`, subpath load), gate-2 nits note.

## Acceptance criteria (ticked, re-verified locally)

- [x] `ronan.eth/blog/` (and `ronan.eth/blog`) is detected as the ENS front door for `ronan.eth` and loads `ipfs://<cid>/blog/`, NOT `https://ronan.eth/blog/`. `navigate` checks `eth_name_and_path_from_entry` FIRST (before `classify_entry`), splitting at the first `/`; `navigate_ens_name(name, path)` threads the path into the resolved load (`format!("{uri}{path}")`).
- [x] The URL bar keeps `ronan.eth/blog/` with the ENS posture (the name+path pinned), not the raw `ipfs://<cid>/blog/`.
- [x] Bare `.eth` unchanged (empty path); explicit scheme still literal (the `://` guard); a non-`.eth` host+path (`github.com/foo`) still routes to `https://` (it fails the `.eth` name recogniser, falls to `classify_entry -> HttpsCandidate`) - NOT ENS. Correct ordering verified in the diff.
- [x] A `.eth/<path>` whose path resolves to no entity fails closed with the existing legible reason keeping the typed `.eth/<path>` (the ipfs PathNotFound-class path is unchanged).
- [x] Reload/back/forward of a `.eth/<path>` re-derive via `eth_name_and_path_from_entry` (reload's ENS-name lookup now splits name+path) + `ens_pages`/`pinned_root_key`.
- [x] Shared-core routing (desktop + mobile drive the same front door).
- [x] Tests cover the split + subpath cases (`eth_name_and_path_splits_a_dot_eth_entry_into_name_and_optional_path` + subpath navigation), network-isolated. Green locally.

## Design coherence

`eth_name_and_path_from_entry` delegates the NAME validation to the existing `eth_name_from_entry` (the `.eth` TLD + non-empty-label + no-`/` guard lives in ONE place; the no-path front door keeps its exact rule). Clean, no duplication.

## Review-nits triage (Gate-2) - two genuine under-specified behaviours flagged for the human

1. QUERY/FRAGMENT folding: a `.eth` entry folds EVERYTHING from the first `/` (including `?query`/`#fragment`) into the ipfs sub-path, so `ronan.eth/blog?x=1#frag` loads `ipfs://<cid>/blog?x=1#frag`. Reasonable Phase-1 posture (the ipfs path resolution + the webview handle a query/fragment), but not covered by a test/criterion. FLAGGED: does the human want query/fragment passed through into the ipfs sub-path as-is, or stripped/handled? Non-blocking (the path case - the actual finding - works).
2. `ronan.eth?x=1` (a `.eth` label but NO `/`, with a query) fails the `.eth` suffix check (name == `ronan.eth?x=1`) and routes to `https://` instead of ENS. Acceptable for Phase-1 (bare `.eth` + path is the scope), but a gap if a query-on-bare-name is wanted. FLAGGED, non-blocking.
3. No `## Decisions` block in the PR body; the split-at-first-slash choice is captured here instead. RATIFIED.

Neither (1) nor (2) blocks - both are edge refinements on top of the delivered `.eth/<path>` -> ENS+subpath behaviour, worth one human ratification on query/fragment posture.

## INFRA note (intermittent, recovered)

First dispatch: Gate-1 + Gate-2 passed, then `spawnSync git ENOENT` at the INTEGRATION/merge git ops (integrator.js `run('git', ...)` calls). The built work was intact in the fresh-gate worktree commit `cad2c9c` (no branch pushed, so `requeue` released the lock; the deterministic build re-ran green on the second dispatch and merged). The ENOENT is INTERMITTENT (not deterministic): the dorfl node process's spawn env occasionally loses `/usr/bin` on a deep git spawn (Gate-2 or integration leg), even with `/usr/local/bin/pi` + system node resolving correctly in the parent shell. Recovery = confirm no branch pushed -> prune fresh-gate worktree -> gc -> requeue --arbiter origin -> re-dispatch. Recorded so the pattern (retry lands it; work is never lost) is known.

## Net effect

`ronan.eth/blog/` now routes to ENS and loads the sub-path, keeping name+path in the bar - fixing the v0.2.4 finding B (`https://ronan.eth/blog/` DNS failure). One human ratification pending on query/fragment posture.
