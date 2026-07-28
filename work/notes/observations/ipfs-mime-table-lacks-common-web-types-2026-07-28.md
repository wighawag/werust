---
title: "ipfs:// MIME table lacks common web types (`.ico`, `.webmanifest`, fonts); unknown types default to text/html"
date: 2026-07-28
kind: observation
status: open
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
---

Spotted in the debug view's Network tab during the `mobile-ronan-eth-buttons-no-navigation` end-to-end: `ipfs://<cid>/pwa/favicon.ico` (a real 7.6 KB file in the ronan.eth build) is served as `GET 200 text/html`.

`mime_type_for_path` (`crates/werust-core/src/ipfs.rs`) knows a handful of extensions and falls through to `DEFAULT_MIME_TYPE = "text/html"` (the default is deliberate so a bare `ipfs://<cid>` opens as a page), so `.ico`, `.webmanifest`, `.woff/.woff2`, `.map`, etc. are all labelled `text/html`. Chromium sniffs and tolerates this for favicons, so nothing visibly breaks, but the served MIME is wrong and the debug Network tab reports it faithfully, which looks confusing ("text/html" for an icon). A small table extension (or content-sniffing for the unknown case) would fix it; low priority.
