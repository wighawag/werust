---
title: "Back/forward onto an ENS page must re-derive the .eth name on the REAL (async) backend and match on a NORMALIZED CID key (fixes ipfs:/// leaking into the bar)"
slug: ens-history-name-rederive-async-and-normalized
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [2]
---

## What to build

FIELD FINDING (v0.2.3, human, DESKTOP): the back button STILL shows `ipfs:///<cid>` in the URL bar when the previous page was an ENS name - the `.eth` name is dropped on history navigation. This is a REGRESSION relative to the `preserve-ens-name-in-bar-on-reload-and-history` acceptance criterion "back/forward onto an ENS-originated page shows the `.eth` name + posture, not the raw CID": that criterion was verified only against the `FakeBackend`, whose behaviour does NOT match the real `WebViewRenderer`. Root-cause source: `work/notes/observations/field-test-v0.2.3-back-nav-anr-urlbar-noprotocol-2026-07-23.md` (finding A). Reload works; back/forward on the real backend does not.

READ-FIRST / drift check: confirm the mechanism is still as below before building. `BrowserShell::go_back`/`go_forward` (`crates/werust-core/src/lib.rs`) set `url_override = None` and rely on `refresh_chrome` to RE-DERIVE the `.eth` name from `ens_pages`, keyed on `self.renderer.current_url()`; `ens_pages` is populated in `load_resolved_content` keyed on `current_url()` at forward-load time. Confirm both keying sites still key on the raw `current_url()`.

Two confirmed reasons the re-derivation misses on the real backend, BOTH must be fixed:

1. **Async history.** On `WebViewRenderer` (`crates/webview-renderer/src/backend.rs`), `current_url()` reads the shared `LoadLifecycle`, which WebKitGTK updates ASYNCHRONOUSLY via `load-changed` signals on the GTK main loop. Immediately after `go_back()`, `current_url()` still reports the PREVIOUS entry, so the synchronous `refresh_chrome` in `go_back` looks up the wrong key and misses. The `FakeBackend` updates `current_url` synchronously and so always matches - hiding this. Fix: the ENS-history name re-derivation must run on the ASYNC pump (`pump` -> `refresh_chrome`) once `current_url` settles onto the ENS CID, not only synchronously inside `go_back`/`go_forward`. (It already runs in `refresh_chrome`, which `pump` calls - so verify WHY the settled pump still misses; reason 2 is the likely remaining cause.)

2. **URL normalization mismatch.** The displayed `ipfs:///` (TRIPLE slash) shows the string WebKitGTK reports for the entry after a back differs from the key stored in `ens_pages` at forward-load time (an authority-less `ipfs://<cid>` normalized by WebKit to `ipfs:///<cid>`, or a trailing-slash / percent-encoding difference). So even once `current_url` settles, `ens_pages.get(&url)` misses on the string mismatch. Fix: key `ens_pages` on a NORMALIZED CID form - canonicalize the `ipfs://` URL to a single stable key BOTH when INSERTING (in `load_resolved_content`) and when LOOKING UP (in `refresh_chrome` and in `reload`'s ENS-name lookup), so the forward-store key and the post-back key are the SAME. Normalize on the CID identity (strip/normalize the authority/slashes; ideally reduce to the canonical `<cid>[/<path>]`), not on the raw display string.

Keep everything else the recorded reload/back decision established: reload still re-resolves; a non-ENS history entry still shows its real URL; the ENS name never leaks onto a genuinely non-ENS page.

## Acceptance criteria

- [ ] Back and forward onto an ENS-originated page show the `.eth` name + its ENS posture (`NameViaTrustedRpc` / `MutableName`) on the REAL backend, never the raw `ipfs://<cid>` (nor `ipfs:///<cid>`).
- [ ] The re-derivation is robust to the backend's ASYNC history: it re-applies once `current_url` settles onto the ENS CID via the pump, not only synchronously in `go_back`/`go_forward`.
- [ ] `ens_pages` is keyed on a NORMALIZED CID form, applied identically at insert (forward load) and lookup (back/forward re-derive AND reload), so a WebKit-normalized `ipfs:///<cid>` / trailing-slash / encoding variant of the SAME entry still matches. The `ipfs:///` triple-slash leak is gone.
- [ ] A plain (non-ENS) history entry still shows its real URL; the ENS name never leaks onto a genuinely non-ENS page.
- [ ] The FakeBackend / test harness is upgraded (or a backend-level/integration test added) so it MODELS the real backend's async history settle AND the URL-normalization variance - so this regression class can no longer pass on a synchronous, identical-string fake. Tests assert the bar text + posture after back and forward, network-isolated.
- [ ] Applied on desktop and the mobile shells (they drive the same shared `BrowserShell`), or tracked per the parity guard.

## Blocked by

- None. (The core is in the shared `BrowserShell`; verify the normalization key is coherent with the reload re-resolve path.)

## Prompt

> Goal: fix the v0.2.3 field regression where the back button shows `ipfs:///<cid>` instead of the `.eth` name when the previous page was an ENS name. The `preserve-ens-name-in-bar-on-reload-and-history` back/forward re-derive was verified only against the synchronous `FakeBackend`; it misses on the real WebKitGTK backend for two reasons, both to fix.
>
> Where to look: `crates/werust-core/src/lib.rs` (`BrowserShell::go_back`/`go_forward` set `url_override = None`; `refresh_chrome` re-derives the `.eth` name from `ens_pages` keyed on `renderer.current_url()`; `load_resolved_content` INSERTS into `ens_pages` keyed on `current_url()`; `reload`'s ENS-name lookup). `crates/webview-renderer/src/backend.rs` (`current_url()` reads the shared `LoadLifecycle`, updated ASYNCHRONOUSLY by WebKitGTK `load-changed` signals). (1) ASYNC history: after `go_back()` the current_url has not settled onto the ENS CID yet, so re-derive must re-apply on the settled pump, not only synchronously. (2) NORMALIZATION: the `ipfs:///` triple-slash means the stored key (from forward load) differs from the post-back string; key `ens_pages` on a NORMALIZED CID form applied IDENTICALLY at insert AND every lookup (refresh_chrome + reload) so they match.
>
> Done = back/forward on the REAL backend show the `.eth` name + posture (never `ipfs://`/`ipfs:///`), robust to async settle, matching on the normalized key; a non-ENS entry still shows its real URL; the name never leaks onto a non-ENS page. CRUCIAL: upgrade the test harness/FakeBackend to MODEL async history settle + URL-normalization variance so this regression class can no longer pass on a synchronous identical-string fake. Network-isolated tests assert bar text + posture after back and forward. FIRST re-check the keying mechanism still matches the description.
