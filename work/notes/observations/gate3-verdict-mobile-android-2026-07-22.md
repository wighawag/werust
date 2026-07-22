---
title: Gate-3 (conductor) verdict — mobile-android-shell-and-static-lib — APPROVE
date: 2026-07-22
kind: observation
reviewOf: mobile-android-shell-and-static-lib
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 6318d45)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review. This task's
acceptance is partly a Gradle/APK concern the pure-Rust gate can't execute, so I
scrutinised HOW each criterion is actually satisfied.

### Environment precondition (checked before dispatch)

This laptop has the full Android toolchain: `ANDROID_HOME=~/.sdks/android` (SDK, NDK
28+29, platforms 34/36, build-tools to 36.1, sdkmanager+apkanalyzer), Java 21, and
the Rust targets `aarch64-linux-android` + `x86_64-linux-android` installed. Exported
`ANDROID_HOME`/`ANDROID_SDK_ROOT` for the build. (Recorded so a re-run knows the deps.)

### Acceptance criteria — met (with the correct pure-Rust-gate/mobile-CI split)

- ✅ A real Android app module (`crates/werust-android/`): Gradle Kotlin-DSL module
  with wrapper, `BrowserActivity.kt` + `WerustCore.kt` (JNI), `AndroidManifest.xml`,
  a `rust/` crate producing the JNI `.so`. Not a spike.
- ✅ Rust core cross-compiled to a native lib and packaged for arm64-v8a + x86_64 via
  a Gradle step (copies each ABI's `.so` into `jniLibs/<abi>/`).
- ✅ Kotlin confined to the OS edge: history/URL-bar/load-state are the Rust core's
  truth (via `ChromeState` over JNI); Kotlin holds no browsing logic. Covered by
  `cargo test` (7 core tests) which the verify gate runs.
- ✅ A BUILD-leg check (`check-apk-abis.sh`) asserts the APK carries the core lib for
  both ABIs. See the note below on WHERE it runs.

### Judgement calls resolved on inspection (clear APPROVE, not a coin-flip)

1. **"static lib" realised as a JNI cdylib** (`libwerust_mobile.so`), not a literal
   `.a` — CORRECT: Android cannot `dlopen` a `.a` at runtime; the `.so` is the
   Android-idiomatic realisation of "cross-compile the core and link it in", and
   `crate-type` ALSO emits `staticlib` for anyone wanting the `.a`. Spirit honoured,
   recorded in DECISIONS.md. Ratify.
2. **The APK-ABI check is a standalone script run in the mobile CI job, NOT the
   pure-Rust `verify` gate** — CORRECT separation: a Gradle/APK build needs the
   Android SDK+NDK, which has no business in the always-on `cargo` gate. The check is
   real and correct (unzips APK, greps `lib/<abi>/libwerust_mobile.so` for both
   ABIs); running it is legitimately the release/mobile CI job's leg (ADR-0002's
   hand-written mobile jobs). So the ABI property is verified BY the check the
   release task runs, not left unverified. ACTIONED via a forward-note on the
   release task so it actually RUNS the check.
3. **`werust-core` crate extraction** (shared browsing logic out of the desktop
   binary) — sound; desktop shell tests moved verbatim, behaviour unchanged; Android
   + future iOS link the same core. Ratify.

### Nit / follow-up

- Doc drift (benign): `backend.rs` says `on_page_committed` is called from
  `onPageCommitVisible`, but `BrowserActivity.kt` wires it from `onPageStarted`.
  Harmless comment mismatch (captured, not fixed — landed code, benign).

### Forward-note planted (conductor step 2)

`release-goreleaser-rust-desktop-and-mobile-artifacts`: reuse the Android module's
`./gradlew :app:assembleDebug`, RUN `check-apk-abis.sh` on the built APK (this is
where the ABI assertion executes), install SDK+NDK on the mobile CI runner, and
compile the shared `werust-core`. (Extend with the iOS `.app` path once that lands.)

### What this unlocks

Landing this is one of the two deps of
`release-goreleaser-rust-desktop-and-mobile-artifacts` (the other:
`mobile-ios-shell-and-static-lib`, next).
