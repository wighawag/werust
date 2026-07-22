---
title: Bare .eth URL-bar front door — resolve and render ronan.eth end to end
slug: bare-eth-urlbar-front-door-end-to-end
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: [ens-namehash-registry-resolver-contenthash-resolution, name-via-trusted-rpc-trust-state]
covers: [1, 2, 3, 4]
---

## What to build

Wire the front door so a user typing a bare `ronan.eth` in the URL bar loads the immutable IPFS site it points to, end to end, closing the tracer bullet.

- **Recognize a bare `.eth` URL-bar entry** (no scheme required, like Brave/Opera) and treat it as an ENS name to resolve. Strictness: treat a `*.eth` URL-bar entry on Enter (or a trailing `/`) as an ENS name; do NOT aggressively auto-resolve anything that merely looks name-ish. (An `ens://` secondary disambiguator is NOT required in Phase 1.)
- **Resolve** it via the ENS resolution core (namehash → registry → resolver → contenthash → ENSIP-7 decode).
- **Dispatch by the decoded contenthash type:** `ipfs-ns` → feed the `ipfs://<cid>` into the EXISTING verified `ipfs://` render path (which hash-verifies the bytes and renders at parity). Every other decoded type → the decoder's graceful named failure, surfaced as a legible chrome load failure. Do NOT default to `ipfs://` for non-ipfs types.
- **Keep `ronan.eth` in the address bar** (the identity the user cares about) while the internal load is the resolved CID. NO `https://` rewrite, NO trusted gateway redirect.
- **Trust state — and the posture-marking clash you MUST resolve:** on a successful `ipfs-ns` render via trusted-RPC resolution, the page must end up in the "content-verified, name via trusted RPC" posture (added by `name-via-trusted-rpc-trust-state`), never "verified"/`ContentVerified`. BUT the existing `ipfs://` render path already marks the load: the webview backend's `install_ipfs` scheme handler (`crates/webview-renderer/src/backend.rs`) UNCONDITIONALLY calls `mark_content_verified()` on ANY successful verified resolution, and the shell READS posture from the backend (`refresh_chrome` pulls `renderer.trust_posture()`, which returns the lifecycle's `ContentVerified`). So a naive "navigate to `ipfs://<cid>`" would render this ENS page as plain `ContentVerified`, silently DROPPING the new posture. This task OWNS designing and wiring the mechanism by which an ENS-originated load reports the new posture instead — e.g. the shell/front-door signals "this load came from ENS trusted-RPC resolution" so the backend/lifecycle reports the new variant, or the shell upgrades the posture for the ENS-originated load without being clobbered by the scheme handler's `mark_content_verified`. Whatever the mechanism, it must still track the ACTUAL load path (only a real ENS-resolved verified load gets it) and must not leak onto a later plain `ipfs://` or served load.
- **Fail-closed:** a name with no/invalid/unsupported contenthash, or a resolution that fails, FAILS the load with a legible reason, never renders anything unverified or guessed.

## Acceptance criteria

- [ ] A bare `ronan.eth`-style `*.eth` URL-bar entry (on Enter / trailing `/`) is recognised and routed to ENS resolution, not treated as a literal host.
- [ ] An `ipfs-ns` name resolves and renders the immutable IPFS site end to end through the EXISTING verified `ipfs://` path (bytes hash-verified), at parity with a served page.
- [ ] The address bar keeps the `.eth` name; there is no `https://` rewrite and no gateway redirect in the displayed URL.
- [ ] The page shows the "content-verified, name via trusted RPC" trust state (never "verified"/`ContentVerified`), even though the load went through the existing `ipfs://` path whose scheme handler marks `mark_content_verified` — i.e. the ENS-origin posture wins over the unconditional content-verified mark, driven by the real load path.
- [ ] A plain (non-ENS) `ipfs://` load still shows `ContentVerified`, and a served load still shows the unverified posture — the ENS posture does not leak onto them.
- [ ] A name whose contenthash is unsupported (ipns-ns / swarm-ns / arweave / unknown) fails the load with the decoder's distinct, protocol-named reason in the chrome.
- [ ] Fail-closed on every failure path (no/invalid contenthash, malformed, unsupported, resolution error) with a legible chrome reason; nothing unverified is ever rendered.
- [ ] Tests drive the end-to-end path (a bare `.eth` entry → resolution → verified render / graceful failure) network-isolated (pinned RPC + contenthash + content fixtures), and cover the new behaviour in the repo's existing test style, INCLUDING the posture outcome (ENS load ends in the new posture; a plain ipfs/served load does not).

