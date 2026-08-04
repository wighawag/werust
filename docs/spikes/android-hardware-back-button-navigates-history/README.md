# Android system Back navigates page history: what it does, the decisions, how to verify

The Android hardware/system Back button (and the OS back gesture) now goes BACK ONE PAGE in werust's session history when there IS back history, and only exits the app when there is none. Before this it always exited: only the on-screen `◀` button drove `core.goBack()`, nothing handled system Back, so it fell through to the platform default and finished the Activity mid-history. Root cause: field finding v0.2.5 (human, mobile/Android) — "the android back button do not navigate back in history like it should". Task `android-hardware-back-button-navigates-history`; capability row `system-back-navigates-history` in `docs/platform-capability-matrix.toml`.

> Update, 2026-08-04 (task `android-chrome-collapse-reload-stop-and-drop-history-buttons`): the on-screen `◀` and `▶` buttons have since been REMOVED from the Android toolbar, precisely because this wiring exists — a toolbar Back duplicating the platform's own is not worth phone toolbar width, and forward has no Android gesture (spec `chrome-conventional-controls`, stories 11/12). System Back is therefore now the ONLY back affordance on Android, so everything below is load-bearing on its own rather than as the second view of a button. The passages that describe the lockstep between the two affordances are kept as the historical record of what was verified; where they say "alongside `backButton.isEnabled`" or "the same expression the on-screen `◀` uses", the lockstep is now with the core FACT (`chrome.canGoBack`, still set in the same place in `refreshChrome`) and with the other `driveCore { … }` dispatches (the URL bar's Go). The device steps below still apply, minus the step-3 comparison against the on-screen button.

## Wiring

`crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt`:

- `systemBackCallback` — an AndroidX `OnBackPressedCallback(false)` (starts disabled: no history at launch), registered in `onCreate` via `onBackPressedDispatcher.addCallback(this, systemBackCallback)`.
- `handleOnBackPressed` runs `driveCore { core.goBack() }` — the SAME expression the on-screen `◀` button uses, so the core action runs on the `coreExecutor` background thread and the WebView/widget updates are posted back to the UI thread. A blocking `.eth`/IPNS resolve triggered by a history navigation therefore never blocks the main thread, and the ANR fix (task `android-anr-main-thread-diagnose-and-unblock`, `docs/spikes/android-anr-main-thread-diagnose-and-unblock/DIAGNOSIS.md`) is not regressed by this second Back entry point.
- `refreshChrome()` sets `systemBackCallback.isEnabled = chrome.canGoBack` immediately after `backButton.isEnabled = chrome.canGoBack` — the SAME core fact, in the SAME place, so the two Back affordances can never disagree. When it is false the dispatcher falls through to the platform default and Back exits the app, as a normal browser does at the start of history.

## Decisions

### 1. Predictive back (Android 13+) is NOT opted into yet

**Chosen:** do NOT set `android:enableOnBackInvokedCallback="true"` in `AndroidManifest.xml` for now. The app handles system Back purely through the AndroidX `OnBackPressedDispatcher`.

**Why:** opting in is a USER-VISIBLE, app-wide behaviour switch (it changes back dispatch to the platform `OnBackInvokedDispatcher` on Android 13+ and turns on the predictive back-gesture animation/preview), and it is orthogonal to the bug this task fixes. `OnBackPressedDispatcher` is the one implementation that works across every version this app supports (`minSdk 21`) and it BRIDGES to the platform API automatically when the manifest flag is set — so opting in later is a one-line manifest change with no code churn. Opting in now would also mean the animated preview shows the *system* back animation while werust's Back is doing an in-page history navigation, which is worth a look on a real device before it ships; the flag is also a whole-app switch (every dialog/fragment/future Activity inherits it), so it belongs to a deliberate predictive-back task, not to a history-navigation fix.

**Alternatives considered:** (a) opt in now, accepting an untested animation on a device the author has not exercised for this case; (b) the deprecated `onBackPressed()` override — rejected outright, the task and the API guidance both name the dispatcher; (c) raw `KEYCODE_BACK` interception — rejected, it is the pre-dispatcher route and does not participate in the platform's back-callback ordering at all.

**What it touches:** `crates/werust-android/app/src/main/AndroidManifest.xml` (the flag would go on `<application>`) and the visual behaviour of every future back-handling Activity/dialog. Nothing about the Back ACTION changes either way; only the dispatch route and the animation.

### 2. The app module now takes ONE androidx dependency (`androidx.activity`)

**Chosen:** add `implementation("androidx.activity:activity:1.9.3")` and make `BrowserActivity` extend `androidx.activity.ComponentActivity` instead of the framework `android.app.Activity`.

