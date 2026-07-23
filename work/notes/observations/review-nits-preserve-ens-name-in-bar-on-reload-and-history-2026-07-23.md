---
title: review-gate non-blocking nits for 'preserve-ens-name-in-bar-on-reload-and-history' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: preserve-ens-name-in-bar-on-reload-and-history
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'preserve-ens-name-in-bar-on-reload-and-history' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- ens_pages is keyed purely on the underlying ipfs://<cid>. If the user later navigates DIRECTLY to that same raw CID as a plain page, refresh_chrome will re-derive the .eth name + re-mark the ENS posture, showing an ENS identity the user did not type. Is decorating a plain CID visit with a prior name's identity intended? Not a verification downgrade (same hash-verified bytes) but a display-identity inaccuracy; no test covers this collision (the plain-reload test uses a distinct CID).
  (crates/werust-core/src/lib.rs refresh_chrome ens_entry lookup + load_resolved_content insert keyed on renderer.current_url())
- ens_pages only ever grows (insert-only, never pruned/bounded) for the session. Fine for a browsing session but worth a note for a long-lived session.
  (load_resolved_content self.ens_pages.insert(...) with no eviction)
- Ratify the recorded reload decision: reload of an ENS page RE-RESOLVES via navigate_ens_name, whose renderer.navigate PUSHES a history entry, so reloading an UNCHANGED CID can add an adjacent duplicate history entry (reload is conventionally in-place). Accepted as Phase-1 cost in the decision note; human to ratify or reverse.
  (work/notes/observations/reload-re-resolves-ens-name-decision-2026-07-23.md 'Reload's history side-effect')
