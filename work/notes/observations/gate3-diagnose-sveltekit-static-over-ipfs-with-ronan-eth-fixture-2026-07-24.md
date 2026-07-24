---
title: "Gate-3 conductor review: diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture (APPROVE)"
date: 2026-07-24
status: approved
reviewOf: diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture
gate: gate-3-conductor
mergedCommit: 9eeb31b
---

## Verdict: APPROVE

Conductor Gate-3 pass. Gate-1 + Gate-2 passed before merge (first dispatch). A diagnose-then-fix task, so I scrutinised whether the diagnosis is evidence-based (it is, exemplary) and re-ran the tests locally.

## Done-move + landing

- `work/tasks/backlog/diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture.md` -> `done/` on origin/main (squash merge `9eeb31b`).
- Files: `crates/werust-core/src/ipfs.rs` (+83: the query/fragment strip + tests), `crates/fetcher/src/retriever.rs` (+87: the regression fixture), a `DIAGNOSIS.md` (+87), an asymmetry observation note, gate-2 nits note.

## The diagnosis is PRIMARY-SOURCE and evidence-based (the key thing for a diagnose task)

The DIAGNOSIS cites the ACTUAL SvelteKit runtime source: `add_data_suffix` (`packages/kit/src/runtime/pathname.js`) + the `INVALIDATED_PARAM` (client.js), deriving the EXACT URL werust receives on the blog client-nav: `ipfs://<rootcid>/blog/__data.json?x-sveltekit-invalidated=01`. ROOT CAUSE (reproduced headlessly pre-fix): `parse_ipfs_uri` took the whole remainder after the CID as the DAG path with NO query/fragment handling, so the last path segment was the literal `__data.json?x-sveltekit-invalidated=01`, which matches NO directory entry -> `PathNotFound` -> the `ipfs://` load fails -> SvelteKit's client fetch rejects -> its error boundary renders = the "500" with no posts. The portfolio-works/blog-fails asymmetry is correctly explained as a symptom-ordering artefact (the initial full-page load renders the home/portfolio surface WITHOUT a client-nav `__data.json` fetch; the blog is reached client-side, so its `__data.json?...` fetch hits the bug), NOT a second root cause.

## Acceptance criteria (ticked, re-verified locally)

- [x] Root cause DIAGNOSED + recorded with evidence (`DIAGNOSIS.md`): the query/fragment leak into the resolved DAG path.
- [x] Fixed: `parse_ipfs_uri` now strips a trailing fragment (`#...`) THEN a query (`?...`) before the `<cid>[/path]` split, so the DAG path is the clean `/blog/__data.json`. A query/fragment is a request modifier, never part of a content-addressed path, so stripping it can only correct resolution. Test `a_sveltekit_data_fetch_with_the_invalidated_query_resolves_the_nested_data` (end-to-end seam) green.
- [x] Verification/fail-closed unchanged: every block still hash-verified; a genuinely-missing nested resource still `PathNotFound` (the fixture asserts `/blog/does-not-exist.json` fails closed). The fix is correct resolution, not a bypass.
- [x] A SMALL committed network-isolated regression fixture landed: `a_sveltekit_adapter_static_build_serves_the_nested_page_and_its_data_json` synthesizes a minimal adapter-static-shaped DAG (root `index.html` + `_app/` + nested `blog/` with `index.html` + `__data.json`) as a real dag-pb/UnixFS CAR offline, asserting werust resolves + serves the nested page AND its `__data.json`, and that a missing nested resource fails closed. Guards the SvelteKit-over-ipfs class. Green locally.
- [x] The MOBILE no-navigation half is explicitly PARKED for a human device re-test (recorded in the DIAGNOSIS "Parked follow-on" + the done-record), NOT attempted here - exactly as scoped.
- [x] Tests network-isolated, ride `cargo test`. All 4 green locally (2 strip unit tests + the seam regression + the fixture).

## Review-nits triage (Gate-2) - two flags for the human

1. The strip is in `parse_ipfs_uri` (the RETRIEVAL seam) but NOT in `normalize_ens_page_key` / `ipfs_root_cid_and_path` (the ENS-BAR-KEY seam). Correct for THIS bug (the `__data.json` subresource fetch flows through `resolve_ipfs_request`/`parse_ipfs_uri`), but a bar-display / history URL that ever carried a query would key differently in the ens_pages association. FLAGGED: confirm the ENS-key seam never needs the same strip (it likely doesn't - a pinned bar URL is a name or a clean ipfs:// path, not a `__data.json?...` subresource - but worth a human confirm). Non-blocking.
2. The portfolio/blog asymmetry is reasoned from SvelteKit source, NOT confirmed against a live ronan-eth build (absent from the worktree). Recorded as an open observation (`sveltekit-ipfs-query-strip-portfolio-vs-blog-asymmetry-2026-07-24.md`) for the next on-device pass. FLAGGED, fine to leave.

Neither blocks.

## Net effect

The ronan.eth blog "500" is fixed at its root: werust no longer leaks SvelteKit's `?x-sveltekit-invalidated` query (or any query/fragment) into the content-addressed DAG path, so a SvelteKit adapter-static site's nested prerendered pages AND their `__data.json` client-nav data resolve over `ipfs://`. A committed regression fixture guards the class. The mobile no-navigation half is parked for your device (it needs on-device diagnosis - possibly the SPA nav not proceeding on Android, or an interaction with the ANR-fix executor).