**Why:** `onBackPressedDispatcher` lives on `ComponentActivity`. The framework `Activity` offers only the DEPRECATED `onBackPressed()` override, and the platform `OnBackInvokedDispatcher` exists only on Android 13+ while this app's `minSdk` is 21 — so with a framework-only Activity there is NO non-deprecated way to satisfy the acceptance criteria across the supported version range.

**What it touches / what it reverses:** the app module's build file previously carried an explicit "No androidx dependency" stance (also referenced by `res/values-night/themes.xml` and by the provider-injection decision in `docs/spikes/mobile-provider-injection-and-trust-indicator/decisions.md`, which chose `onPageStarted` injection over `WebViewCompat.addDocumentStartJavaScript` specifically to avoid adding androidx). That stance is now narrowed, not abandoned: the OS edge still uses the plain platform `WebView`, plain framework widgets, and framework themes; `androidx.activity` is taken ONLY for the back dispatcher. Note the consequence for a future task: the "avoid an androidx dependency" argument in the provider-injection decision no longer applies as-is — `WebViewCompat.addDocumentStartJavaScript` (in `androidx.webkit`, still a separate artifact) can be reconsidered on its own merits if the document-start race is ever observed on-device.

**Alternatives considered:** keep zero androidx and use `onBackPressed()` — rejected, deprecated and explicitly excluded by the acceptance criteria. Keep zero androidx and gate on `Build.VERSION` to use the platform `OnBackInvokedDispatcher` on 13+ only — rejected: it leaves everything below Android 13 (down to `minSdk 21`) still exiting the app, i.e. the bug unfixed for most of the supported range, and it hand-rolls the exact version bridging the dispatcher already does.

### 3. Desktop and iOS are `n-a` for this capability, not stubs

**Chosen:** in `docs/platform-capability-matrix.toml`, the new `system-back-navigates-history` row is `implemented` on Android and `n-a` (with reasons) on desktop and iOS.

**Why:** the capability is defined as "the OS-provided SYSTEM Back affordance navigates history instead of leaving the app". Only Android HAS such an affordance: an OS Back button/gesture delivered to the app which, unhandled, finishes the Activity. A GTK desktop window has no OS Back button (desktop Back is the on-screen control, already covered by the `back-forward` row), and iOS has no hardware Back button either. Marking those cells `stubbed` would demand a follow-on task to implement something that does not exist on those platforms, which is exactly the noise `n-a` (with a required reason) is for.

**What it touches:** the parity guard, which forces a cell per platform for every row. iOS's nearest analogue — the WKWebView edge-swipe back GESTURE (`allowsBackForwardNavigationGestures`, currently not enabled) — is a DIFFERENT affordance with its own enablement and its own trust/lifecycle questions, so it is captured as a separate signal in `work/notes/observations/ios-edge-swipe-back-gesture-not-enabled-2026-07-26.md` rather than folded into this row and silently half-answered.

## What is automatable vs runtime-only

The FACT the edge reads is the core's, and is pinned headlessly:

- `crates/werust-android/rust/tests/system_back_wiring_shape.rs` — `the_system_back_affordance_is_enabled_exactly_when_the_core_can_go_back` drives a real `CoreSession` and asserts `can_go_back` is false at the start, false with one entry, true with two, and false again after walking back to the first entry (i.e. exactly when system Back must navigate vs fall through to exit). Network-isolated.
- `crates/werust-android/rust/src/lib.rs` — `back_and_forward_reflect_navigation_state_through_the_core` (the pre-existing core-truth test for the same fact).

The EDGE WIRING (a live Android `Activity` + the dispatcher) is runtime-only: it cannot run inside this repo's pure-Rust `verify` gate (`cargo fmt && clippy && build && test`, no Android SDK), and the Gradle/Kotlin build is not in the gate either, so the SOURCE-SHAPE half of the same test file is the ONLY gate-side cover for it. It parses `BrowserActivity.kt` and asserts: the AndroidX callback is registered on `onBackPressedDispatcher` and starts disabled; `systemBackCallback.isEnabled = chrome.canGoBack` sits in `refreshChrome` alongside `backButton.isEnabled = chrome.canGoBack`; `handleOnBackPressed` drives `driveCore { core.goBack() }` and makes no other core call (i.e. never an inline UI-thread one); and neither `onBackPressed()` nor `KEYCODE_BACK` is used. Same spirit as the config-shape guard `crates/werust-core/tests/release_plumbing_shape.rs`.

