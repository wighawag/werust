---
title: "Decision: reload of an ENS page RE-RESOLVES the name (not same-CID); back/forward re-derive the .eth name from a CID<->name association"
date: 2026-07-23
status: open
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
kind: decision
task: preserve-ens-name-in-bar-on-reload-and-history
---

## Context

Task `preserve-ens-name-in-bar-on-reload-and-history` requires reload + back/forward of an ENS-resolved page to keep the `.eth` name AND its ENS trust posture (`NameViaTrustedRpc` / `MutableName`) in the URL bar, instead of leaking the underlying `ipfs://<cid>`. The task asked for one decision to be made and recorded: on RELOAD of a MUTABLE name, does reload RE-RESOLVE the name (fetch the current pointer again) or reload the SAME resolved CID?

## Decision (as recommended by the task)

**Reload of an ENS-originated page RE-RESOLVES the name** (re-runs the front-door resolution `navigate_ens_name`), rather than reloading the same cached CID. A plain `ipfs://<cid>` page (no ENS origin) reloads the backend URL as today.

Rationale: re-resolve is the honest meaning of reload for a MUTABLE name (an IPNS name like `ronan.eth`, or an ENS name that can be repointed): reload = "get the current version", so it catches a changed CID / refreshes a mutable pointer. It is also correct for an immutable `ipfs-ns` name (it re-derives the same CID). Either way the `.eth` name + its posture stay in the bar and the content is still hash-verified by the `ipfs://` scheme handler.

## Alternative considered: reload the same CID

Reload the backend's current `ipfs://<cid>` and merely re-apply the pinned name + re-mark the ENS posture flags. Simpler, and it keeps the backend's history untouched. Rejected as the PRIMARY reload behaviour because it does NOT refresh a mutable pointer (a repointed IPNS/ENS name would keep showing the stale CID on reload) — dishonest for a mutable name. It is still the behaviour for a plain (non-ENS) `ipfs://` page, which IS immutable.

## What this touches (the honest cost)

- **Back / forward** rely on a separate mechanism: the shell now remembers a `ipfs://<cid>` -> `.eth`-name association for every ENS-originated load (`ens_pages`), and `refresh_chrome` re-derives the `.eth` name (and re-marks the ENS posture flags on the seam) whenever the backend's `current_url` lands on a known ENS page. This makes back/forward onto an ENS page show the name + posture WITHOUT re-resolving (history is the backend's; we just re-decorate the entry). A non-ENS entry shows its real URL as today.
- **Reload's history side-effect:** because the shell has no "replace current history entry" seam method (the backend owns history via `Renderer`), re-resolving on reload goes through the front door's `renderer.navigate(uri)`, which PUSHES a history entry for the freshly-resolved CID. For an UNCHANGED CID this can add an adjacent duplicate history entry (reload is conventionally in-place). This is accepted as the cost of honest mutable-name reload in Phase 1; a future "replace current entry" seam method (or a backend-native ENS-aware reload) could remove the duplicate. Recorded here so a reviewer/human can ratify or reverse.
- **Reload of a FAILED ENS load** (the front door pinned the name but never navigated the backend): reload re-runs the resolution from the pinned name, so a transient resolution failure is retryable — consistent with re-resolve.

## Where implemented

`crates/werust-core/src/lib.rs`: the `ens_pages` association map on `BrowserShell`, populated in `load_resolved_content`; consumed in `refresh_chrome` (re-derive name + re-mark posture); `reload` branches on whether the current page is ENS-originated and re-resolves via `navigate_ens_name`. Applied cross-platform for free: the desktop + Android + iOS shells all drive the SAME `BrowserShell::reload`/`go_back`/`go_forward` and read `chrome()`, so the fix lands on every platform with no per-shell change. Proven by reload/back/forward tests over the fake backend in the same file.
