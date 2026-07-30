# werust — Android app module

A real Android app module (not a spike) that links the werust **Rust core** cross-compiled as a native library and launches a browsing surface. This is the Android half of mobile parity with wezig (task `mobile-android-shell-and-static-lib`, spec story 18).

## What it is

- **Kotlin at the OS edge only.** `BrowserActivity` hosts a URL bar, Back/Forward/Reload/Stop buttons, and the platform `android.webkit.WebView`. It holds NO browsing logic: every decision (URL-bar text, Back/Forward availability, load status) is the Rust core's truth, read back through `WerustCore`. The Activity extends `androidx.activity.ComponentActivity` (the module's ONE androidx dependency) purely for the non-deprecated `OnBackPressedDispatcher`, which makes the SYSTEM Back button navigate page history instead of exiting the app; the view layer itself is still plain platform widgets + `WebView` + framework themes (task `android-hardware-back-button-navigates-history`, decision 2 in `docs/spikes/android-hardware-back-button-navigates-history/README.md`).
- **The Rust core behind the seams.** The browsing logic is the shared `werust-core` crate (`BrowserShell` over the `Renderer` seam), driven on Android through an `AndroidBackend` (`crates/werust-android/rust`). The core is compiled to `libwerust_mobile.so` and called from Kotlin over JNI (`WerustCore`).
- **Cross-compiled as a normal Gradle step.** The `cargoBuildRustCore` Gradle task (`app/build.gradle.kts`) runs `cargo build` per ABI with the NDK clang linker and stages each `libwerust_mobile.so` into `jniLibs`, so `./gradlew :app:assembleDebug` produces a debug APK carrying the Rust core for **arm64-v8a** and **x86_64**.
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

## BUILD-leg check (the APK carries the Rust core for both ABIs)

```sh
docs/spikes/mobile-android-shell-and-static-lib/check-apk-abis.sh
```

It fails unless BOTH `lib/arm64-v8a/libwerust_mobile.so` and `lib/x86_64/libwerust_mobile.so` are present in the APK. This is a separate leg from the repo's pure-Rust `verify` gate (which cannot host an SDK+NDK build); the release/mobile CI job (`release-goreleaser-rust-desktop-and-mobile-artifacts`, `docs/adr/0002`) runs it after building the APK.

## Design decisions

See `docs/spikes/mobile-android-shell-and-static-lib/DECISIONS.md`.