The handler assertion is bounded by BRACE MATCHING (`kotlin_block_body`), so it reads `handleOnBackPressed`'s own body and nothing else. That matters: the first version of this guard ended a body at the next member declaration chosen by KIND rather than position, so the extracted "handler" ran on through all of `onCreate` and the assertion was satisfied by the ON-SCREEN button's `compactNavButton("◀") { driveCore { core.goBack() } }` line, so it stayed green with an EMPTY handler (a vacuous guard, caught in review). The fixed guard was re-checked by mutation: emptying the handler, swapping the dispatch for an inline `core.goBack()`, adding an inline core call beside the dispatch, and removing or relocating the `systemBackCallback.isEnabled` line each turn the corresponding assertion RED. `the_block_extractor_stops_at_the_matching_brace` pins the extractor itself on a fixture shaped like that original trap, so the guard cannot regress to vacuous unnoticed.

The runtime BEHAVIOUR that no gate can assert was verified by hand on an emulator; the executed run is recorded below.

## Device verification (on a device or emulator)

Build/install the debug APK: `cd crates/werust-android && ANDROID_HOME=<sdk> ./gradlew :app:assembleDebug`, then `adb install -r app/build/outputs/apk/debug/app-debug.apk`.

### Run of record: EXECUTED and PASSED, 2026-07-26

Steps 1-4 and 6 were actually run on an emulator (AVD `Medium_Phone_API_36.1`, Android API 36.1 x86_64, headless `-no-window`, gesture navigation active — `settings get secure navigation_mode` = `2`) against the debug APK built from this branch. The oracle for "exited vs stayed" was `adb shell dumpsys activity activities | grep ResumedActivity`, and the page/URL-bar/button state was read from `adb shell screencap`. Results:

- Step 1 (one entry, on-screen `◀` greyed): `KEYCODE_BACK` -> resumed activity became `com.google.android.apps.nexuslauncher/.NexusLauncherActivity`, i.e. the app EXITED via the platform default. PASS.
- Step 2 (two entries after following the `example.com` -> `iana.org` link, on-screen `◀` enabled): `KEYCODE_BACK` -> resumed activity was STILL `com.github.wighawag.werust/.BrowserActivity` and the URL bar went back to `https://example.com/` with the previous page rendered. The app did NOT exit; it navigated. PASS.
- Step 3 (lockstep): after that Back, `◀` was greyed and `▶` had become enabled — the on-screen affordance and the system Back agreed at every observed point. PASS.
- Step 4 (walk back to the start, then exit): one further `KEYCODE_BACK` from the first entry -> launcher resumed, i.e. exit. PASS.
- Step 6 (gesture navigation): the same two-entry setup driven with the left-edge swipe (`input swipe 5 1200 700 1200`) instead of the key event -> the app stayed resumed and the URL bar returned to `https://example.com/`. The dispatcher handles the gesture identically. PASS.
- No ANR/crash: `adb logcat -d -b crash,main` showed no `ANR`, no `FATAL`, and no "Not responding" for the app across the run.

Step 5 (no ANR on a slow `.eth` history navigation) was NOT run in this pass: it needs live ENS/IPNS egress from the emulator, so it stays a manual step for a networked device. The ANR-safety of this path is otherwise covered structurally — `handleOnBackPressed` uses the very same `driveCore { core.goBack() }` expression as the on-screen button, asserted by `the_system_back_drives_the_core_off_the_ui_thread_like_the_on_screen_button`.

### The steps

1. **Back exits at the start of history.** Launch the app (it opens the start URL, one entry). Press the system Back button (or perform the OS back gesture). EXPECT: the app EXITS (the platform default) — the on-screen `◀` is greyed out at this point, and system Back agrees with it.
2. **Back navigates history.** Navigate somewhere (type a URL and press Go), then follow a link so there are at least two entries. Press system Back. EXPECT: the WebView goes BACK ONE PAGE — exactly what tapping the on-screen `◀` does — and the URL bar + trust indicator update to the previous page. The app does NOT exit.
3. **Lockstep with the on-screen button.** At each step compare: whenever the on-screen `◀` is ENABLED, system Back navigates; whenever it is greyed out, system Back exits. They must never disagree.
4. **Walk back to the start, then exit.** Press system Back repeatedly until you are at the first page (the on-screen `◀` greys out). Press system Back once more. EXPECT: the app exits, as a normal browser does.
5. **No ANR on a slow history navigation.** Navigate to a bare `.eth` name (e.g. `ronan.eth`, a blocking ENS/IPNS resolve), then to another page, then press system Back to return to the `.eth` page. EXPECT: the UI stays responsive while the resolve runs (the URL bar is still typeable, no "werust isn't responding" dialog) — the core call is on the background executor, not the UI thread.
6. **Gesture navigation too.** On a device using gesture navigation (no Back button), repeat steps 2 and 4 with the edge-swipe back gesture. EXPECT: identical behaviour — the dispatcher handles both.
