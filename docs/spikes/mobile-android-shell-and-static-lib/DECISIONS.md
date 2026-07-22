# Decisions — mobile-android-shell-and-static-lib

Durable record of the design choices made building the Android app module that links the werust Rust core as a cross-compiled static/shared library. Linked from the task done record so a reviewer + the human can ratify or reverse. Current truth remains the code + ADRs; this file only explains the load-bearing choices.

## Extracted the browsing core into a shared `werust-core` library crate

**Chosen:** moved the GTK-free `BrowserShell` + `ChromeState` seam logic out of the desktop `werust` binary crate (`crates/werust/src/shell.rs`) into a new library crate `crates/werust-core`. The desktop binary now depends on `werust-core` and re-uses the exact same shell logic; the Android FFI core (`crates/werust-android`) depends on the SAME crate.

**Why:** the task says "browsing logic stays in the Rust core behind the seams" and CONTEXT.md names "the Rust core" as a first-class concept. Before this task there was no crate actually NAMED the core: the browsing logic lived inside the desktop binary, so mobile could not link it. Extracting it makes "the Rust core" a real, buildable artifact shared by every OS edge (desktop GTK view, Android Kotlin Activity, and later the iOS Swift shell), rather than duplicating `BrowserShell` per platform. This is the coherent home for the shared browsing logic the seam already abstracts.

**Alternatives considered:**
- *Duplicate `BrowserShell` in the Android crate.* Rejected: two copies of the browsing logic drift, and the whole point of the seam is one core behind many OS edges.
- *Depend on the `werust` BINARY crate.* Rejected: a `[[bin]]` crate that links GTK is not linkable from an Android `cdylib`, and a binary is not a library dependency.

**What it touches:** the desktop `werust` crate (its `mod shell;` becomes `use werust_core::{BrowserShell, ChromeState};`) and the workspace `Cargo.toml` (new member). The shell's public API and tests move verbatim; behaviour is unchanged, so the desktop shell task's acceptance is preserved. The `mobile-ios-shell-and-static-lib` sibling task will reuse the SAME `werust-core` crate for its Swift edge.

**Coherence check:** "core" is CONTEXT.md's own term ("browsing logic stays in the Rust core behind the seams"); this crate makes that term concrete rather than re-meaning it. It does not overlap `renderer` (the seam) or the backends (`webview-renderer`, `native-renderer`): the core is the seam-DRIVING logic (URL bar, history, chrome), backends are seam IMPLEMENTATIONS.

## Android backend is a Rust `Renderer` over the platform WebView, driven from Kotlin over JNI

**Chosen:** the Android OS edge (Kotlin `Activity` + URL bar + back/forward buttons) hosts the platform `android.webkit.WebView` and drives the Rust core over a small JNI/C-ABI surface. The Rust core holds a `BrowserShell` over an `AndroidBackend` (a `Renderer` implementation that models the browsing surface: it records the navigate/back/forward/reload/stop intents and the load-lifecycle, and exposes the derived `ChromeState`). Kotlin: (1) forwards user actions (typed URL, Back, Forward, Reload, Stop) INTO the core, (2) reads the core's `ChromeState` back out to paint the URL bar / button enablement / status, and (3) reflects the core's committed URL onto the platform WebView and reports the WebView's real load-lifecycle signals back into the core. Kotlin holds NO browsing logic of its own: history availability, URL-bar text, and load state are all the core's truth.

**Why:** on Android the forced OS edge is Kotlin + the Java `WebView` (there is no GTK). Confining Kotlin to the edge means the browsing LOGIC (what Back does, whether Back is available, what the URL bar shows) lives in Rust behind the seam, exactly as the desktop `BrowserShell` does over WebKitGTK. The JNI surface is deliberately the SAME shape as the desktop main.rs wiring (navigate/go_back/go_forward/reload/stop/pump + read `ChromeState`), so the two edges are the same core with a different view.

**Alternatives considered:**
- *Put the URL bar / history logic in Kotlin and use Rust only for a helper.* Rejected: that is browsing logic outside the core — the exact thing the task forbids ("Kotlin confined to the OS edge").
- *Render in-process in Rust (native renderer) instead of the platform WebView.* Rejected for THIS task: the day-one path is the system webview (ADR-0001 "webview now"), and the platform WebView is Android's system webview. The native renderer stays hot-swappable behind the same seam later.

**What it touches:** adds a new `AndroidBackend` `Renderer` implementation (a sibling of `webview-renderer`'s `WebViewRenderer`, but Rust-side/edge-driven), and a JNI export surface. It does not change the `Renderer` seam trait.

## The Rust core ships as a `.so` (cdylib), packaged per-ABI into the APK by a Gradle step

**Chosen:** the Android core crate builds a `cdylib` (`libwerust_mobile.so`) cross-compiled by cargo for `aarch64-linux-android` (arm64-v8a) and `x86_64-linux-android` (x86_64) using the NDK's clang linkers, and a Gradle task copies each ABI's `.so` into `src/main/jniLibs/<abi>/` before `mergeJniLibFolders`, so the packaged debug APK carries `lib/arm64-v8a/libwerust_mobile.so` and `lib/x86_64/libwerust_mobile.so`. (The library is named `werust_mobile` rather than `werust_core` so its cdylib/rlib output does not collide with the shared `werust-core` crate's rlib in the workspace; the browsing logic it carries IS the `werust-core` crate, linked in.)

**Why:** an Android app loads native code via `System.loadLibrary` from a `.so` inside the APK's `lib/<abi>/` tree; a `staticlib` (`.a`) cannot be `dlopen`ed at runtime. The task says "static library" (the wezig-parity framing / ADR-0002 "static lib each mobile app links"); on Android the linkable-into-the-app form of the Rust core is the JNI `.so`. Building a `cdylib` is the Android-idiomatic realisation of "cross-compile the Rust core and link it into the app". The floor ABIs are the two named in the acceptance criteria (arm64-v8a for real devices, x86_64 for the emulator).

**Coherence note on "static lib" vs `.so`:** the acceptance criterion phrase "static lib" is honoured in SPIRIT (the Rust core cross-compiled and linked into the app as a native library the app carries); the Android-correct artifact is a JNI shared object, because Android has no mechanism to load a `.a` at runtime. This mirrors what the release job expects ("the Rust static-lib cross-compiles" packaged into the APK). If a reviewer wants a literal `.a` too, `crate-type` already also emits `staticlib` for anyone who wants to statically link; the APK carries the `cdylib`.

**What it touches:** the release job `release-goreleaser-rust-desktop-and-mobile-artifacts` consumes this Gradle build to produce the Android APK artifact; the ABI set + Gradle task name are the contract it reuses.

## BUILD-leg check is a standalone script, not part of the Rust `verify` gate

**Chosen:** the "APK carries the Rust core lib for both ABIs" assertion is a standalone script (`docs/spikes/mobile-android-shell-and-static-lib/check-apk-abis.sh`) that unzips the built APK and fails unless BOTH `lib/arm64-v8a/libwerust_mobile.so` and `lib/x86_64/libwerust_mobile.so` are present. The Rust-side seam behaviour of the Android core is covered by ordinary `cargo test` (which the `verify` gate runs).

**Why:** the repo `verify` gate is `cargo fmt --check && cargo clippy && cargo build && cargo test` — a pure-Rust, no-Android-SDK gate. A Gradle/APK build needs the Android SDK+NDK and cannot run inside that gate, so the BUILD-leg APK-ABI assertion is a separate leg (run in the release/mobile CI job, as ADR-0002's hand-written mobile jobs do), while the core's own logic stays testable in the always-on Rust gate.
