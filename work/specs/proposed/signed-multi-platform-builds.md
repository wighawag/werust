---
title: "Signed multi-platform builds: Android (signed APK), macOS (native .app + notarized), Windows (x86_64 installer) — gated behind a CI-runner expansion"
slug: signed-multi-platform-builds
spec: signed-multi-platform-builds
needsAnswers: true
humanOnly: true
---

## Problem

werust v0.2.9 ships three artifacts: a desktop Linux x86_64 tarball (unsigned), an Android debug APK (unsigned), and an iOS Simulator `.app` zip (unsigned). There are no builds for:

- **Android**: a signed, shippable APK/AAB that can be sideloaded without developer mode or installed through an app store.
- **macOS desktop**: a native `.app` bundle the user can download and open (signed and notarized for Gatekeeper).
- **Windows**: an x86_64 binary or installer for the majority of desktop users.
- All three need SIGNING to avoid OS-level security blocks (Android's "Install blocked/Unknown sources", macOS's "cannot be opened because the developer cannot be verified", Windows SmartScreen).

## What each platform needs

### Android (APK signing)

- A release keystore (`.jks` / `.keystore`) generated and stored as a GitHub Encrypted Secret (base64-encoded). The keystore password, key alias, and key password are THREE separate secrets.
- An `android-apk-release` CI job (modelled on the existing `android-apk` debug job) that:
  1. Decodes the base64 keystore from the secret.
  2. Signs the unsigned APK with `jarsigner` or the Android `signingConfig`.
  3. Zipaligns the result.
  4. Attaches the signed APK to the GitHub Release (replacing or alongside the debug APK — decide: keep the debug APK for development alongside the signed release one, or replace it; recommended: keep both, suffix the signed one as `app-release.apk`).
- A Gradle signing configuration (`signingConfigs.release`) gated behind the keystore env being present, so local dev builds are unaffected.
- Costs: no new runner — runs on the existing `ubuntu-latest` NDK job.

### macOS desktop (native .app + notarization)

- A macOS (`macos-14`) runner job (the existing `ios-simulator-app` job already runs on `macos-14`, but iOS SIMULATOR is a different build target from macOS desktop; they can share the runner but need separate Rust targets, build commands, and artifacts).
- A cross‑compiled or native‑compiled `x86_64-apple-darwin` (Intel) AND `aarch64-apple-darwin` (Apple Silicon) binary, ideally as a **universal binary** (`lipo -create`).
- An `.app` bundle with the correct `Info.plist` (CFBundleIdentifier, version, icon, etc.) — the existing `.goreleaser.yaml` is Linux‑specific; macOS needs a different packaging path (no GoReleaser for macOS unless a GoReleaser macOS runner picks it up). The macOS `.app` can be built by a script in the CI job that:
  1. Builds the Rust binary for both targets (cargo build --target x86_64-apple-darwin --target aarch64-apple-darwin — requires Xcode + Rust cross‑targets).
  2. Creates a universal binary with `lipo -create -output werust`.
  3. Builds the `.app` bundle structure (`Werust.app/Contents/MacOS/werust`, `Info.plist`, `PkgInfo`, `Resources/` with icon).
  4. Signs the bundle with `codesign` (Developer ID Application certificate from a macOS code‑signing key, stored in GitHub Actions secrets as a base64‑encoded `.p12` with a password).
  5. **Notarizes** it with `xcrun notarytool` (Apple ID credentials or App Store Connect API key, stored as secrets).
  6. Staples the ticket with `xcrun stapler staple Werust.dmg` / the `.app` zip.
- The artifact: a `.dmg` (disk image) or a signed and notarized `.zip` (depending on distribution preference).
- Costs: a `macos-14` runner (GitHub‑hosted, 10x the cost of a Linux runner — ~$0.08/min vs ~$0.008/min). The existing `ios-simulator-app` job already uses one, so the macOS desktop build can piggyback on that same runner (extending the job, not creating a new one).

### Windows desktop (x86_64 .exe)

