---
title: "Gate-3 conductor review: track-webview-url-on-spa-clientside-navigation (APPROVE)"
date: 2026-07-23
status: approved
reviewOf: track-webview-url-on-spa-clientside-navigation
gate: gate-3-conductor
mergedCommit: e8ee13a
---

## Verdict: APPROVE

Conductor Gate-3 pass. Gate-1 passed; Gate-2 APPROVED on the recovery re-run (see below). Both coupled parts scrutinised + re-tested locally (this task fixes the human-reported `ipfs://`-reappears leak, so I verified that scenario's test directly).

## Done-move + landing

- `work/tasks/backlog/track-webview-url-on-spa-clientside-navigation.md` -> `done/` on origin/main (integrated the kept commit `e8ee13a`).
- Files (all platforms): `crates/renderer/src/lib.rs` (+38: `LoadEvent::UrlChanged`), `webview-renderer` (backend `notify::uri` + lib), Android (`BrowserActivity.kt` `doUpdateVisitedHistory` + rust backend/lib), iOS (`WKWebViewShellController.swift` KVO + rust backend/lib + header), `werust-core` (`ipfs.rs` `ipfs_root_cid_and_path`, `lib.rs` root-CID-prefix `ens_identity_for_url` + `EnsIdentity.root_cid/root_name`), capability matrix (a new `spa-url-tracking` row), DECISIONS.md, gate-2 nits note.

## Acceptance criteria (ticked, re-verified locally)

- [x] PART 2 (the leak): after loading an ENS site and navigating to a SUB-PATH, a back/forward/reload landing on ANY `<rootcid>/<path>` shows the `.eth` name (as name or name/path) + posture, NEVER `ipfs://<rootcid>/<path>`. `EnsIdentity` gained `root_cid`+`root_name`; `ens_identity_for_url` does exact-key-THEN-root-CID-prefix lookup (`ipfs_root_cid_and_path` splits the root CID from the in-site path). Test `history_return_onto_any_subpath_of_a_known_ens_site_re_derives_the_name_never_the_cid` - GREEN locally. This closes the v0.2.4 `ipfs://`-reappears leak the human hit.
- [x] PART 1 (SPA tracking): a client-side pushState/replaceState nav updates the bar. Modelled as a distinct `LoadEvent::UrlChanged { url }` (NOT a faked load), observed per platform: WebKitGTK `notify::uri`, iOS KVO on `webView.url`, Android `doUpdateVisitedHistory`. Flows through the one `pump()` drain -> `drop_pin_on_in_page_nav` + re-derive. Tests `a_spa_same_document_url_change_updates_the_bar_and_drops_the_pin` + `a_spa_url_change_on_a_plain_page_follows_the_url_unregressed` - GREEN.
- [x] Same-document change is a distinct signal, decision recorded (Decision 1). Plain non-SPA + full-page loads unregressed (posture-no-leak tests green). Applied desktop + mobile; new `spa-url-tracking` parity row (Decision 3, so a re-stubbed edge is caught).
- [x] Composes with the `.eth/<path>` task: `a_dot_eth_with_a_path_routes_to_ens_and_loads_the_subpath_keeping_the_name_path_in_the_bar` green.
- [x] Renderer seam test `url_changed_is_a_distinct_same_document_event_carrying_the_new_url` green.

## Review-nits triage (Gate-2)

Nits 1-3 RATIFY the three recorded decisions (UrlChanged variant; root-CID-prefix match; new parity row) - all sound, no action. Nit 4 (the one substantive one): a DESKTOP ordering nuance - on a genuine full-page load, WebKitGTK `notify::uri` can fire with the new target BEFORE `load-changed(Started)` calls `life.begin`, so `url_changed` may emit a SPURIOUS `UrlChanged` for a real load. Effect is BENIGN (`drop_pin_on_in_page_nav` is idempotent and `Started` runs it too; the `UrlChanged` arm never touches load state/error; `refresh_chrome` reconciles). FLAGGED for the human's live-desktop confirmation (no double-repaint / transient bar flicker) - headless tests can't exercise the real signal ordering. Non-blocking.

## Recovery this task required (flake, no work lost)

Two interruptions before landing: (1) a runner CRASH mid-implementation (my turn was interrupted) left an uncommitted partial in the job worktree, no branch pushed - discarded + requeued fresh. (2) On re-dispatch, Gate-1 passed but Gate-2's review VERDICT was UNPARSEABLE JSON ("Bad escaped character") - a stochastic-reviewer JSON-escaping flake, NOT a real block; the green work branch WAS pushed (`a4622c8`). Recovered with `requeue --arbiter origin -m "<flake, continue from kept branch, re-run review>"` + re-`do`: the re-claim continued from the kept branch tip, the review re-ran and produced a parseable APPROVE, and it integrated the kept commit. No work lost.

## Net effect

The two v0.2.4-addendum bugs are fixed: (1) SPA client-side navigation now updates the URL bar (the internal-nav frozen-bar case; the external `_blank` case was fixed by the blank-links task), and (2) the `ipfs://` CID no longer reappears in the bar on history return onto a sub-path of a known ENS site (the root-CID-prefix association replaces the leaky root-entry-only `ens_pages`). One benign desktop signal-ordering nuance flagged for live confirmation.
