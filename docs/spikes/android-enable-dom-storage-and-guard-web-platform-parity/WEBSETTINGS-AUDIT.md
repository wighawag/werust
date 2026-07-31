# The other browser-wrong `WebSettings` defaults: the audit, changing NOTHING

Android's `WebView` defaults are tuned for an APP EMBEDDING a web view, not for a BROWSER. That is the general root cause behind the `window.localStorage` bug (`domStorageEnabled` defaults to `false`), and DOM storage is unlikely to be the only default that is wrong for werust. This is the LIST, so a human can triage it in ONE pass instead of discovering each on a device months apart.

**Task `android-enable-dom-storage-and-guard-web-platform-parity` deliberately changed NONE of these.** Every item below is a user-visible behaviour decision, and at least one (pinch-zoom) touches the chrome. The deliverable is the list plus a recommendation; the decision is a human's. `crates/werust-core/tests/web_storage_edge_wiring_shape.rs` asserts both halves of that: every setting named here is listed, and none of them is set by `BrowserActivity.kt`. When a human decides one, update this note in the same change.

Every value below was **MEASURED off a fresh `WebView`** by `the_audited_websettings_defaults_are_measured_for_the_human_triaging_them` in `crates/werust-android/app/src/androidTest/.../WebStorageTest.kt`, not repeated from documentation. Two of them (text scaling, `displayZoomControls`) do not behave the way the docs alone would suggest. Harness: `Medium_Phone_API_36.1` emulator, Android 16 / API 36, System WebView 142.0.7444.174, 2026-07-31.

| Setting | Measured default | Browser-correct? | Recommendation |
|---|---|---|---|
| `domStorageEnabled` | `false` | no | **FIXED by this task** (`= true`) |
| `builtInZoomControls` | `false` | no | enable, with `displayZoomControls = false` |
| `displayZoomControls` | `true` | n/a until the above | set `false` if pinch-zoom is enabled |
| `supportZoom()` (set via `setSupportZoom`) | `true` | yes | leave |
| `useWideViewPort` | `false` | no | enable, paired with the next |
| `loadWithOverviewMode` | `false` | no | enable, paired with the previous |
| `mediaPlaybackRequiresUserGesture` | `true` | yes | leave |
| `textZoom` | follows the OS font scale | yes | leave, and do NOT pin it |
| `databaseEnabled` | `false` | yes | leave |

## 1. Pinch-to-zoom is OFF (`builtInZoomControls`, `displayZoomControls`, `supportZoom`)

**Measured:** `supportZoom() = true` (the setter is `setSupportZoom`), `builtInZoomControls = false`, `displayZoomControls = true`.

**What it means.** `supportZoom()` being true is necessary but not sufficient: with `builtInZoomControls = false` the `WebView` installs no zoom gesture handling at all, so **a user cannot pinch-to-zoom a page in werust today**. Every mobile browser can. It is also an accessibility affordance, not only a convenience: pinch-zoom is how a low-vision user reads a page whose author set a small font.

`displayZoomControls` only matters once the built-in controls are on: it then draws the legacy on-screen +/- overlay widget, which no modern browser shows. It is deprecated from API 33 and the platform stops drawing the widget there, so on a current device it is inert — but this app's `minSdk` is 21, so on an old device it would appear. The idiomatic browser pairing is therefore `builtInZoomControls = true` **with** `displayZoomControls = false`.

**Recommendation:** enable, as that pair. **Why it is a human's call:** it changes touch behaviour across the whole app and interacts with the chrome (a pinch that begins on the toolbar row, the interaction with `useWideViewPort` below, and whether a page's `user-scalable=no` should be honoured — browsers increasingly override it for accessibility, which is a policy werust would be choosing).

## 2. Pages are laid out at device width, with no wide viewport (`useWideViewPort`, `loadWithOverviewMode`)

**Measured:** both `false`.

**What it means.** These two are a pair and govern how a page with no (or an unusual) `<meta name="viewport">` is laid out. With `useWideViewPort = false` the `WebView` lays every page out at the view's own width, so a legacy desktop-oriented page — which expects the ~980px wide viewport a mobile browser gives it — is squeezed into a phone-width viewport and reflows or overflows instead of rendering as it does everywhere else. With `loadWithOverviewMode = false`, a page wider than the view opens zoomed IN at the top-left rather than zoomed out to fit.

This is the item with the strongest argument for changing, because it is not only cosmetic: `CONTEXT.md` makes rendering the normal server web with **full compatibility** a hard requirement, and this default makes werust lay out the legacy web differently from every other mobile browser.

**Recommendation:** enable both together (they are only coherent as a pair). **Why it is a human's call:** it changes the layout and initial zoom of EVERY page, including content-addressed sites that currently look right, and it is entangled with the pinch-zoom decision above (a zoomed-out overview page with no way to zoom in is worse than either default). Verify on a device against a real legacy page and a real dapp before landing.

## 3. Media autoplay needs a user gesture (`mediaPlaybackRequiresUserGesture`)

**Measured:** `true`.

**What it means.** The `WebView` default here is already the browser-correct one: real browsers also refuse to autoplay audible media without a user gesture. Nothing to fix.

**One honest caveat worth recording, not acting on:** Chrome permits MUTED autoplay while this flag is all-or-nothing, so a muted background/hero video that plays in Chrome will not start in werust. That is a small compatibility gap, and closing it would need a finer-grained mechanism than this flag has.

**Recommendation:** leave it. Setting it to `false` would make werust autoplay audible media, which is user-hostile and worse than the gap above.

## 4. Text scaling already follows the OS (`textZoom`)

**Measured, and this one contradicts what the API alone suggests:** `textZoom` reads `100` at system font scale `1.0` and **`130` at system font scale `1.3`**. The `WebView` picks the OS accessibility font-size setting up by itself.

**What it means.** werust is already accessibility-correct here, for free. The trap is the opposite of the other items: an app that "fixes" a layout breaking at large font sizes by pinning `textZoom = 100` would be silently **removing** a user's accessibility setting.

**Recommendation:** leave it, and do NOT pin it. If a chrome layout ever breaks at 1.3x, fix the layout.

## 5. `databaseEnabled` (the deprecated WebSQL API) is off, and should stay off

**Measured:** `false`.

Named here only because it sits directly beside `domStorageEnabled` in the same settings object, so the next reader of the storage fix will see it and wonder whether it was missed by symmetry. It was not. `databaseEnabled` gates WebSQL, which is REMOVED from the web platform, not merely deprecated; enabling it would resurrect a dead API. The storage APIs a dapp actually uses (`localStorage`, `sessionStorage`, IndexedDB) are all covered and all measured in `MEASUREMENTS.md`.

**Recommendation:** leave it off.

## Adjacent settings this audit did NOT cover

Named so the next person knows the boundary rather than assuming the list is exhaustive. These are SECURITY/PRIVACY defaults rather than the UX defaults this task was scoped to, none of them was measured here, and each deserves its own look: `setMixedContentMode`, `allowFileAccess` / `allowContentAccess` / `setAllowFileAccessFromFileURLs`, `setSafeBrowsingEnabled`, and `userAgentString` (werust currently ships the stock `WebView` UA, which both misidentifies the browser and is a fingerprinting surface). A second audit pass over those, scoped as a security review rather than a UX triage, is worth a task of its own.
