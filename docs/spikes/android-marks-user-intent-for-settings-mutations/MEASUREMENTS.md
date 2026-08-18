# Android intent signal, MEASURED on-device: what the System WebView actually reports

The on-device half of task `android-marks-user-intent-for-settings-mutations`. Everything below was produced against a REAL System WebView, by the instrumented probe `crates/werust-android/app/src/androidTest/java/com/github/wighawag/werust/SettingsIntentSignalTest.kt` and by a hand-driven walk-through of the SHIPPING debug APK; nothing here is inferred from documentation. The gate-side evidence (which stands on its own) is listed in `README.md` beside this file, and the judgement calls these readings confirm are `DECISIONS.md`.

**This probe does NOT run in CI.** There is no Android CI leg in this repo (`.github/workflows/android-instrumented.yml` does not exist; the leg is task `android-instrumented-emulator-ci-leg`), so its siblings `WebStorageTest.kt` / `SpaClientNavOriginTest.kt` are hand-run for the same reason. **No CI evidence is claimed for this task.**

## Harness

| | |
|---|---|
| Command | `cd crates/werust-android && ./gradlew :app:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=com.github.wighawag.werust.SettingsIntentSignalTest` |
| Result | `Starting 5 tests on Medium_Phone_API_36.1(AVD) - 16` / `Finished 5 tests` / `BUILD SUCCESSFUL` (all green) |
| Device | `Medium_Phone_API_36.1` emulator (x86_64), Android 16 / API 36 |
| System WebView | `com.google.android.webview` 142.0.7444.174 (`dumpsys webviewupdate`) |
| Date | 2026-08-18 |
| Evidence | the `SettingsIntentProbe` logcat tag, quoted verbatim below, plus the screen-by-screen walk-through in the last section |

## The readings, verbatim

```
SettingsIntentProbe: HOSTILE page link: Navigation(url=werust://settings?backend=custom&url=http%3A%2F%2F127.0.0.1%3A8080, document=https://hostile.example/, mainFrame=true, gesture=false, redirect=false)
SettingsIntentProbe: SCRIPT location=: Navigation(url=werust://settings?backend=custom&url=http%3A%2F%2F127.0.0.1%3A8080, document=werust://settings, mainFrame=true, gesture=false, redirect=false)
SettingsIntentProbe: TAP on werust's own settings form: Navigation(url=werust://settings?backend=custom&url=http%3A%2F%2F127.0.0.1%3A8080, document=werust://settings, mainFrame=true, gesture=true, redirect=false)
SettingsIntentProbe: window.open: created=true navigations=[]
SettingsIntentProbe: SPELLING marked=werust://settings?backend=custom&url=http%3A%2F%2F127.0.0.1%3A8080 served=werust://settings?backend=custom&url=http%3A%2F%2F127.0.0.1%3A8080
```

As a table, with the verdict the shared rule (`IntentMarker::note_page_navigation`, reached over the same JNI export the app uses) returned for each:

| Navigation shape | `isForMainFrame` | `hasGesture()` | `isRedirect` | `WebView.getUrl()` | core marks? |
|---|---|---|---|---|---|
| A REAL TAP on werust's own settings form | `true` | **`true`** | `false` | `werust://settings` | **YES** (required) |
| A script's `location = 'werust://settings?…'` on werust's own page | `true` | `false` | `false` | `werust://settings` | no |
| A link click on a plain web page pointing at the same URL | `true` | `false` | `false` | `https://hostile.example/` | no (and still no with the gesture forced `true`) |
| `window.open('werust://settings?…')` | *(arrives at `onCreateWindow`; the marking hook is never called, `navigations=[]`)* | | | | no |

## What the measurements settle

1. **`hasGesture()` is `true` for a REAL TAP on werust's own settings form.** This is the reading the whole edge was waiting on: Android documents that the bit "may return false even though the sequence of events … was initiated by a user gesture", and it is werust's ONLY Android spelling of "the user activated this in the page" (`DECISIONS.md`, decision 1). It is `true` here, and it is `false` for a script's `location = …` on the same page, so the input separates exactly the two shapes it was chosen to separate. Decision 1's escape hatch (drop the gesture input and lean on the source-document fact, as GTK does) is therefore NOT taken.

