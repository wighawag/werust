---
title: "Reload (and back/forward) of an ENS page LEAKS the ipfs://<cid> into the URL bar instead of keeping the .eth name"
date: 2026-07-23
status: open
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
kind: field-observation
source: human manual test of the v0.2.2 build (desktop)
---

## What the human observed

Reloading an ENS-resolved page replaces the `.eth` name in the URL bar with the raw underlying CID:
- reload `ronan.eth` -> bar shows `ipfs://bafybeiepw4aijr4dtlhth2xkzskxaxcjvtk6neqsd6zua7rfv6m5nbkesu`
- reload `mandalas.eth` -> bar shows `ipfs://bafybeiavxbgbnouuvgwervyknnrjujtkckyhhur25k5m5gl7llyvinmlwu/`

## Root cause (confirmed in code)

`BrowserShell::reload` (crates/werust-core/src/lib.rs) explicitly sets `self.url_override = None`, and its own docstring documents this as intentional: "A reload re-loads the backend's CURRENT underlying URL (the resolved ipfs://<cid>), so it DROPS any pinned ENS name from the bar and follows the backend; Phase 1 does not re-resolve the name on reload." `refresh_chrome` then shows the backend's `current_url` (the CID) because the override that pinned the `.eth` name is gone. `go_back` / `go_forward` do the SAME (`url_override = None`), so navigating history back onto an ENS page would also show the CID.

This VIOLATES the Phase-1 acceptance criterion the front door was built + tested for ("keep the `.eth` name in the address bar while the internal load is the resolved CID"). It holds on the initial Enter navigation (the front door pins `url_override = Some(name)`) but is dropped on reload/back/forward. It is a deliberate-but-wrong decision, not an accidental bug.

## Fix direction + a decision to settle

Preserve the ENS identity across reload and history navigation: reload/back/forward onto an ENS-originated page must keep the `.eth` name in the bar (and keep the ENS trust posture — NameViaTrustedRpc / MutableName — not the plain ContentVerified the raw CID would show). DECISION to record: on reload of a MUTABLE name (IPNS like ronan.eth, or ENS which can be repointed), does reload RE-RESOLVE the name (fetch the current pointer again — arguably correct: reload = "get the current version", and it would catch a changed CID) or reload the SAME resolved CID? For an immutable ipfs-ns the same CID is fine; for a mutable name re-resolving is more correct. Either way the .eth name + its posture must stay in the bar. For back/forward, the shell keeps no URL stack (history is the backend's), so preserving the ENS name across history needs the shell to remember which history entries were ENS-originated (map CID<->name), or re-derive the display from the entry.
