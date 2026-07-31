---
title: "`window.localStorage` is null on Android: DOM storage was never enabled, and no capability row existed to catch it"
date: 2026-07-31
status: open
---

## What the human found

Testing `mandalas.eth` on desktop and Android: the site works on desktop, and on Android **`window.localStorage` is `null`**.

Their conformance point is exactly right and is the key to the diagnosis: the web platform requires `window.localStorage` to be **a `Storage` object**, or for accessing the property to **throw a `SecurityError`** (which is what happens on an opaque origin). **Returning `null` is neither, and is non-conformant.** That `null` is a fingerprint, and it identifies the cause precisely.

## Root cause: `domStorageEnabled` is false by default, and we never set it

`crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt` configures the `WebView` with `javaScriptEnabled`, `setSupportMultipleWindows`, `javaScriptCanOpenWindowsAutomatically` and the debug-gated `setWebContentsDebuggingEnabled`. It never sets **`settings.domStorageEnabled`**, and the Android `WebSettings` default is **`false`**. With DOM storage disabled, Android's WebView returns `null` from `window.localStorage` instead of throwing — the exact non-conformance observed.

**It is NOT the opaque-origin problem**, and the `null` is what rules that out. Android is the one platform where `ipfs://` is origin-MAPPED (`crates/werust-android/rust/src/origin_map.rs`), so an opaque origin was the obvious suspect. But an opaque origin produces a **`SecurityError` throw**, not `null`. The origin map is working; the setting is simply off.

**Why only Android.** The other four edges never touch a storage setting because they do not need to: WebKitGTK (desktop), WKWebView (iOS and macOS) and WebView2 (Windows) all have DOM storage ON by default. Android's `WebView` is configured for an APP EMBEDDING a web view, not for a browser, so its defaults are deliberately conservative. werust is a browser; several of those defaults are wrong for it.

## Enabling it is safe here, and that is worth stating rather than assuming

Storage is partitioned per ORIGIN, so "turn on localStorage" is only safe if two different sites cannot land on the same origin. On Android they cannot: `origin_map.rs` maps each CID to its OWN subdomain, `https://<cid>.ipfs.werust.invalid`. So storage is isolated per content address, exactly as it is on the platforms that serve real `ipfs://<cid>` origins. There is no cross-site leak in flipping this switch.

## Why the parity guard did not catch this

This is the ADR-0005 failure mode wearing new clothes — a user-facing capability that works on one platform and silently fails on another — and the guard that exists precisely to prevent it did not fire. It could not: **`docs/platform-capability-matrix.toml` has 24 capability rows and not one covers web storage** (no `localStorage`, `sessionStorage`, IndexedDB or cookie row).

The lesson is about the guard's ceiling, and it is worth recording: the parity guard prevents a KNOWN capability from silently shipping on one platform. It cannot discover a capability nobody thought to write a row for. Every row in that matrix is a werust-specific feature (trust indicator, ENS resolution, debug view); not one is a **web-platform** capability. So the entire question "does the web platform itself work the same on all five edges?" is currently unguarded, and DOM storage is unlikely to be the only gap.

## The deeper consequence, which is NOT Android-specific and needs a human decision

Storage is keyed by ORIGIN, and werust's origins are CONTENT-ADDRESSED (`ipfs://<cid>` on four platforms, `https://<cid>.ipfs.werust.invalid` on Android). Therefore:

**When a site publishes a new version, its CID changes, its origin changes, and every byte of its `localStorage` becomes unreachable.** A user's saved state in a dapp vanishes on every update of that dapp, on EVERY platform, not just Android. Enabling DOM storage on Android makes werust conformant; it does not make storage durable across a site update anywhere.

That is inherent to content addressing rather than a bug, and every IPFS-capable browser has it. But werust has something most do not: a first-class model of MUTABLE NAMES with trust-on-first-use pinning (`ipns-tofu-pin-and-warn-on-change`, `docs/adr/0006`, `docs/adr/0007`). So there is a real design question here that this project is unusually well-placed to answer, and it is a human's to make:

- Key storage by the CID (today's behaviour): perfectly isolated, but state is lost on every publish.
- Key storage by the stable mutable NAME (e.g. `mandalas.eth`): state survives updates, but the name's controller can then repoint the name at content that READS the previous version's storage — which is precisely the threat the TOFU work exists to surface, and would want the pin to gate it.

Recorded, not decided.