2. **`WebView.getUrl()` really is werust's own page for the form submit** (`werust://settings`), and the hostile page's own URL for a navigation started in web content. That is the load-bearing, unforgeable fact the rule rests on, and it reads correctly on this engine.

3. **The URI the scheme handler is asked for is BYTE-FOR-BYTE the URI the navigation hook reported.** The `SPELLING` line is the second question no unit test could answer: the core compares a mark against the URL the handler is later asked for (`intent_key` = the frame key PLUS the query, and the query is compared verbatim), so a re-spelling by the engine would refuse every settings change made from werust's own form. Blink hands back exactly what it reported, with no added authority or trailing slash, so the collapses `intent_key` performs are not even needed on this edge.

4. **`window.open` never reaches the marking hook at all.** It arrives at `WebChromeClient.onCreateWindow` (`created=true`) and the probe recorded ZERO navigations for it, which is the structural version of "the router is not a trust bypass" (`docs/adr/0010`).

## End-to-end on the SHIPPING app (hand-driven, same emulator)

The probe drives a raw `WebView` over canned bytes. To close the gap between "the facts read correctly" and "the browser behaves", the debug APK itself (`./gradlew :app:assembleDebug` + `adb install`) was driven by hand through the walk-through in `README.md`, with the real settings page and a hostile page served from the host over `http://10.0.2.2:8099/`:

| Step | What was done | What werust did |
|---|---|---|
| 2 | typed `werust://settings` in the URL bar | page rendered, `Active backend: default-gateway — endpoint: https://dweb.link` (reads are ungated) |
| 3 | typed `http://127.0.0.1:9999` in the custom field and TAPPED `use this` (the page's own GET form, the path only this task supplies) | `Selected \`custom\` for this session` and `Active backend: custom — endpoint: http://127.0.0.1:9999` |
| 4 | tapped the `use this` LINK of the default-gateway option | `Selected \`default-gateway\` for this session` |
| 5 | typed `werust://settings?backend=custom&url=http://127.0.0.1:7777` into the URL bar | `Active backend: custom — endpoint: http://127.0.0.1:7777` (the chrome's front door marks) |
| 6 | tapped Reload on that same URL | `Not changed: werust did not start this change, so nothing was applied (a page cannot change your settings)`, real values still shown; the single-use mark, decision 2 of the core task, seen on-device |
| 7 | loaded a hostile page carrying `<img src="werust://settings?backend=custom&url=http://attacker.example/">` plus a `fetch()` of the same URL | nothing changed; the settings page still reports `default-gateway` |
| 7b | TAPPED an in-page link on that hostile page pointing at the same mutating URL (a real gesture, main frame) | settings page rendered READ-ONLY with `Not changed: …` and `Active backend: default-gateway`; `attacker.example` never became active |
| 8 | tapped a button running `window.open('werust://settings?backend=custom&url=http://attacker.example/')` | the target was routed into the same WebView (`docs/adr/0010`) and rendered READ-ONLY with `Not changed: …`; nothing applied |

## A pre-existing limit these runs re-confirmed (NOT this task's, and NOT introduced by it)

Every applied change above says `… for this session (could not persist: no settings directory)`. On Android `werust_core::retrieval::settings_dir()` resolves from `WERUST_SETTINGS_DIR` / `XDG_CONFIG_HOME` / `HOME`, none of which exists in an Android app process, and no mobile edge sets the lever from its sandbox path, so a retrieval-backend choice has never persisted on this edge. That is recorded, open, and out of scope here: `work/notes/observations/retrieval-backend-setting-cannot-take-effect-on-mobile-2026-07-28.md`. This task changes nothing about it: a marked change applies exactly as far as it applied before the gate existed, and a refused one applies no further.
