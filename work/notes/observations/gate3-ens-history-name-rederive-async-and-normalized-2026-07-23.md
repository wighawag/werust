---
title: "Gate-3 conductor review: ens-history-name-rederive-async-and-normalized (APPROVE)"
date: 2026-07-23
status: approved
reviewOf: ens-history-name-rederive-async-and-normalized
gate: gate-3-conductor
mergedCommit: 96871f4
---

## Verdict: APPROVE

Conductor Gate-3 pass. This task FIXES a regression my own earlier Gate-3 tick missed (I APPROVED `preserve-ens-name-in-bar-on-reload-and-history`'s back/forward criterion against the synchronous FakeBackend, which the real backend does not match), so I scrutinised the test-harness upgrade especially, and re-ran the tests locally. Gate-1 + Gate-2 passed before merge. Driven in place from backlog via `dorfl do ... --allow-backlog --isolated --review --merge`.

## Done-move + landing

- `work/tasks/backlog/ens-history-name-rederive-async-and-normalized.md` -> `done/` on origin/main (squash merge `96871f4`).
- Files: new `crates/werust-core/src/ipfs.rs::normalize_ens_page_key` (+94, with 4 unit tests), `crates/werust-core/src/lib.rs` (+160: normalized keying at all sites + the FakeBackend async-history upgrade), gate-2 nits note.

## Acceptance criteria (ticked against the diff + a local re-run)

- [x] Back/forward onto an ENS page show the `.eth` name + posture on the REAL (async) backend, never `ipfs://`/`ipfs:///`. Both root causes fixed.
- [x] Robust to async history: the re-derive runs in `refresh_chrome` (called by `pump`) and re-applies once `current_url` settles onto the ENS CID, not only synchronously in `go_back`/`go_forward`.
- [x] `ens_pages` keyed on a NORMALIZED CID (`normalize_ens_page_key`) applied IDENTICALLY at insert (`load_resolved_content`, lib.rs:714) and every lookup (`refresh_chrome` :800, `reload` :926). `normalize_ens_page_key` collapses `ipfs://<cid>` and the WebKit authority-less `ipfs:///<cid>` (the triple-slash tell) to one bare-`<cid>[/path]` key, trims a bare trailing slash, preserves sub-paths, leaves non-ipfs URLs untouched. The `ipfs:///` leak is gone.
- [x] A plain (non-ENS) entry still shows its real URL; the name never leaks onto a genuinely non-ENS page (see collision note below for the one edge).
- [x] **The test harness was genuinely upgraded** (the crucial ask). The FakeBackend now models the real async lag: `pending_history` + a `reported_url` distinct from `history[cursor]`, so right after `go_back()`/`go_forward()` `current_url()` still names the PREVIOUS entry until `settle_pending_history()` (called from `drive_to_finished`/`drive_to_failed`) lands it, exactly like WebKitGTK's `load-changed` settling `current_url` on the GTK loop. History also stores the WebKit-NORMALIZED (`ipfs:///`) form via `webkit_normalize`, so a raw-string lookup WOULD miss. The back/forward test now drives `go_back()` then a settling pump. I re-ran `back_and_forward_onto_an_ens_page_show_the_name_and_posture_not_the_cid` and the 4 `normalize_ens_page_key_*` tests locally: all green. The regression class can no longer pass on a synchronous identical-string fake.
- [x] Applied on desktop + mobile (fix is in the shared `BrowserShell` core all shells drive).

## Review-nits triage (Gate-2) + the decisions I'm capturing here

The 3 nits converge on two undocumented key-policy choices (no `## Decisions` block was in the PR body). I capture them here (Gate-3's job) and flag the widening for the human:

1. **Direct-CID collision, now WIDENED (flag for human).** `normalize_ens_page_key` reduces to a bare `<cid>` (scheme + authority dropped). If a user navigates DIRECTLY to `ipfs://<cid>` for a CID previously ENS-resolved, it normalizes to the same key and would surface the prior `.eth` name from `ens_pages`. This collision PRE-EXISTED (I flagged it on the `preserve-ens-name...` Gate-3 note); the normalized key WIDENS it (now `ipfs:///` variants collide too). `ens_pages` is still insert-only via the ENS front door, and it is not a verification downgrade (same hash-verified bytes; that CID genuinely is what the name resolved to). ACCEPTED as-is for this fix (the fix's job is the back/forward leak, and the widening is inherent to a correct normalized key), but the collision is now on the human's docket as a single item across two tasks: decide whether a DIRECT raw-CID navigation should suppress the ens_pages decoration (scope the decoration to name-originated navigations + add a collision test).
2. **Trailing-slash / sub-path key policy (deliberate, recorded here).** The key treats `ipfs://cid` == `ipfs://cid/` (bare trailing slash trimmed) and preserves deeper sub-paths as identity. This is a small user-visible key-equivalence policy the task under-specified ("ideally reduce to canonical <cid>[/path]"); low risk, and the unit tests pin it. Recorded here as the deliberate choice so the next author does not re-derive it.
3. The absent `## Decisions` block: captured by this Gate-3 note instead, per the conductor-review role.

None block. All are ratify/record, plus the one cross-task collision item now consolidated for the human.

## Net effect

The v0.2.3 back-button `ipfs:///` leak is fixed at BOTH roots (async re-derive + normalized key), and - importantly - the test harness now models the real backend's async+normalization behaviour so my earlier fake-backend blind spot is closed. This unblocks `urlbar-tracks-in-page-navigation-not-just-pinned-name` (next), which relies on the normalized `ens_pages` re-derive to recover the root-entry name when the in-page pin is dropped.
