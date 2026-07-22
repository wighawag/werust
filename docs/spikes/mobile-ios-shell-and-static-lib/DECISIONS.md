# Decisions — mobile-ios-shell-and-static-lib

Durable record of the design choices made building the iOS app that links the werust Rust core as a cross-compiled static library. Linked from the task done record so a reviewer + the human can ratify or reverse. Current truth remains the code + ADRs; this file only explains the load-bearing choices. It mirrors the Android task's DECISIONS (`docs/spikes/mobile-android-shell-and-static-lib/DECISIONS.md`); where a choice is the direct iOS twin of an Android one, that is noted.

## iOS backend is a Rust `Renderer` over the platform WKWebView, driven from Swift over a C-ABI

**Chosen:** the iOS OS edge (Swift `UIViewController` + URL bar + back/forward buttons) hosts the platform `WKWebView` and drives the Rust core over a small C-ABI surface (`werust_ios_*`, declared in `Sources/werust_mobile.h`, the project's bridging header). The Rust core holds a `BrowserShell` over an `IosBackend` (a `Renderer` implementation that models the browsing surface: it records the navigate/back/forward/reload/stop intents and the load-lifecycle, and exposes the derived `ChromeState`). Swift: (1) forwards user actions INTO the core, (2) reads the core's `ChromeState` back out (as JSON) to paint the URL bar / button enablement / status, and (3) reflects the core's committed URL onto the `WKWebView` and reports the `WKWebView`'s real load-lifecycle signals back into the core. Swift holds NO browsing logic of its own.

**Why:** on iOS the forced OS edge is Swift + `WKWebView` (there is no GTK). Confining Swift to the edge means the browsing LOGIC lives in Rust behind the seam, exactly as the desktop `BrowserShell` does over WebKitGTK and the Android edge does over the Java `WebView`. This is the DIRECT twin of the Android `AndroidBackend`: the same platform-neutral session-history + load-lifecycle logic, only the FFI differs (a plain C-ABI here vs JNI there). `IosBackend`/`CoreSession`/`ffi_json` are byte-for-byte analogues of the Android core so the two mobile edges stay in lock-step.

**Alternatives considered:**
- *Put the URL bar / history logic in Swift and use Rust only for a helper.* Rejected: that is browsing logic outside the core — the exact thing the task forbids ("Swift confined to the OS edge").
- *Mirror wezig's iOS shape literally (Swift passes C ops tables the core calls back into to drive the WKWebView).* Rejected: the werust Android core already established a simpler, robust mobile FFI shape (core surfaces a pending-load URL + a chrome-JSON read-back; the edge applies the URL to the platform webview and reports its signals back). Reusing that shape keeps the two werust mobile edges identical rather than importing wezig's more elaborate ops-table protocol; the seam contract is the same.

**What it touches:** adds a new `IosBackend` `Renderer` implementation (a sibling of the Android `AndroidBackend`) and a C-ABI export surface. It does not change the `Renderer` seam trait. The release job `release-goreleaser-rust-desktop-and-mobile-artifacts` consumes the `.app` this produces.

## The Rust core ships as a `staticlib` (`.a`), linked into the app with `-force_load`

**Chosen:** the iOS core crate builds a `staticlib` (`libwerust_mobile.a`) cross-compiled by cargo for `aarch64-apple-ios-sim` (the Simulator on Apple-Silicon runners), staged at the stable path `target/ios-lib/libwerust_mobile.a` by the Xcode "Build Rust static lib" build phase, and linked into the Swift app via `OTHER_LDFLAGS -force_load`. (The library is named `werust_mobile` rather than `werust_core` so its output does not collide with the shared `werust-core` crate's rlib in the workspace — the SAME convention the Android core uses; the browsing logic it carries IS the `werust-core` crate, linked in.)

**Why:** an iOS Simulator app has no signing and no runtime `dlopen` of an unsigned `.dylib`, so the linkable-into-the-app form of the Rust core is a STATIC archive `-force_load`ed into the app binary — which is also exactly the task's wording ("static lib") and wezig's iOS shape (`libwezig_mobile.a` `-force_load`ed). `-force_load` keeps every `#[no_mangle]` C-ABI export in the final binary even though only a few are referenced, so the BUILD-leg check can confirm the core is linked by looking for its symbols. This is the iOS-idiomatic realisation of "cross-compile the Rust core and link it into the app", and unlike Android (which needs a `cdylib` `.so` because Android loads native code via `System.loadLibrary`), iOS genuinely wants the `.a`.

**What it touches:** the release job consumes the `.app` this build produces; the crate name (`werust-ios-core`) + the staged lib path (`target/ios-lib/libwerust_mobile.a`) + the build-phase script are the contract it reuses.

## The single-arch pin (`ONLY_ACTIVE_ARCH=YES ARCHS=$(uname -m)`) is copied from wezig

**Chosen:** `build-and-run.sh` pins ONE arch end-to-end for both the link and the per-arch "Build Rust static lib" phase. Without it, `xcodebuild build` with no `-destination` resolves `ARCHS` to every simulator arch and links one, while the script phase sees `CURRENT_ARCH=undefined_arch` and builds another — an architecture mismatch. wezig hit this exact bug; copying the guard (plus `CODE_SIGNING_ALLOWED=NO`, Simulator-only) is load-bearing.

**Why:** it is a known iOS-build footgun the forward-pointer explicitly calls out; not pinning it would make the CI leg flaky/red for a reason unrelated to werust's own code.

## The BUILD-leg check + the Xcode build are a SEPARATE CI leg, not the pure-Rust `verify` gate

**Chosen:** the repo `verify` gate is `cargo fmt --check && cargo clippy && cargo build && cargo test` — a pure-Rust, no-Xcode gate. The Xcode `.app` build + the criterion-4 BUILD-leg check (`docs/spikes/mobile-ios-shell-and-static-lib/check-app-bundle.sh`, asserting the `.app` bundle + binary + linked Rust core) run in a dedicated `mobile-ios` workflow on `macos-14` (the direct analogue of the Android `check-apk-abis` leg), and the release/mobile job packages the same `.app`. The core's own Swift↔core protocol logic (`CoreSession`/`IosBackend`/`ffi_json`, INCLUDING the C-ABI exports, which are `cfg`-free so the host links them) stays testable in the always-on Rust gate via `cargo test`.

**Why:** an Xcode/Simulator build needs macOS + Xcode + the iOS SDK and cannot run inside the Linux pure-Rust gate. Keeping the fast, deterministic logic in `cargo test` and the platform build in its own macOS leg is the same split the Android task established and what the release task's forward-pointer expects ("do NOT expect a local Xcode build in the pure-Rust `verify` gate; the Xcode/Simulator build is a SEPARATE CI leg").

**Coherence note ("static lib" vs the Android `.so`):** the acceptance phrase "static lib" is honoured LITERALLY on iOS (a `.a` `-force_load`ed into the app), whereas the Android task honoured it in SPIRIT with a `cdylib` `.so` (because Android has no way to load a `.a` at runtime). Same task family, platform-correct artifact each side — no concept is re-meant.
