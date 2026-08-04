# werust — iOS app shell

A real iOS app (not a spike) that links the werust **Rust core** cross-compiled as a static library and launches a browsing surface on the iOS Simulator. This is the iOS half of mobile parity with wezig (task `mobile-ios-shell-and-static-lib`, spec story 18), the twin of the Android app module (`crates/werust-android`).

## What it is

- **Swift at the OS edge only.** `WKWebViewShellController` (a plain `UIViewController`) hosts a URL bar, ONE reload/stop control, a loading spinner, the ⋮ menu, and the platform `WKWebView`. It holds NO browsing logic: every decision (URL-bar text, the control's mode, whether the spinner shows, load status) is the Rust core's truth, read back through `WerustCore` (`App/Sources/WerustCore.swift`). There are no ◀/▶ buttons: the WebKit edge-swipe is the iOS history affordance and it covers both directions, so on a phone toolbar the URL bar is worth more than two controls duplicating a platform gesture (task `ios-chrome-collapse-reload-stop-and-drop-history-buttons`, spec `chrome-conventional-controls` story 11). The history CAPABILITY is untouched — `can_go_back` / `can_go_forward` and the seam's history methods are exactly as they were, and the swipe rides on them.
- **The Rust core behind the seams.** The browsing logic is the shared `werust-core` crate (`BrowserShell` over the `Renderer` seam), driven on iOS through an `IosBackend` (`crates/werust-ios/rust`). The core is compiled to `libwerust_mobile.a` and called from Swift over a C-ABI (declared in `Sources/werust_mobile.h`, the project's bridging header). This is the SAME `werust-core` crate the desktop GTK shell and the Android app link — not a per-platform copy.
- **Cross-compiled as a normal Xcode build phase.** The "Build Rust static lib" build phase (`App/build-rust-lib.sh`) runs `cargo build` for `aarch64-apple-ios-sim` and stages `libwerust_mobile.a`, which the app links with `OTHER_LDFLAGS -force_load`. So `xcodebuild -sdk iphonesimulator ... build` produces a Simulator `WerustShell.app` carrying the Rust core. Simulator only; device/store builds need signing (out of scope).

## Build + run (on a Mac / macos-14 CI runner)

Requires Xcode + the Rust toolchain with the iOS Simulator target:

```sh
rustup target add aarch64-apple-ios-sim
crates/werust-ios/build-and-run.sh          # builds, boots an iOS 17 sim, launches, asserts the Rust greeting
BUILD_ONLY=1 crates/werust-ios/build-and-run.sh   # build only: stops after producing the .app
```

The built app lands at `crates/werust-ios/build/WerustShell.app`.

## BUILD-leg check (the `.app` contains the app bundle + binary)

```sh
docs/spikes/mobile-ios-shell-and-static-lib/check-app-bundle.sh
```

It fails unless the path is a real `.app` bundle carrying its `Info.plist`, the app binary (a Mach-O), and the Rust core's `werust_ios_*` C-ABI symbols (proof the static `-force_load` link pulled the core into the binary) — `werust_ios_session_new` plus `werust_ios_activate_reload_stop_control`, the entry point the toolbar's ONE reload/stop control (and therefore CANCEL) rides on. This is a separate leg from the repo's pure-Rust `verify` gate (which cannot host an Xcode build); the `mobile-ios` CI workflow (`.github/workflows/mobile-ios.yml`, `macos-14`) and the release/mobile job (`release-goreleaser-rust-desktop-and-mobile-artifacts`, `docs/adr/0002`) run it after building the `.app`.

## Design decisions

See `docs/spikes/mobile-ios-shell-and-static-lib/DECISIONS.md`.

The EDGE-SWIPE history gesture (`allowsBackForwardNavigationGestures`, enabled in `WKWebViewShellController.layoutChrome`) has its own set, because WebKit performs a gesture navigation itself and reports it differently from a programmatic one: `docs/spikes/enable-the-ios-back-forward-swipe-gesture/DECISIONS.md`. Both the flag and the wiring that keeps the chrome honest after a swipe are pinned in the pure-Rust gate by `rust/tests/back_forward_gesture_wiring_shape.rs`, since no human on this project has a Mac to notice a regression.

The COLLAPSE of Reload and Stop into one control, the loading spinner, and the removal of the history buttons the swipe replaced: `docs/spikes/ios-chrome-collapse-reload-stop-and-drop-history-buttons/DECISIONS.md`, pinned by `rust/tests/collapsed_control_and_dropped_history_buttons_shape.rs`.
