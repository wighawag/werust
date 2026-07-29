---
title: "Field test v0.2.7 (Android, mobile network): UI-thread page-signal callbacks block behind a CAR retrieval (the 'kill app / wait' dialog on ronan.eth) — fix is to take them off the SyncSession mutex; market:// and twitter inline-script violations are real but expected"
date: 2026-07-29
status: open
kind: field-finding
release: v0.2.7
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
---

## Context

Human field test of v0.2.7 on Android (mobile network, bypassing a router that DNS-blocks `ethereum-rpc.publicnode.com`). Tested `ronan.eth` and `jolly-roger.eth`.

## Findings

### A. (KEY for v0.2.7 quality, ROOT CAUSE REFINED) "kill app / wait" dialog on `ronan.eth` is werust's UI-thread blocking behind a worker-thread CAR retrieval — NOT the page's SvelteKit JS

User observed that navigating around `ronan.eth` (blog list + blog posts + portfolio) periodically triggers Android's "kill app / wait?" dialog. The user repeatedly hits "wait" and the navigation completes.

**Critically, the same site over `ronan.eth.limo` (the https gateway) is smooth.** The SvelteKit client-side router runs the IDENTICAL JS on both paths. So the freeze is NOT in the page's JS — it is in werust's threading on the `ipfs://` path specifically.

Root cause (confirmed by code review):

- `shouldInterceptRequest` runs on a WebView WORKER thread. It calls `core.resolveIpfs(url)`, which goes through `SyncSession::with(|s| s.resolve_ipfs_request(...))`. During a CAR retrieval this lock is held for SECONDS.
- The UI thread's WebView callbacks — `onPageStarted` (calls `core.onPageCommitted(url)`), `onPageFinished` (calls `core.onPageFinished(url)`), `doUpdateVisitedHistory` (calls `core.onUrlChanged(url)`), `onReceivedError` (calls `core.onPageFailed(url, reason)`) — ALSO go through `SyncSession::with(...)`. Each of these is a tiny pure-ish function (a `from_webview_url` URL map + a `VecDeque` push + an enum assignment) that does NOT need to wait for an in-flight CAR retrieval.
- When the worker thread is mid-CAR, every UI-thread page-signal callback queues behind it. SPA client-side nav on `ronan.eth` is particularly exposed because `doUpdateVisitedHistory` fires per URL update AND the `__data.json` round-trip keeps the worker thread holding the lock.
- Over the https gateway (`ronan.eth.limo`), the worker thread does NOT hold the session mutex during navigation — the WebView's network stack handles `https://` requests itself — so the UI thread never queues behind it.

Fix (a real werust bug, not just a UX gap): take the UI-thread page-signal callbacks off `SyncSession`'s mutex, the same way the debug capture reads already are. The `Rc<RefCell<...>>` handle in `backend.rs::AndroidInner` can be exposed as a clone-out handle on `SyncSession`, and the UI-thread callbacks do `let b = inner.clone(); b.borrow_mut().state = ...` — no mutex. The worker thread's `shouldInterceptRequest` keeps its `inner.clone()` + `borrow_mut()`. This is filed as a follow-on task (`mobile-page-signal-callbacks-off-session-lock` in backlog/), where the prompt names the exact files, the existing precedent, and the regression-guard test.

### B. (UX gap, separate from A) No loading affordance on long retrievals

Even after A is fixed, the page's own JS work can still starve the UI thread. The chrome already carries `load_state` (Started/Committed) and `load_step` (name-fetch / content-fetch / content-rendering / verifying / settled), and `is_loading()` is exposed — but no UI surfaces them during a load. The existing amber banner appears only on a failed load. Filed as `loading-banner-with-phase-and-cancel` in backlog/. Independent of A: the freeze is on us, the missing signal is a UX gap.

### C. market:// scheme errors — expected, app-store links from a non-wallet context

User saw `net::ERR_UNKNOWN_URL_SCHEME` on a `market://details?id=com.twitter.android` link. This is the WebView's honest answer: werust registers only `ipfs://` and the internal `https://*.ipfs.werust.invalid` origins. A `market://` link (a Play Store app-link) needs either (a) `shouldOverrideUrlLoading` routing to the platform intent system (`startActivity(Intent.ACTION_VIEW, Uri.parse("market://…"))`) — opening the Play Store app if installed, the web fallback otherwise — or (b) the user's choice to ignore. The current behaviour (an error banner) is honest; only path (a) would be a UX improvement. Filed as a candidate follow-on; not blocking.

### D. inline-script CSP violations on twitter — expected, not ours

The Console tab records two `'unsafe-inline' https://…` errors on twitter.com. These are Chrome's enforcement of a CSP directive the **page itself** declared, not anything werust did; an inline `<script src=…>` that violates the page's own CSP triggers them regardless of how the page was loaded. They are noise to filter (and the favicon-noise observation extends naturally here: the Console tab would benefit from a filter/search). Recorded for completeness; no action needed.

### E. RPC block by home router — reproduced as expected

The LAN's router DNS-blocks `ethereum-rpc.publicnode.com`, returning a block-page IP (`blocking.asus.hns.tm`). The chrome surfaces the full reason honestly (`rpc transport error: io: invalid peer certificate: certificate not valid for name "ethereum-rpc.publicnode.com"; certificate is only valid for DnsName("blocking.asus.hns.tm")`). On the mobile network the resolution worked and the navigation worked. Already documented in the mobile-no-nav task's DIAGNOSIS.md; re-confirms the honest-fail-closed surfacing is correct on desktop too.

## What works (v0.2.7 confirmed)

- Mobile network -> ENS resolution works -> page renders.
- The honest failure surface (the full reason in the banner + footer) on the desktop is unchanged from v0.2.6 and is correctly fail-closed.
- The 3xx navigation and the in-app debug menu both still work on the desktop (no regressions observed).

## Cross-cutting note

The most useful correction from this field test: when something appears to be "the page's JS" because the freeze only happens on a specific site, the diagnostic move is to check whether the same site on the https gateway ALSO freezes. Here, `ronan.eth.limo` is smooth, so the freeze is NOT the page — it is the path. That flips the question from "what is SvelteKit doing?" to "what does werust do differently over `ipfs://` than over `https://`?" and the answer (`shouldInterceptRequest` on a worker thread holding a session lock) was visible in the code immediately. Filed the fix.
