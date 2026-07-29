---
title: "Mobile ipfs:// page renders fully BLACK (parked: closed by v0.2.7 origin-map fix; mandalas.eth now renders on mobile)"
slug: ipfs-site-mobile-black-page
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## Status

CLOSED by the v0.2.7 origin-map fix (task `mobile-ronan-eth-buttons-no-navigation`), NOT by this task's prescribed MIME/bytes trace.

## Why this is closed without action

The v0.2.3 field test noted "mandalas.eth is still black but jolly-roger.eth and ronan.eth works fine" and routed this into the parked task as the re-test result. The v0.2.7 origin-map fix (the Android WebView now loads the page on `https://<cid>.ipfs.werust.invalid` instead of the opaque `ipfs://` origin) closed the root cause for all three sites, including mandalas.eth, which now renders on mobile per human re-test on v0.2.7.

## What the parked task WOULD have diagnosed (for posterity)

The parked task's prescribed candidates — wrong bytes/empty body, wrong MIME/charset, the `WebViewAssetLoader`/`appassets` fallback mis-serving, response streaming/threading — were symptoms of the SAME underlying cause: an `ipfs://` document served through `WebViewClient.shouldInterceptRequest` gets an OPAQUE origin in the Android System WebView. The opaque origin breaks Blink-level behaviours (relative fetches, `pushState`, `localStorage`) but it ALSO caused the document to render with reduced Blink features; on some sites (those relying on client-side hydration, dynamic imports, or specific MIME-driven rendering paths) the result was a fully black or blank page. The origin-map fix resolves the same root cause for ALL such cases.

## No follow-on needed

The origin-map fix is on `origin/main` as part of v0.2.7; no further work for this parked task. Recording it as closed so the ledger is honest and a future conductor does not re-pick it up.

## What the human should know

The full black-page symptom class (mobile ipfs:// renders black for SOME sites while the same site over https works) is fixed by v0.2.7. If a NEW site shows up black on v0.2.7+ mobile, that is a NEW issue (not this task); capture it as a fresh field finding.
