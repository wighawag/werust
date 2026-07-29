---
title: "Field test v0.2.7: mandalas.eth renders on mobile — the parked mobile-black-page task is closed by the v0.2.7 origin-map fix"
date: 2026-07-29
status: closed
kind: field-finding
release: v0.2.7
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
---

## Context

The parked task `ipfs-site-mobile-black-page` was opened on a v0.2.1 field finding (Android: ENS/ipfs:// site rendered fully black, while the SAME site over `https://mandalas.eth.limo` rendered fine). The v0.2.3 field test noted that mandalas.eth was STILL black on mobile, while jolly-roger.eth and ronan.eth worked — and routed that detail into the parked task as the re-test result.

## Finding

On v0.2.7, the human re-tested mandalas.eth on mobile and it renders correctly. The v0.2.7 origin-map fix (the Android WebView now loads the page on the `https://<cid>.ipfs.werust.invalid` internal origin rather than the opaque `ipfs://` origin) closed the root cause for all three sites, including mandalas.eth.

## Action

The parked task `ipfs-site-mobile-black-page` is closed via a done-record. No further work for it; the full symptom class (a subset of ENS/ipfs sites rendering black on mobile) was a single root cause fixed by the origin-map task. A NEW site showing up black on v0.2.7+ is a NEW issue, not this one — capture as a fresh field finding.

## What the diagnosis WOULD have looked like

The parked task's candidates (wrong bytes/empty body, wrong MIME/charset, `WebViewAssetLoader` mis-serving, response streaming) were all symptoms of the same root cause: the opaque origin. The origin-map fix removes the opaque origin and the symptoms disappear together. Captured here so a future conductor does not re-litigate this class.
