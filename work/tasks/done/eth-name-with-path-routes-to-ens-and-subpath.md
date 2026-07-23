---
title: "A .eth name WITH a path (ronan.eth/blog/) must route to the ENS front door and load ipfs://<cid>/<path>, not fall through to https://ronan.eth/blog/"
slug: eth-name-with-path-routes-to-ens-and-subpath
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

FIELD FINDING (v0.2.4, human): entering `ronan.eth/blog/` in the URL bar is NOT detected as an ENS name - werust tries `https://ronan.eth/blog/`, which fails with `Error resolving "ronan.eth": Name or service not known`. Root-cause source: `work/notes/observations/field-test-v0.2.4-spa-clientrouting-eth-path-blank-links-2026-07-23.md` (finding B).

READ-FIRST / drift check: confirm the mechanism. `eth_name_from_entry` (`crates/werust-core/src/lib.rs`) REJECTS any entry containing `/` after stripping ONE trailing slash ("a `ronan.eth/page` entry is not a bare name in Phase 1"), so `ronan.eth/blog/` falls through to the scheme-less classifier `classify_entry`, which sees a plausible dotted host and routes it to `https://ronan.eth/blog/` -> DNS failure. Confirm the `name.contains('/')` rejection and the classifier fallthrough still hold.

Fix: recognise a `.eth` name WITH a path as the ENS front door. Split a scheme-less entry `<label>.eth[/<path>]` into the ENS NAME (`<label>.eth`) and the PATH (`/<path>`), resolve the name through the existing front door (`navigate_ens_name`), and feed the path into the resolved load as `ipfs://<cid>/<path>` (the `ipfs://` path resolution already supports sub-paths + directory `index.html`, per `docs/adr/0004` and `resolve_ipfs_request`). So `ronan.eth/blog/` resolves `ronan.eth` -> `ipfs://<cid>` and loads `<cid>/blog/` (its `index.html`), keeping `ronan.eth/blog/` in the URL bar with the ENS posture.

Design + coherence:
- Change `eth_name_from_entry` (or add a sibling `eth_name_and_path_from_entry`) so it returns the NAME plus an optional PATH for a `<label>.eth/<path>` entry, instead of rejecting on the first `/`. Keep the existing guards: an explicit scheme (`ipfs://`, `https://…`) is still taken literally (never hijacked); a bare `.eth` (no path) is unchanged; a non-`.eth` host still classifies as before (this must not turn `github.com/foo` into an ENS name - only a `.eth` TLD label does).
- `navigate_ens_name` currently resolves the name and loads the ROOT `ipfs://<cid>`. Thread the optional path through so the resolved load targets `ipfs://<cid>/<path>` (for both the `ipfs-ns` and IPNS-resolved cases). The path goes into the `renderer.navigate` URI and into the `ens_pages` association / `pinned_root_key` so reload/back/forward and the bar keep working (compose with the ens-history + urlbar-in-page tasks: the pinned display is `ronan.eth/blog/`, the normalized root key is the CID+path form the existing normalizer produces).
- Trailing slash: `ronan.eth/blog/` and `ronan.eth/blog` should resolve the same entity (the ipfs path resolution + the normalizer already handle a directory + trailing slash); keep it consistent with `normalize_ens_page_key`.
- Fail-closed unchanged: a bad path (no such entity in the DAG) fails with the existing `PathNotFound`-class reason surfaced in the chrome, keeping the `.eth/<path>` in the bar for the user to see the reason (mirroring a failed bare-name load).

## Acceptance criteria

- [ ] `ronan.eth/blog/` (and `ronan.eth/blog`) is detected as the ENS front door for `ronan.eth`, resolves the name, and loads `ipfs://<cid>/blog/` (its index.html) - NOT `https://ronan.eth/blog/`.
- [ ] The URL bar keeps `ronan.eth/blog/` (the identity + path the user typed) with the ENS trust posture, not the raw `ipfs://<cid>/blog/`.
- [ ] A bare `.eth` (no path) is unchanged; an explicit scheme is still taken literally (no hijack of `ipfs://`/`https://`); a non-`.eth` host with a path (`github.com/foo`) still routes to `https://github.com/foo`, NOT to ENS.
- [ ] A `.eth/<path>` whose path resolves to no DAG entity fails closed with the existing legible reason in the chrome, keeping the typed `.eth/<path>` in the bar (no silent reset, no https fallback).
- [ ] Reload / back / forward of a `.eth/<path>` page keep the name+path+posture (compose with `ens_pages` / `pinned_root_key`: the normalized key and the pinned display stay coherent).
- [ ] Applied on desktop and mobile (the routing is in the shared core front door), or tracked per the parity guard.
- [ ] Tests cover: `.eth/<path>` -> ENS + subpath load with the bar showing name/path; bare `.eth` unchanged; `github.com/foo` -> https (not ENS); a bad `.eth/<path>` fails closed keeping the bar; reload/back re-derive the name/path. Fake backend, network-isolated.

## Blocked by

- None. (Touches the same `navigate` front door + `eth_name_from_entry` as the scheme-less/urlbar tasks; those have landed, so build on them.)

## Prompt

> Goal: make `ronan.eth/blog/` route to the ENS front door (resolve `ronan.eth` -> `ipfs://<cid>`, load `<cid>/blog/`) instead of falling through to `https://ronan.eth/blog/` (DNS fail). Today `eth_name_from_entry` rejects any entry with a `/`, so a `.eth` name with a path is not recognised.
>
> Where to look: `crates/werust-core/src/lib.rs` - `eth_name_from_entry` (the `name.contains('/')` rejection), the `navigate`/`classify_entry` front door, `navigate_ens_name` (resolves + loads the ROOT `ipfs://<cid>` today - thread an optional path so it loads `ipfs://<cid>/<path>`), and the `ens_pages` / `pinned_root_key` / `normalize_ens_page_key` machinery (keep reload/back/forward + the pinned bar coherent for a name+path). The ipfs sub-path + directory-index resolution already exists (`docs/adr/0004`, `resolve_ipfs_request`).
>
> Split `<label>.eth[/<path>]` into name + path; resolve the name; load the subpath; pin `ronan.eth/blog/` in the bar with the ENS posture. Keep: bare `.eth` unchanged, explicit scheme literal, a non-`.eth` host+path -> https (NOT ENS), a bad path fail-closed keeping the bar. Done = the acceptance list, fake-backend network-isolated tests. FIRST re-check `eth_name_from_entry` still rejects `/`.
