---
title: "Gate-3 conductor review: preserve-ens-name-in-bar-on-reload-and-history (APPROVE)"
date: 2026-07-23
status: approved
reviewOf: preserve-ens-name-in-bar-on-reload-and-history
gate: gate-3-conductor
mergedCommit: 3b84060
---

## Verdict: APPROVE

Conductor Gate-3 diff-vs-criteria pass. Gate-1 + Gate-2 passed before merge. Driven in place from `work/tasks/backlog/` via `dorfl do ... --allow-backlog --isolated --review --merge`.

## Done-move + landing

- `work/tasks/backlog/preserve-ens-name-in-bar-on-reload-and-history.md` -> `work/tasks/done/` on origin/main (squash merge `3b84060`).
- Files: `crates/werust-core/src/lib.rs` (+352), a decision note `reload-re-resolves-ens-name-decision-2026-07-23.md`, the gate-2 nits note.

## Acceptance criteria (ticked against the diff)

- [x] Reload keeps the `.eth` name + ENS posture (NameViaTrustedRpc / MutableName), not the ipfs://<cid>. `reload` now branches: if the backend's current CID is a known ENS-originated page (`ens_pages`), it re-runs `navigate_ens_name` (front-door resolution) which re-pins the name + re-marks the posture. Test `reloading_an_ens_page_re_resolves_and_keeps_the_name_and_posture_in_the_bar`.
- [x] The mutable-name reload decision is made + recorded. DECIDED: reload RE-RESOLVES the name (honest "get the current version" for a mutable IPNS/repointable-ENS name; re-derives the same CID for immutable). Recorded in `reload-re-resolves-ens-name-decision-2026-07-23.md`. Content stays hash-verified via the ipfs:// scheme handler.
- [x] Back/forward onto an ENS page shows the name + posture, not the raw CID; a non-ENS entry shows its real URL. Mechanism: an `ens_pages: HashMap<ipfs-url, EnsIdentity>` association populated in `load_resolved_content`, consumed in `refresh_chrome` to re-derive the name + re-mark the posture whenever the backend's current_url is a known ENS page (no re-resolve needed for history — just re-decorate). Tests `back_and_forward_onto_an_ens_page_show_the_name_and_posture_not_the_cid`, `back_onto_a_mutable_ipns_ens_page_keeps_the_name_and_mutable_axis`.
- [x] Plain ipfs:// / served page unaffected; ENS name never leaks onto a non-ENS page. Tests `reloading_a_plain_ipfs_page_is_unaffected_and_never_grows_the_eth_name`, `a_non_ens_history_stack_is_wholly_unaffected_by_the_ens_association`. (See nit 1 for the one narrow collision edge.)
- [x] Applied on desktop AND mobile shells. Fix lands in the SHARED `BrowserShell` core: desktop + Android + iOS all drive the SAME `reload`/`go_back`/`go_forward` and read `chrome()`, so the fix is cross-platform with NO per-shell change. Correct parity approach (verified: the diff touches only werust-core, which all three shells consume).
- [x] Tests cover reload + back + forward across ENS + non-ENS pages (fake backend), asserting bar text + posture. 4 new tests, network-isolated.

## Forward-notes / drift honoured

Task carried the confirmed root-cause (reload/go_back/go_forward setting url_override=None) and the recommended re-resolve decision. Both honoured: the re-resolve recommendation was adopted and recorded; url_override is no longer unconditionally dropped for an ENS-originated reload. No drift.

## Review-nits triage (Gate-2)

1. `ens_pages` is keyed on the underlying ipfs://<cid>, so if the user later navigates DIRECTLY to that same raw CID as a plain page, refresh_chrome re-derives the .eth name + re-marks the ENS posture — showing an ENS identity the user did not type. NOT a verification downgrade (same hash-verified bytes; that CID genuinely IS what the name resolved to), a display-identity nicety. No test covers this collision. NON-BLOCKING but FLAGGED for a human decision: is decorating a direct-CID visit with a prior name's identity intended, or should a direct raw-CID navigation suppress the association? Candidate small follow-on (suppress/scope the decoration to name-originated navigations, and add a collision test). Does not block: it is an ENS-DERIVED CID, not a leak onto an unrelated non-ENS page, and the acceptance intent (no leak onto a non-ENS page) holds for genuinely non-ENS content.
2. `ens_pages` is insert-only, never pruned/bounded for the session. Fine for a browsing session; worth a note for a very long-lived session. RATIFIED — benign; a bounded/LRU map is a trivial future tidy if ever needed.
3. Ratify the recorded reload decision's history side-effect: re-resolve goes through `renderer.navigate`, which PUSHES a history entry, so reloading an UNCHANGED CID can add an adjacent duplicate history entry (reload is conventionally in-place). Accepted as Phase-1 cost in the decision note (no "replace current entry" seam exists yet). RATIFIED as a documented Phase-1 cost; a future "replace-current-entry" seam or backend-native ENS-aware reload removes it.

## Net effect

An ENS page's `.eth` name + trust posture now survive reload, back, and forward on every platform; the Phase-1 front-door criterion ("keep the .eth name in the bar while the internal load is the resolved CID") now holds beyond the first Enter. One display-identity edge (nit 1) surfaced for the human to ratify or tighten.
