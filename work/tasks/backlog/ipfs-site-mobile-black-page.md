---
title: "Mobile ipfs:// page renders fully BLACK (but the same site works via https on mobile): fix the mobile ipfs:// interception content/MIME"
slug: ipfs-site-mobile-black-page
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

Fix the fully-black mobile render of an `ipfs://` site. FIELD FINDING (v0.2.1, human): on Android, an ENS/`ipfs://` site rendered FULLY BLACK — BUT the SAME site loaded via plain `https://mandalas.eth.limo` on mobile renders fine (not black). So the black page is SPECIFIC to werust's mobile `ipfs://` interception path, NOT the general base-styling issue (that one is covered separately by `webview-base-styling-wrong-on-all-pages`, and it reproduces on plain https too). Because https works on the same device, the mobile WebView itself renders fine; something werust's `ipfs://` mobile handler returns is wrong.

Trace what the mobile WebView requests for an `ipfs://` directory site and what `resolve_scheme` / the OS-edge interceptor returns per request, and fix the cause. Candidates:
- **Wrong bytes/empty body** for the root or a key resource (a black page can be an empty/failed document served as success).
- **Wrong MIME/charset** so the document is not parsed as HTML (served as the wrong type -> the WebView shows nothing/black), or a missing charset.
- **The internal-https mapping** (if the Android edge used the `WebViewAssetLoader`/`appassets` fallback rather than a native scheme) mis-serving the document or its sub-resources.
- **Response streaming/threading** on the mobile edge returning before bytes are ready.

Compare the exact response werust's mobile ipfs path returns for the document against what the working https load receives, and align it.

## Acceptance criteria

- [ ] An `ipfs://` directory site renders VISIBLY (not black) on Android, at parity with the same site over https on the same device.
- [ ] The same holds on iOS.
- [ ] The mobile `ipfs://` path returns the document + resources with correct bytes and MIME/charset (the black-page cause is identified and fixed, grounded in a real request/response trace, recorded).
- [ ] Fail-closed unchanged: a real failure still fails the load with its reason (a black page is never a silent success).
- [ ] Tests cover the mobile ipfs response shape (document served with the right type/bytes on each edge); the visual check is documented.

## Blocked by

- None — can start immediately. (Related to `webview-base-styling-wrong-on-all-pages`, which fixes styling on ALL pages incl. https; this fixes the ipfs-SPECIFIC mobile black page. If the base-styling task lands first, re-check whether black persists.)

## Prompt

> Goal: fix the fully-black mobile `ipfs://` render. FIELD FINDING: the same site is fine via https on the same mobile device, so this is werust's mobile `ipfs://` interception returning something wrong (bytes/MIME/charset/threading), NOT the general base-styling gap. Trace the mobile WebView's requests + werust's responses for an ipfs directory site and align them with the working https response.
>
> Where to look: `crates/werust-android` (the scheme interception + `resolve_scheme` / the OS-edge, whether native scheme or the internal-https mapping) and `crates/werust-ios`; the shared resolve path `crates/werust-core/src/ipfs.rs` (`resolve_ipfs_request`, `mime_type_for_path`). Compare the response werust returns for the document vs what eth.limo serves over https. A black page is typically an empty/failed/mistyped document.
>
> Done = an ipfs directory site renders visibly (not black) on Android + iOS at parity with https, the cause is traced + fixed + recorded, and fail-closed still fails visibly. FIRST reproduce the black page and capture the actual mobile response (bytes + type) for the document. RECORD the diagnosis + fix durably.
