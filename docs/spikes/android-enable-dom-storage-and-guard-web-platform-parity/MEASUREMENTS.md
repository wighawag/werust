# Android web storage, MEASURED on-device: what `domStorageEnabled` actually governs

The on-device evidence behind task `android-enable-dom-storage-and-guard-web-platform-parity`. Everything below was produced by the instrumented probe `crates/werust-android/app/src/androidTest/java/com/github/wighawag/werust/WebStorageTest.kt`, run against the REAL System WebView; nothing here is inferred from documentation or a blog post. The task asked for exactly that, and it was right to: one widely-repeated claim about this setting turned out to be false on the API levels werust ships against.

**This probe does NOT run in CI.** There is no CI emulator leg in this repo (its sibling `SpaClientNavOriginTest.kt` is a hand-run probe for the same reason). The half that runs on every push is the source-shape guard `crates/werust-core/tests/web_storage_edge_wiring_shape.rs`, which pins that the Android edge still enables DOM storage so a refactor of that settings block cannot silently return `window.localStorage` to `null`.

## Harness

| | |
|---|---|
| Command | `cd crates/werust-android && ./gradlew :app:connectedDebugAndroidTest` |
| Device | `Medium_Phone_API_36.1` emulator (x86_64), Android 16 / API 36 |
| System WebView | `com.google.android.webview` 142.0.7444.174 |
| Date | 2026-07-31 |
| Origin under test | `https://bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq.ipfs.werust.invalid` — the internal per-CID origin every `ipfs://` load really runs on (`crates/werust-android/rust/src/origin_map.rs`), served through `shouldInterceptRequest` exactly as the edge serves content |
| Variable | `WebSettings.domStorageEnabled`, and nothing else |

The origin matters: it is a normal tuple origin, not an opaque one (`SpaClientNavOriginTest` already measured that), so every result below isolates the SETTING rather than the origin.

## The measurements, verbatim

Logged under the `WebStorageProbe` logcat tag by the probe; quoted here unedited.

### With `WebSettings` exactly as Android ships them (`domStorageEnabled` untouched, i.e. `false`) — THE BUG

```
window.localStorage: null
window.sessionStorage: [object Storage]
window.indexedDB: [object IDBFactory]
localStorage before this load: throw:TypeError
localStorage round-trip: throw:TypeError
sessionStorage round-trip: ok:werust-round-trip
indexedDB round-trip: ok:werust-round-trip
document.cookie round-trip: ok:werust-probe=werust-round-trip
```

### With `settings.domStorageEnabled = true` (what `BrowserActivity` now sets) — THE FIX

```
window.localStorage: [object Storage]
window.sessionStorage: [object Storage]
window.indexedDB: [object IDBFactory]
localStorage round-trip: ok:werust-round-trip
sessionStorage round-trip: ok:werust-round-trip
indexedDB round-trip: ok:werust-round-trip
document.cookie round-trip: ok:werust-probe=werust-round-trip
```

### After a RELOAD of the same origin in the same WebView

```
localStorage before this load: ok:werust-round-trip
```

What the previous load wrote is readable after the reload: the round-trip a dapp actually depends on, not just a property that stringifies nicely.

### Cookies

```
COOKIES: acceptCookie=true acceptThirdPartyCookies=false
document.cookie round-trip: ok:werust-probe=werust-round-trip
```

## What the measurements say

1. **`window.localStorage` was `null`, and enabling the setting fixes it.** `null` is neither of the two answers the web platform allows (a `Storage` object, or a `SecurityError` throw on an opaque origin), so the pre-fix behaviour was non-conformant. The field report on `mandalas.eth` is reproduced and closed.

2. **`domStorageEnabled` governs `localStorage` ONLY — this is the surprise, and the reason the task said MEASURE, do not assume.** With the setting off, `sessionStorage` is still a real `Storage` object and still round-trips. The folklore that this one switch gates "DOM storage" as a whole does not hold on WebView 142 / API 36. The finding note's diagnosis of the CAUSE is exactly right; its scope was wider than reality. The probe now pins the measured behaviour of both areas, so a future WebView that DID couple them shows up as a red test on the next hand-run rather than as another field report.

3. **IndexedDB needs nothing from this switch.** It is a working `IDBFactory` that opens a database and round-trips a record with the setting OFF as well as ON. So this is not a half fix: there is no second Android-side switch left to find, and `indexed_db_needs_nothing_from_this_switch_which_is_measured_not_assumed` pins it. (Historically IndexedDB in a `WebView` was reported to require `domStorageEnabled`; on the API levels this app supports, it does not.)

4. **First-party cookies already work; third-party cookies are OFF.** `CookieManager.acceptCookie()` is `true` and `document.cookie` round-trips, so nothing is broken. `acceptThirdPartyCookies(webView)` is `false`, the `WebView` default.

## The deliberate privacy position on third-party cookies

**Third-party cookies stay OFF, and that is a POSITION, not an oversight.** werust's thesis is a privacy-protecting browser for a post-trusted-server web (`CONTEXT.md`, `docs/adr/0001`), and third-party cookies are the canonical cross-site tracking mechanism. Every major browser is removing or partitioning them. Inheriting the `WebView` default here happens to land exactly where werust would choose to land, so the right action is to RECORD the position rather than to change anything: if a later task ever calls `CookieManager.setAcceptThirdPartyCookies(webView, true)`, it should have to argue against this paragraph first.

Two limits worth naming rather than leaving implied. This position is currently recorded for ANDROID only: whether the other four edges block third-party cookies is unmeasured, and a cookie capability row is proposed as a follow-on task (`work/tasks/backlog/matrix-web-platform-row-cookies.md`) rather than filled in speculatively here. And no cookie POLICY is expressed in shared `werust-core` code — every edge inherits its engine's default — which is the kind of "same intent, five independent implementations" drift `docs/adr/0011` exists to remove.

## What is NOT measured here, deliberately

- **The other four edges.** WebKitGTK, WKWebView (iOS and macOS) and WebView2 set no storage setting at all because their engines enable web storage by default, and the human's field report confirms desktop works. But no probe on those platforms measured `window.localStorage` the way this one did. The `web-storage` matrix row states the evidence class per cell, and closing that gap is the proposed follow-on `matrix-web-platform-rows-are-measured-on-every-edge`.
- **Service workers.** The macOS origin probe recorded `service_worker: reject:TypeError` on `ipfs://`; that belongs to its own row and its own task (`matrix-web-platform-row-service-workers`), not to a storage measurement.
- **Storage DURABILITY across a site update.** Storage is keyed by origin and werust's origins are content-addressed, so a new CID means a new origin and a dapp's saved state becomes unreachable on every publish — on EVERY platform, not just Android. That is inherent to content addressing, it is NOT a bug this fix leaves behind, and it is a design question for a human (key by CID, as today, versus key by the stable mutable NAME with the TOFU pin gating it). Recorded in the finding note `work/notes/findings/android-localstorage-is-null-dom-storage-never-enabled-2026-07-31.md`; untouched here.
