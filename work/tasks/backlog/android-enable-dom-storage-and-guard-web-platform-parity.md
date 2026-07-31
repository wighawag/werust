---
title: "`window.localStorage` is null on Android: enable DOM storage, and give the parity matrix its first WEB-PLATFORM row so this class cannot recur"
slug: android-enable-dom-storage-and-guard-web-platform-parity
blockedBy: []
covers: []
needsAnswers: true
---

## What to build

Found by the human on real hardware, 2026-07-31, testing `mandalas.eth`: it works on desktop, and on Android `window.localStorage` is **`null`**. Full diagnosis: `work/notes/findings/android-localstorage-is-null-dom-storage-never-enabled-2026-07-31.md`.

**The one-line cause.** `BrowserActivity.kt` never sets `settings.domStorageEnabled`, and the Android `WebSettings` default is `false`. With DOM storage off, Android's WebView returns `null` from `window.localStorage` rather than throwing — which is non-conformant (the platform requires a `Storage` object, or a `SecurityError` on an opaque origin; `null` is neither).

**Do not mistake this for the origin problem, and keep that distinction in the record.** Android is the one platform where `ipfs://` is origin-MAPPED, so an opaque origin is the obvious suspect — but an opaque origin throws `SecurityError`, it does not return `null`. The origin map is fine. If you "fix" the origin map you will have broken a working thing chasing the wrong cause.

## 1. Enable DOM storage (the fix)

