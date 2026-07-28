---
title: "Debug view Network tab: per-SPA-nav favicon noise buries the interesting entries; no filter"
date: 2026-07-28
kind: observation
status: open
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
---

Spotted while using the in-app debug view as the diagnosis instrument for `mobile-ronan-eth-buttons-no-navigation` (its first real field use; the sufficiency assessment is in `docs/spikes/mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md`).

On `ronan.eth`, EVERY SvelteKit client-side navigation produces ~5 favicon requests (the site's service-worker update check plus Chromium re-fetching `/pwa/favicon.ico` and `/pwa/favicon.png`; verified these accumulate per navigation, not while idle). The newest-first Network list therefore buries the one entry a user is usually looking for (the client router's `__data.json` fetch) within seconds, and the tab has no filter/search. The entries are in the 300-entry store; they are just hard to reach on a noisy site. A filter box (substring over the URL) or a "hide favicon/static-asset noise" toggle would keep the tab useful on real sites. Related minor point: `.ico` files are served `text/html` today (see `ipfs-mime-table-lacks-common-web-types-2026-07-28.md`), which makes these noise entries look even odder ("GET 200 text/html 7.4 KB" for a favicon).
