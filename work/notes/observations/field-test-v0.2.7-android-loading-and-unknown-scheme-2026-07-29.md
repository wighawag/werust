---
title: "Field test v0.2.7 (Android, mobile network): loading banner is missing on slow retrievals; market:// and twitter inline-script violations are real but expected"
date: 2026-07-29
status: open
kind: field-finding
release: v0.2.7
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
---

## Context

Human field test of v0.2.7 on Android (mobile network, bypassing a router that DNS-blocks `ethereum-rpc.publicnode.com`). Tested `ronan.eth` and `jolly-roger.eth`.

## Findings

### A. (KEY for v0.2.7 quality) No loading affordance on long retrievals — Android shows "kill app / wait" repeatedly

When navigating around `ronan.eth` (blog list + blog posts + portfolio), the app periodically shows Android's "kill app / wait?" dialog (Android's 5-second UI-thread watchdog). The user repeatedly hits "wait" and the navigation completes — but there is NO chrome signal that the app is working. The user cannot tell whether they should wait, reload, or give up. The freeze is most likely the SvelteKit client router doing its own page-side work on the page's main thread (werust cannot speed that up), but the chrome should still TELL the user something is happening.

The chrome JSON already carries `load_state` (Started/Committed) and `load_step` (name-fetch / content-fetch / content-rendering / verifying / settled), and `is_loading()` is exposed — but no UI surfaces them during a load. The existing amber banner appears only on a failed load.

This is filed as a follow-on task (`loading-banner-with-phase-and-cancel` in backlog/) so it can land in a follow-up release; the fix is shell-only, the chrome contract already exists, no core change needed.

### B. market:// scheme errors — expected, app-store links from a non-wallet context

User saw `net::ERR_UNKNOWN_URL_SCHEME` on a `market://details?id=com.twitter.android` link. This is the WebView's honest answer: werust registers only `ipfs://` and the internal `https://*.ipfs.werust.invalid` origins. A `market://` link (a Play Store app-link) needs either (a) `shouldOverrideUrlLoading` routing to the platform intent system (`startActivity(Intent.ACTION_VIEW, Uri.parse("market://…"))`) — opening the Play Store app if installed, the web fallback otherwise — or (b) the user's choice to ignore. The current behaviour (an error banner) is honest; only path (a) would be a UX improvement. Filed as a candidate follow-on; not blocking.

### C. inline-script CSP violations on twitter — expected, not ours

The Console tab records two `'unsafe-inline' https://…` errors on twitter.com. These are Chrome's enforcement of a CSP directive the **page itself** declared, not anything werust did; an inline `<script src=…>` that violates the page's own CSP triggers them regardless of how the page was loaded. They are noise to filter (and the favicon-noise observation extends naturally here: the Console tab would benefit from a filter/search). Recorded for completeness; no action needed.

### D. RPC block by home router — reproduced as expected

The LAN's router DNS-blocks `ethereum-rpc.publicnode.com`, returning a block-page IP (`blocking.asus.hns.tm`). The chrome surfaces the full reason honestly (`rpc transport error: io: invalid peer certificate: certificate not valid for name "ethereum-rpc.publicnode.com"; certificate is only valid for DnsName("blocking.asus.hns.tm")`). On the mobile network the resolution worked and the navigation worked. Already documented in the mobile-no-nav task's DIAGNOSIS.md; re-confirms the honest-fail-closed surfacing is correct on desktop too.

## What works (v0.2.7 confirmed)

- Mobile network -> ENS resolution works -> page renders.
- The honest failure surface (the full reason in the banner + footer) on the desktop is unchanged from v0.2.6 and is correctly fail-closed.
- The 3xx navigation and the in-app debug menu both still work on the desktop (no regressions observed).

## Cross-cutting note

Finding A is the highest-impact item on this list and the only one I'd call user-facing-blocking: without a loading signal, a slow page is indistinguishable from a frozen app, and the user's only escape is the OS's kill-app dialog. The fix is small (chrome JSON already carries the data), shell-only, and the same task should land on all three platforms.
