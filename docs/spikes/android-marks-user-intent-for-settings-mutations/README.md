# Android supplies the user-intent signal for settings mutations: what is proved, and what is not

Task `android-marks-user-intent-for-settings-mutations`, spec `settings-mutations-require-user-intent`. The RULE is `docs/adr/0013` (it landed with the core + GTK task); the judgement calls this edge made are `DECISIONS.md` beside this file, and the on-device readings are `MEASUREMENTS.md`.

## What landed

- `werust_android_core::install_settings_page` registers the `werust` scheme against the GATED core entry point (`retrieval::apply_settings_request_with_intent`) and returns an `IntentMarker`: this edge's clone of the ONE `werust_core::intent::NavigationIntent` carrier, bundled with the redirect sink that answers the main-frame half.
- The session hands that one carrier BOTH ways: to its `BrowserShell` (`with_navigation_intent`, so a change typed into the URL bar marks through the chrome-only front door) and to the Kotlin navigation hook (so werust's own settings form marks).
- `IntentMarker::note_page_navigation(target, document, main_frame, user_gesture, redirect)` is the Android member of the per-edge marking rule ADR-0013 names. It marks only when ALL of them hold, and the load-bearing one is `document`: web content can never BE at a `werust://` URL.
- `SyncSession::note_page_navigation` marks OFF the session lock (the clone-handle path the page-signal callbacks take), because Kotlin's hook runs on the UI thread and must never wait behind an in-flight `ipfs://` retrieval — the ANR guard.
- Kotlin: `CoreWebViewClient.shouldOverrideUrlLoading` REPORTS the facts (`request.url`, `WebView.url`, `isForMainFrame`, `hasGesture()`, `isRedirect`) and returns `false` — read-only observation, so the WebView navigates exactly as it did before the hook existed. `onCreateWindow` (the `_blank`/`window.open` router, `docs/adr/0010`) reports NOTHING.
- The `android` cell of the `settings-mutation-user-intent` row in `docs/platform-capability-matrix.toml` flips `stubbed` -> `implemented`, naming what proves it.

## What is PROVED, and by what

Everything below runs on the plain Linux `verify` gate (`cargo fmt --check && cargo clippy --all-targets -D warnings && cargo build && cargo test`). The Android Rust edge is a normal workspace crate, so its unit tests are gate-run; only the JNI export module is `cfg(target_os = "android")`.

| Claim | Where |
|---|---|
| A submission from werust's own settings page applies and PERSISTS (persistence into the scratch settings directory the test isolates; on a real Android device there is no settings directory at all, a pre-existing gap named at the end of `MEASUREMENTS.md`) | `werust_mobile::tests::a_form_submission_inside_werusts_own_settings_page_applies_the_change` |
| A change typed in the URL bar applies through the carrier this edge hands its handler (the wiring, not just the rule), and the mark is single-use | `…::a_url_bar_commit_marks_through_the_carrier_this_edge_hands_its_handler` |
| A page-started navigation to a mutating settings URL changes nothing, byte-for-byte, and still renders the page read-only with real values | `…::a_page_started_navigation_to_a_mutating_settings_url_changes_nothing` |
| A sub-resource request changes nothing, even while the user is ON the settings page (the query-stripped frame-key case) | `…::a_sub_resource_request_for_a_mutating_settings_url_changes_nothing` |
| `window.open('werust://settings?…')` cannot mutate: the in-place route marks nothing | `…::the_blank_and_window_open_route_cannot_mutate_because_it_marks_nothing` |
| Every fact the mark requires is load-bearing (each one flipped alone withholds the mark) | `…::every_fact_the_mark_requires_is_load_bearing` |
| Marking never takes the session lock (the ANR guard) | `…::the_marking_path_never_waits_on_the_session_lock_so_the_ui_thread_cannot_anr` |
| The tests never touch the user's own `retrieval.json` | `…::the_intent_signal_never_touches_the_users_own_settings_file` |
| The Android handler is the GATED entry point; the one carrier reaches both handler and shell; the rule requires activation + a `werust://` source document; the Kotlin hook reports facts and decides nothing; `onCreateWindow` marks nothing; neither Android file reads a mark | `crates/werust-core/tests/settings_user_intent_edge_wiring_shape.rs` (the appended Android block) |

On top of that gate-side table there is REAL on-device evidence, hand-run rather than CI-run: the instrumented probe (5 cases, including the spelling round-trip between the two hooks) and a hand-driven walk-through of the shipping debug APK, both recorded in `MEASUREMENTS.md`. The section below says exactly what that evidence is and is not.

The shape guard's teeth were re-checked by mutation: pointing the Android handler at the ungated `apply_settings_request` reds `android_serves_the_settings_page_through_the_gated_core_entry_point`; dropping the redirect exclusion reds `android_marks_only_a_navigation_activated_inside_a_surface_werust_drew`; dropping `.with_navigation_intent` reds `android_hands_the_one_carrier_to_both_the_handler_and_the_shell`; adding a marking call to the `_blank` transport client reds `android_never_marks_the_blank_and_window_open_path`; wrapping the report in a Kotlin `if` reds `the_android_kotlin_shell_reports_the_facts_and_decides_nothing`. The Kotlin block extractor has its own guard (`the_kotlin_block_extractor_stops_at_the_matching_brace`), because the negative assertions are exactly the kind that pass vacuously on a mis-bounded slice.

## HONEST LIMIT: there is no Android CI leg, so the on-device half is a HAND-RUN (and it WAS run)

`.github/workflows/android-instrumented.yml` does not exist (task `android-instrumented-emulator-ci-leg` is still in `work/tasks/backlog/`), so **no CI evidence is claimed here**, and nothing above or below is protected by a runner. What runs on every push is the gate-side table above.

The on-device half is a HAND-RUN probe, `crates/werust-android/app/src/androidTest/java/com/github/wighawag/werust/SettingsIntentSignalTest.kt`, in the style of its unrun-in-CI siblings `WebStorageTest.kt` / `SpaClientNavOriginTest.kt`:

```
cd crates/werust-android && ./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=com.github.wighawag.werust.SettingsIntentSignalTest
```

**It was run on 2026-08-18** against the `Medium_Phone_API_36.1` emulator (Android 16 / API 36, System WebView 142.0.7444.174): 5 tests, all green. The readings are quoted verbatim in `MEASUREMENTS.md`, which also records a hand-driven walk-through of the SHIPPING debug APK through the steps below. The two things only a running System WebView could settle are both settled there:

1. `WebResourceRequest.hasGesture()` IS `true` for a REAL TAP on werust's own settings form (Android documents that it "may return false even though the sequence of events … was initiated by a user gesture"), and `false` for a script's `location = …`. So decision 1's escape hatch is not taken.
2. The URI the WebView hands `shouldInterceptRequest` is BYTE-FOR-BYTE the URI this hook marked, so the mark and the request reduce to the same `intent_key` (the collapses that key performs are not even exercised on this edge; the query is compared verbatim and comes back verbatim).

Had either come back the other way the failure would still have been FAIL-CLOSED and legible: the settings page renders and says `Not changed: werust did not start this change, so nothing was applied (a page cannot change your settings).`, i.e. the user's own change refused and nothing a page does applied. Decision 1 in `DECISIONS.md` records what to change if a future WebView regresses reading 1.

A hand-run is a snapshot of ONE engine version on ONE device, not a standing guarantee: only the CI leg would make it one. Re-run the command above (and the walk-through) when the Kotlin hook, the JNI signature or the marking rule changes.

### Manual verification steps (a human with a device or emulator)

All of these were walked on 2026-08-18; the observed answers are in `MEASUREMENTS.md`. The wording differs slightly from what this list first predicted, because on Android nothing PERSISTS (see the limit note at the end of `MEASUREMENTS.md`): an applied change reads `Selected \`<kind>\` for this session (could not persist: no settings directory)` rather than `Saved: …`.

1. `cd crates/werust-android && ./gradlew :app:installDebug` and open werust.
2. Type `werust://settings` in the URL bar. The page renders and shows the active backend. (Reads are ungated.)
3. Type a custom gateway URL into the custom-backend field and tap `use this`. Expect the change APPLIED and the custom endpoint active. (The settings page's own form IS marked: this is the path only this task supplies.)
4. Tap `use this` on the default-gateway option (a link, not a form). Expect the change applied.
5. Type `werust://settings?backend=custom&url=http://127.0.0.1:7777` straight into the URL bar. Expect the change applied. (The chrome's front door marks.)
6. Reload the page from step 5. Expect `Not changed: werust did not start this change …` and no second application (marks are single-use, Decision 2 of the core task). A Back/Forward onto such a URL reads the same way, for the same reason.
7. Load any page containing `<img src="werust://settings?backend=custom&url=http://attacker.example/">`, then re-open `werust://settings` and confirm the active backend is unchanged. (The attack.) Tapping an in-page LINK on that page to the same URL renders the settings page read-only with `Not changed: …` and the real current values.
8. From that page, run `window.open('werust://settings?backend=custom&url=http://attacker.example/')`. The settings page loads in place (`docs/adr/0010`) showing REAL values and `Not changed: …`.

## What this closes, and what it does not

It closes the Android quarter of the residual window the core task recorded: this edge's settings page previously RENDERED but refused every change, its own form AND a URL-bar-typed one, because no carrier reached its handler. Both work again, and nothing a page controls does.

It does not touch the three remaining edges (`macos-`, `windows-`, `ios-marks-user-intent-for-settings-mutations`), which still fail closed in the same way, nor anything about WHAT the settings are (`retrieval-default-egress-before-final-release` owns the default).
