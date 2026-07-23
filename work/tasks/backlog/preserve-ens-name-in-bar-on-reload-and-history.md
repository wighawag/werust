---
title: "Keep the .eth name (and its trust posture) in the URL bar on reload and back/forward — don't leak the ipfs://<cid>"
slug: preserve-ens-name-in-bar-on-reload-and-history
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [2]
---

## What to build

Stop reload / back / forward from replacing an ENS page's `.eth` name in the URL bar with the raw underlying CID. FIELD FINDING (v0.2.2): reloading `ronan.eth` shows `ipfs://bafybei…` in the bar; same for `mandalas.eth`. ROOT CAUSE (confirmed): `BrowserShell::reload` sets `url_override = None` (and its docstring documents this as an intentional Phase-1 choice), so `refresh_chrome` falls back to the backend's `current_url` (the CID). `go_back` / `go_forward` do the same. This VIOLATES the Phase-1 criterion the front door was built for ("keep the `.eth` name in the address bar while the internal load is the resolved CID") — it holds on the first Enter but is dropped on reload/history.

Fix: an ENS-originated page keeps its `.eth` name AND its ENS trust posture (`NameViaTrustedRpc` / `MutableName`, not the plain `ContentVerified` the bare CID would show) across reload and history navigation.
- **Reload**: keep the `.eth` name pinned. DECISION to make + record: reload a MUTABLE name (IPNS like ronan.eth, or an ENS name that can be repointed) by RE-RESOLVING the name (fetch the current pointer again — reload = "get the current version", and it catches a changed CID / refreshes a mutable pointer), vs reloading the SAME resolved CID. Recommended: re-resolve for a name-originated load (it is the honest meaning of reload for a mutable name and is correct for immutable too), keeping the name + posture in the bar; a bare `ipfs://<cid>` load reloads the CID as today. Do not drop `url_override` on reload of an ENS page.
- **Back / forward**: the shell keeps no URL stack (history is the backend's), so preserve the ENS identity by remembering which loads were ENS-originated (a CID<->name association, or re-derive the bar display for a history entry) so navigating back onto an ENS page shows the `.eth` name + posture, not the CID.

## Acceptance criteria

- [ ] Reloading an ENS-resolved page keeps the `.eth` name in the URL bar (not the `ipfs://<cid>`), and keeps its ENS trust posture (`NameViaTrustedRpc` / `MutableName`).
- [ ] The reload behaviour for a mutable name (re-resolve vs same-CID) is decided and recorded; whichever is chosen, the name + posture stay in the bar and the content is still hash-verified.
- [ ] Back / forward onto an ENS-originated page shows the `.eth` name + its posture, not the raw CID; a non-ENS history entry shows its real URL as today.
- [ ] A plain `ipfs://` or served page is unaffected (reload/back/forward show its real URL); the ENS name never leaks onto a non-ENS page.
- [ ] Applied on desktop and the mobile shells (the URL bar / posture are cross-platform parity surfaces) or tracked per the parity guard.
- [ ] Tests cover reload + back + forward across an ENS page and a non-ENS page (a fake backend), asserting the bar text + posture, mirroring the existing front-door/posture tests.

## Blocked by

- None — can start immediately.

## Prompt

> Goal: keep an ENS page's `.eth` name (and its NameViaTrustedRpc / MutableName posture) in the URL bar on reload and back/forward — today reload/history drop `url_override` and leak the `ipfs://<cid>`. The front door pins the name on the first Enter; reload/back/forward must not lose it.
>
> Where to look: `crates/werust-core/src/lib.rs` — `reload` / `go_back` / `go_forward` all set `url_override = None` (reload's docstring documents the intentional-but-wrong drop); `navigate_ens_name` sets `url_override = Some(name)` and marks the ENS posture; `refresh_chrome` shows `url_override` else the backend `current_url`. The backend owns session history (`Renderer::go_back/go_forward/reload`), so preserving the ENS name across history needs the shell to associate history entries / the current CID with the ENS name. Decide reload-re-resolves-vs-same-CID for a mutable name and record it. Keep loading/error/trust semantics from the recent tasks intact.
>
> Done = reload + back + forward keep the `.eth` name + ENS posture for an ENS page, a non-ENS page is unaffected, the mutable-name reload behaviour is decided + recorded, applied on desktop + mobile (or tracked), proven with reload/history tests. FIRST re-check the reload/history code still drops `url_override`. RECORD the reload decision durably.
