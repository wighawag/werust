# werust — Android app module

A real Android app module (not a spike) that links the werust **Rust core** cross-compiled as a native library and launches a browsing surface. This is the Android half of mobile parity with wezig (task `mobile-android-shell-and-static-lib`, spec story 18).

## What it is

- **Kotlin at the OS edge only.** `BrowserActivity` hosts a URL bar, Back/Forward/Reload/Stop buttons, and the platform `android.webkit.WebView`. It holds NO browsing logic: every decision (URL-bar text, Back/Forward availability, load status) is the Rust core's truth, read back through `WerustCore`. The Activity extends `androidx.activity.ComponentActivity` (the module's ONE androidx dependency) purely for the non-deprecated `OnBackPressedDispatcher`, which makes the SYSTEM Back button navigate page history instead of exiting the app; the view layer itself is still plain platform widgets + `WebView` + framework themes (task `android-hardware-back-button-navigates-history`, decision 2 in `docs/spikes/android-hardware-back-button-navigates-history/README.md`).
- **The Rust core behind the seams.** The browsing logic is the shared `werust-core` crate (`BrowserShell` over the `Renderer` seam), driven on Android through an `AndroidBackend` (`crates/werust-android/rust`). The core is compiled to `libwerust_mobile.so` and called from Kotlin over JNI (`WerustCore`).
- **Cross-compiled as a normal Gradle step.** The `cargoBuildRustCore` Gradle task (`app/build.gradle.kts`) runs `cargo build` per ABI with the NDK clang linker and stages each `libwerust_mobile.so` into `jniLibs`, so `./gradlew :app:assembleDebug` produces a debug APK carrying the Rust core for **arm64-v8a** and **x86_64**.
- **The APK's version comes from the release tag.** `versionCode`/`versionName` are resolved from the SAME source the Rust core uses (`WERUST_VERSION`, which the `android-apk` release job exports from the tag; else `git describe --tags --always`; else the workspace Cargo version), so the version Android's system settings show and the version the ⋮ menu reports are one string. The semver triple is folded into the monotonic integer Android sequences updates on — `major * 10000 + minor * 100 + patch`, so `v0.2.9` → `209` and `v1.0.0` → `10000`. An untagged local build keeps the old `versionCode = 1` placeholder (and a `git describe`-shaped `versionName`), so it still builds and installs. A build where `WERUST_VERSION` *is* set but does not fold to a `versionCode` (a pre-release tag such as `v0.3.0-rc1`, or a hand-set name) FAILS loudly instead of taking the placeholder: a signed release APK carrying `versionCode = 1` could never be offered as an update, and sequencing pre-release tags is deliberately not designed. Decisions: `docs/spikes/android-apk-signing/README.md` (decisions 4–8).
- **Release signing is CI-only and env-gated.** `app/build.gradle.kts` declares a `signingConfigs.release` whose keystore + credentials come ONLY from the environment (`ANDROID_KEYSTORE_PATH` / `ANDROID_KEYSTORE_PASSWORD` / `ANDROID_KEY_ALIAS` / `ANDROID_KEY_PASSWORD`), which the `android-apk` release job fills from four repository secrets. With no such environment the signing config is never created, so local dev builds are untouched and `assembleRelease` simply emits AGP's `app-release-unsigned.apk`. Setup + decisions: `docs/spikes/android-apk-signing/README.md`.

## Build

Requires the Android SDK + NDK and a Rust toolchain with the Android targets:

```sh
rustup target add aarch64-linux-android x86_64-linux-android
export ANDROID_HOME=/path/to/android-sdk        # or ANDROID_SDK_ROOT
# optional: export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/<version>
cd crates/werust-android
./gradlew :app:assembleDebug
```

The APK lands at `app/build/outputs/apk/debug/app-debug.apk`. It is signed with AGP's auto-generated DEBUG keystore, so it installs but carries no release identity; the release-signed `app-release.apk` is produced only by CI (see above).

## Installing a release-signed APK over a debug one requires an uninstall

The release APK keeps the SAME `applicationId` as the debug APK (`com.github.wighawag.werust`) but is signed with a DIFFERENT key, and Android refuses to replace an installed app with a build signed by another key. So the first time you install a release-signed `app-release.apk` on a device that already holds a locally built debug APK, the install fails until you **uninstall the existing app first**:

```sh
adb uninstall com.github.wighawag.werust   # or: long-press the icon -> Uninstall
adb install app-release.apk
```

This is a ONE-TIME transition, not a defect: once a device holds a release-signed build, every later release updates it in place (which is what the tag-derived `versionCode` above is for). Two release-signed builds never need this — only the debug-to-release crossing does.

## BUILD-leg check (the APK carries the Rust core for both ABIs)

```sh
docs/spikes/mobile-android-shell-and-static-lib/check-apk-abis.sh
```

It fails unless BOTH `lib/arm64-v8a/libwerust_mobile.so` and `lib/x86_64/libwerust_mobile.so` are present in the APK. This is a separate leg from the repo's pure-Rust `verify` gate (which cannot host an SDK+NDK build); the release/mobile CI job (`release-goreleaser-rust-desktop-and-mobile-artifacts`, `docs/adr/0002`) runs it after building the APK.

## Design decisions

See `docs/spikes/mobile-android-shell-and-static-lib/DECISIONS.md`.