- A `windows-latest` runner job (fully Windows‑native cross‑compile from the same ubuntu runner is not possible for native WebKitGTK because GTK/WebKitGTK do NOT exist on Windows. This is the fundamental blocker for Windows: werust's desktop shell is a **GTK4+WebKitGTK** application. There is no WebKitGTK on Windows).
- **ALTERNATIVE:** A standalone webview shell for Windows could be built with `webview2` (Edge WebView2) and a lightweight Rust HTTP backend that serves the same UI locally. That is a SIGNIFICANT design and implementation effort — basically a new backend (similar to the Android/iOS backends, but for Win32/Edge).
- **RECOMMENDATION:** Defer Windows desktop until a proper cross‑platform webview abstraction exists or until the Rust native renderer (`crates/native-renderer`) is real and ports to Windows. The practical Windows story for Phase 1 is the ADB‑forwarded Chrome DevTools / the Android APK on an emulator, plus the Linux desktop in WSL2‑forwarded X11. Not great, but honest.

### Cross-platform signing infrastructure

- A SINGLE set of GitHub Actions environment variables mapped to secrets:
  - `ANDROID_KEYSTORE_B64` (base64-encoded .jks)
  - `ANDROID_KEYSTORE_PASSWORD`
  - `ANDROID_KEY_ALIAS`
  - `ANDROID_KEY_PASSWORD`
  - `MACOS_CERTIFICATE_B64` (base64-encoded .p12 Developer ID Application cert)
  - `MACOS_CERTIFICATE_PASSWORD`
  - `MACOS_NOTARY_API_KEY_B64` (App Store Connect API key, or APPLE_ID + APPLE_ID_PASSWORD — API key is preferred because it does not need 2FA)
  - (Windows signing would go here when Windows builds exist — Authenticode cert in Azure Key Vault or a `.pfx` secret)
- The signing steps are NO-OP when the secrets are absent (dev builds, forks, dry‑runs). A dry‑run workflow_dispatch produces unsigned artifacts with a "NOT SIGNED" suffix.

## Gate axes

- **humanOnly: true** — this is a human‑driven spec because it requires: (a) the human to GENERATE and upload the signing keys/secrets to GitHub, which no agent can do; (b) a human decision on the macOS notarization workflow (Apple Developer account with the appropriate certificates); (c) a human call on whether to include Windows at all given the WebKitGTK dependency.
- **needsAnswers: true** — the open questions below must be answered before tasking.

## Open questions for the human

1. **Android signing keys**: do you have an existing keystore, or should the spec include instructions to generate one (`keytool -genkey -alias werust -keyalg RSA -validity 10000`)? Generated keystores are cheap and the spec can prescribe the exact commands.

2. **macOS notarization**: do you have an Apple Developer account with a Developer ID Application certificate? If not, this blocks the macOS signing path. (The iOS simulator build does not need signing because it is not distributed outside of development.)

3. **Windows**: given that werust uses GTK4/WebKitGTK which does not exist on Windows, is a Windows build in scope at all for Phase 1? If yes, the approach is either a separate Win32 webview2 backend (a new crate, similar to the Android/iOS Rust edges) or a port of the native renderer. Both are multi‑task efforts.

4. **Distribution channel**: are you distributing these signed builds through GitHub Releases (as today), through an app store (Google Play, Mac App Store), or both? App store distribution adds a separate set of signing requirements and review processes that this spec does not cover (but can).

5. **macOS desktop vs iOS**: do you need a macOS DESKTOP build, or is the iOS Simulator `.app` zip sufficient (assuming the user runs it in an iOS Simulator, not as a native macOS app)? The existing release already ships a `WerustShell-Simulator.app.zip` for the iOS Simulator; a true macOS desktop build would be a separate, native macOS app that does NOT require Xcode/the Simulator. The user's wording ("desktop builds for macos and windows") suggests the native macOS desktop, not the iOS simulator.

6. **macOS universal binary priority**: is Apple Silicon only, Intel only, or a universal binary? Universal is better UX but doubles build time and requires lipo. Recommend universal if time permits.

## Implementation sketch

When tasked, the most efficient order of implementation is:

1. **Android signing** (smallest lift, no new runner, no new platform — just a signing step on the existing android-apk job). Independent; can land in the next release alone.

2. **macOS desktop** (extend the existing ios-simulator-app runner to also build a native macOS .app using a shell script; signing and notarization are additive steps gated behind secret presence). Dependent on the human having a Developer ID cert.

3. **Windows** (deferred — only after a native webview abstraction exists). Not gated on the other two but fundamentally blocked by the GTK dependency.

Each should be its own task under this spec, with the signing infrastructure shared across them.

## Spec coverage

- **Android signing**: single task, self-contained.
- **macOS desktop**: single task after signing infrastructure is set up; could be combined with the ios-simulator job (same runner, different outputs).
- **Windows**: a research/ADR task ("what would a Windows backend look like") before any build task, to decide the approach.

## Prompt for the tasker

> From the `signed-multi-platform-builds` spec: cut a task for Android APK signing first (smallest, no new runner, just a Gradle signingConfig + CI signing step gated behind secrets). Then a task for the macOS desktop build (extend the ios-simulator runner to also build a native macOS .app + `lipo` universals + codesign + notarize, all gated behind secret presence). Leave Windows as deferred (an ADR/design note, not a build task) because werust uses GTK/WebKitGTK which does not exist on Windows.