Set `settings.domStorageEnabled = true` in the `WebView` configuration block, with a comment in the style of its neighbours saying WHY (a browser needs it; Android's default is false because `WebView` is built for apps embedding a view, not for browsers) and noting that it is safe because `origin_map.rs` gives each CID its own subdomain, so storage stays partitioned per content address exactly as it is on the four platforms with real `ipfs://<cid>` origins.

## 2. Verify the neighbours of the same feature, and report rather than guess

`localStorage` is one of several storage APIs a dapp uses. **Measure, do not assume**, on a device or emulator, and record what you find:

- `window.localStorage` and `window.sessionStorage`: a `Storage` object, and a set/get/reload round-trip that survives.
- `window.indexedDB`: whether it works, and whether it depends on `domStorageEnabled` in the WebView (historically it has; confirm on the API levels this app supports rather than trusting a blog post). Wallets and dapps use IndexedDB heavily, so a `localStorage` fix that leaves IndexedDB broken has fixed half the problem.
- Cookies: whether `CookieManager` behaviour matches the other edges, and whether third-party cookies are off (they are, by WebView default). If they are off, that is arguably CORRECT for a privacy-focused browser — record it as a deliberate position rather than an accident.

Fix what is unambiguously right for a browser. For anything that is a UX or privacy JUDGEMENT rather than a conformance bug, do NOT change it here: write an observation and let a human decide.

## 3. Audit the other `WebSettings` defaults, and write them down WITHOUT changing them

The root cause here is general: Android's `WebView` defaults are tuned for an embedded view, and werust is a browser. Produce a short audit note listing the settings whose defaults differ from browser-correct behaviour, what each would change, and your recommendation. Candidates worth checking (not exhaustive, and each needs verifying rather than repeating): `builtInZoomControls` / `displayZoomControls` / `setSupportZoom` (pinch-to-zoom, which a mobile browser is expected to have and a default `WebView` does not), `useWideViewPort` and `loadWithOverviewMode` (how a page with no `<meta viewport>` is laid out), `mediaPlaybackRequiresUserGesture`, and text/font scaling.

**Change none of these in this task.** They are user-visible behaviour decisions and at least one (pinch-zoom) touches the chrome. The deliverable is the LIST, so a human can triage it in one pass instead of discovering each on a device months apart.

## 4. Give the parity matrix its first WEB-PLATFORM capability row

This is the durable half of the task. `docs/platform-capability-matrix.toml` has 24 rows and not one covers web storage, which is exactly why the guard that exists to stop a capability shipping on one platform did not fire here. Every existing row is a werust FEATURE (trust indicator, ENS resolution, debug view); none asks "does the web platform itself behave the same on all five edges?".

Add a `web-storage` capability row, with an explicit cell for all five platforms, honest against what you MEASURED in step 2 (`implemented` where verified; `stubbed` with a real task slug where not). Follow the macOS and Windows columns as the worked examples of the honesty standard, and read `docs/adr/0005-platform-capability-parity-guard.md` first.

In the row's description, state the ceiling this incident exposed: the matrix guards capabilities someone thought to add a row for, and until now it contained no web-platform rows at all. If you think further web-platform rows are warranted (IndexedDB, cookies, service workers — note the macOS probe measured `service_worker: reject:TypeError` on `ipfs://`), propose them as authored follow-on tasks rather than filling the matrix speculatively.

## 5. Guard it where the guard will actually run

There is no CI emulator leg — `SpaClientNavOriginTest.kt` is a hand-run on-device probe — so the regression guard that runs on every push must be a source-shape test, the pattern this repo already uses for platform code the Ubuntu gate cannot compile (`crates/werust-core/tests/debug_view_mobile_wiring_shape.rs` and friends). Assert that the Android edge enables DOM storage, so a later refactor of that settings block cannot silently return `localStorage` to `null`.

Additionally extend the instrumented `androidTest` in the `SpaClientNavOriginTest.kt` style to assert `localStorage` is a `Storage` object and round-trips, so whoever next runs on a device gets the real check. Say plainly in the record that this half does not run in CI today.

## Acceptance criteria

- [ ] `window.localStorage` on Android is a `Storage` object that round-trips, verified on a device or emulator, not argued.
- [ ] `sessionStorage` and `indexedDB` are verified too, and whatever is found is RECORDED (including if IndexedDB needs more than this switch).
- [ ] The Kotlin change carries a comment explaining why the default is wrong for a browser and why per-CID origin mapping makes it safe.
- [ ] An audit note lists the other browser-wrong `WebSettings` defaults with recommendations, and changes none of them.
- [ ] `docs/platform-capability-matrix.toml` gains a `web-storage` row with an explicit, honest cell for all five platforms, and the parity guard passes with no weakening.
- [ ] A source-shape test on the Ubuntu gate pins that Android enables DOM storage.
- [ ] An instrumented test asserts the round-trip on-device, and the record says plainly that it does not run in CI.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green, and the Android edge still builds (release dry run).

## Prompt

> Goal: on Android `window.localStorage` is `null` (found on `mandalas.eth`; desktop is fine). Cause: `BrowserActivity.kt` never sets `settings.domStorageEnabled` and Android's default is `false`, so the WebView returns `null` instead of a `Storage` object — non-conformant, and the `null` is what proves it is NOT the opaque-origin problem (that throws `SecurityError`), so do not touch `origin_map.rs`. (1) Set `domStorageEnabled = true` with a comment saying why the WebView default is wrong for a BROWSER and why it is safe here (each CID gets its own subdomain in `origin_map.rs`, so storage is partitioned per content address like the real `ipfs://<cid>` origins elsewhere). (2) MEASURE `localStorage`, `sessionStorage` and `indexedDB` on a device/emulator and record what you find — IndexedDB may need more than this switch, and a dapp fix that leaves it broken is half a fix; check cookie/third-party-cookie behaviour too and record it as a deliberate privacy position if it stays off. (3) AUDIT the other `WebSettings` browser-wrong defaults (pinch-zoom via `builtInZoomControls`, `useWideViewPort`/`loadWithOverviewMode`, media gesture, text scaling) and write the list down WITHOUT changing any of them — they are UX decisions for a human. (4) Add the matrix's FIRST web-platform row, `web-storage`, with an honest explicit cell for all five platforms; the guard missed this bug because all 24 existing rows are werust features and none asks whether the web platform behaves the same everywhere — say that in the row's description, and propose further rows (IndexedDB, cookies, service workers) as follow-on tasks rather than filling them speculatively. (5) Guard it where it runs: a source-shape test on the Ubuntu gate pinning that Android enables DOM storage (there is no CI emulator leg), plus an instrumented `androidTest` round-trip in the `SpaClientNavOriginTest.kt` style, saying plainly that half does not run in CI.