## Blocked by

- Blocked by `ens-namehash-registry-resolver-contenthash-resolution` (the resolution core the front door calls).
- Blocked by `name-via-trusted-rpc-trust-state` (the trust posture this load marks, and the `TrustPosture`/`ChromeState` edit is serialized there to avoid a collision).

## Prompt

> Goal: close the tracer bullet — a user types a bare `ronan.eth` in the URL bar and werust loads the immutable IPFS site it points to, honestly labelled "content-verified, name via trusted RPC". This wires the front door onto the resolution core and the existing verified render path; it is the end-to-end, demoable slice.
>
> Domain vocabulary: the "front door" is a bare `.eth` typed in the URL bar with NO scheme (what Brave/Opera do). The settled `.eth`-input rule (spec's Settled decisions): treat a `*.eth` URL-bar entry, on Enter or a trailing `/`, as an ENS name — do not aggressively auto-resolve anything merely name-ish, and `ens://` is at most a secondary disambiguator (not required in Phase 1). Dispatch is by the decoded contenthash's OWN type: only `ipfs-ns` is rendered (into the existing verified `ipfs://` path); every other type is the decoder's graceful named failure. Never default to `ipfs://`.
>
> Where to look: the URL-bar Enter → `navigate` path lives in the `werust-core` `BrowserShell::navigate` and the desktop `werust` binary's `url_entry.connect_activate` (`crates/werust/src/main.rs`, which calls `shell.navigate(&entry.text())`). The existing verified `ipfs://` render path is `werust-core`'s ipfs module (`resolve_ipfs_request` / `parse_ipfs_uri`) wired by the webview backend's `install_ipfs` (`crates/webview-renderer/src/backend.rs`) — feed your resolved `ipfs://<cid>` into THAT path rather than re-implementing verification. The address bar must keep the `.eth` name: the shell already tracks a URL-bar string distinct from the underlying load (see `ChromeState.url_text` and `refresh_chrome`), so display the name while loading the CID — no `https://` rewrite. The resolution core (blocking task `ens-namehash-registry-resolver-contenthash-resolution`) turns the name into the decoded reference; the trust-state task (`name-via-trusted-rpc-trust-state`) added the posture variant and its plumbing.
>
> The posture-marking clash is the load-bearing trap here (do NOT skip it): the shell READS posture from the backend — `refresh_chrome` pulls `self.renderer.trust_posture()`, and the webview backend returns the lifecycle's posture, which `install_ipfs`'s scheme handler sets to `ContentVerified` via `mark_content_verified()` on ANY successful verified `ipfs://` resolution (it knows nothing about ENS). So if you just `navigate("ipfs://<cid>")`, the page renders as plain `ContentVerified` and the new "name via trusted RPC" posture never appears. YOU own the mechanism that makes an ENS-originated verified load report the NEW posture instead: signal the ENS origin into the load path so the backend/lifecycle reports the new variant, or upgrade the posture shell-side for the ENS-originated load in a way the scheme handler's mark cannot clobber. It MUST stay driven by the real load path (only an actual ENS-resolved verified load), and MUST NOT leak onto a later plain `ipfs://` or served navigation (a fresh `begin`/navigation resets to untrusted). Prove both directions in a test.
>
> Fail-closed is a hard requirement: no/invalid/unsupported contenthash or a resolution failure fails the load with a legible chrome reason (the shell already surfaces `last_error`), never a guessed or unverified render.
>
> Keep it network-isolated: reuse the pinned-fixture harnesses the blocking tasks established (RPC fixture endpoint + contenthash fixtures + a pinned content source/loopback gateway) so the end-to-end test needs no live network — mirror the offline test style across `fetcher` / `werust-core::ipfs`.
>
> Done = a bare `.eth` entry resolves and renders an `ipfs-ns` site through the verified path with the name in the bar and the trusted-RPC trust state (proven distinct from a plain `ipfs://` load's `ContentVerified`), an unsupported name fails with its named reason, and it is all proven offline. FIRST re-check the blocking tasks landed as assumed (the resolution API, the new posture variant + its wiring hook, the ipfs render path's `mark_content_verified` behaviour) — if any differs, route to needs-attention rather than build on a stale premise (WORK-CONTRACT.md "Drift is a needs-attention signal"). RECORD non-obvious in-scope decisions (the exact `.eth` recognition rule you implement, how you keep the name in the bar while loading the CID, and above all the mechanism by which the ENS-origin posture wins over the scheme handler's content-verified mark) durably per the task template.
