# The general browser menu (⋮): version + Debug entry

Task: `general-browser-menu-with-version-and-debug-entry`. Spec: `work/specs/tasked/in-app-debug-menu-console-and-network.md`. Decisions: [`DECISIONS.md`](DECISIONS.md).

werust now has the GENERAL browser menu every browser has (the ⋮ / hamburger menu), on desktop, Android and iOS. It is the menu CONTAINER, built to GROW into the usual browser items; its Phase-1 contents are the werust VERSION line and a DEBUG entry that opens the in-app debug view.

## Where it lives

| Platform | Affordance | Menu surface | Code |
| --- | --- | --- | --- |
| Desktop (GTK) | a `MenuButton` at the end of the toolbar, carrying GNOME's standard `open-menu-symbolic` (hamburger) icon | a GTK `Popover` of the core's items | `build_menu_button` in `crates/werust/src/main.rs` |
| Android | a `⋮` compact toolbar button | a native `PopupMenu` anchored on it | `showBrowserMenu` in `BrowserActivity.kt`, over `WerustCore.Menu` in `WerustCore.kt` |
| iOS | a `⋮` toolbar button (`showsMenuAsPrimaryAction`) | a native `UIMenu` | `browserMenu()` in `WKWebViewShellController.swift`, over `WerustCore.Menu` in `WerustCore.swift` |

The AFFORDANCE is deliberately platform-idiomatic rather than pixel-identical: desktop uses GNOME's standard menu icon (`open-menu-symbolic`, a hamburger — what a GTK user reaches for), while Android and iOS use `⋮` (what a phone user reaches for). Same menu, same items, each platform's own glyph.

The ITEMS are not written three times: the shared toolkit-free core owns the ordered list (`crates/werust-core/src/menu.rs`, `BrowserMenu`), each item carrying a stable cross-platform id, its label, and a `MenuItemKind` (`Info` = a non-interactive line, `Action` = an activatable entry). Each edge iterates that list and maps the kind onto a platform affordance. The mobile edges read the same list over the FFI as one JSON document (`nativeMenuJson` / `werust_ios_menu_json`).

## The one version source

`werust_core::version()` is the single source. Desktop reads it directly (the startup banner reads it too now, instead of its own `env!`); mobile reads it over the FFI (`nativeVersion` / `werust_ios_version`). No edge carries a version literal, the Gradle `versionName`, or the iOS bundle version in its menu. The guard `crates/werust-core/tests/browser_menu_edge_wiring_shape.rs` fails if one appears.

That accessor returns a version RESOLVED at build time by `crates/werust-core/build.rs`: the injected `WERUST_VERSION` (the release workflow exports it from the tag for every leg that compiles Rust), else `git describe --tags --always` with the leading `v` stripped (an informative dev build, `0.2.6-3-gabc1234`), else `CARGO_PKG_VERSION`. Without that injection a tagged release would ship every menu reading `werust 0.0.0`, since GoReleaser derives only the archive NAME from the tag. See Decision 2 in [`DECISIONS.md`](DECISIONS.md); the precedence lives in `crates/werust-core/src/version_resolution.rs` and is unit-tested.

## Growing the menu

Adding a bookmarks / settings / history entry is ONE edit in `BrowserMenu::new` (a new `MenuItem` with its own `MENU_ITEM_*` id). It then appears in all three native menus with no layout change. Only an `Action` item with new behaviour also needs a dispatch branch per edge (`if id == …` / `when (id)` / `if id == …`).

## The Debug entry is a HOOK

This menu task lands BEFORE the tabbed debug view (`debug-view-console-network-tabs-desktop` / `-mobile` are blockedBy it plus the capture store). So the Debug entry calls a named open-debug-view hook per edge:

- desktop `open_debug_view(&window)` (`crates/werust/src/main.rs`)
- Android `openDebugView()` (`BrowserActivity.kt`)
- iOS `openDebugView()` (`WKWebViewShellController.swift`)

Each currently states, honestly, that the view is not built yet (naming the version and that it will show Console + Network) rather than being a silent no-op that reads as a broken menu item. Replacing that ONE function per edge is the whole edge-side job of the debug-view tasks.

> Superseded on DESKTOP (2026-07-28): `debug-view-console-network-tabs-desktop` filled the desktop hook with the real tabbed view, so `open_debug_view` on desktop no longer shows the placeholder (see `docs/spikes/debug-view-console-network-tabs-desktop/README.md`). The two MOBILE hooks still carry the placeholder until `debug-view-console-network-tabs-mobile`. The manual steps below describe the menu as this task landed it; desktop step 5 now opens the real debug view.

## What the automated gate covers, and what it cannot

In the pure-Rust `verify` gate (`cargo fmt && clippy && build && test` — no Android SDK, no Xcode):

- `crates/werust-core/src/menu.rs` unit tests: the version line comes from the one source and is never empty or the un-injected `0.0.0` placeholder, the Debug entry exists and is activatable, the menu is a growable ordered list with unique ids, and the wire document carries everything the mobile edges need.
- `crates/werust-core/src/version_resolution.rs` unit tests: the build-time version precedence (injection beats `git describe` beats the Cargo version), the tag's leading `v` stripped, `git describe`'s trailing newline trimmed, and the no-git path never failing.
- `crates/werust-core/tests/release_plumbing_shape.rs`: every release leg that compiles Rust injects `WERUST_VERSION` from the tag ref name (and checks out with tags, so the `git describe` fallback works on the dry-run path).
- `crates/werust-android/rust/src/lib.rs` / `crates/werust-ios/rust/src/lib.rs` tests: the same version + menu document over each FFI (including the raw C-ABI exports and their string ownership), and that both accessors are session-free.
- `crates/werust/src/main.rs` tests: the desktop popover is built from the shared model, the banner and the menu agree on the version, and the Debug hook's placebo-free wording.
- `crates/werust-core/tests/browser_menu_edge_wiring_shape.rs`: a source-shape guard over all three edges (the ⋮ affordance + native surface, iterating the core's items, no hardcoded version, the Debug id routed to a named hook, and no debug-build gate on the menu).
- `crates/werust-core/tests/platform_capability_parity.rs`: the new `browser-menu` capability row.

What no automated test in this repo can cover: that the real GTK popover, the real Android `PopupMenu`, and the real iOS `UIMenu` actually appear and are tappable on a device. Those are the manual steps below.

## Manual verification steps (the native menu surfaces)

Not yet executed (no device/emulator session was run for this task). Each step is written so it can be executed and its result recorded here later.

### Desktop (GTK)

1. `cargo run -p werust` on a machine with a display.
2. The toolbar shows a menu button (the hamburger icon) at its right-hand end, after the trust indicator, with a "Menu" tooltip.
3. Click it: a popover opens with two entries — a greyed, non-clickable `werust <version>` line, and a clickable `Debug` entry.
4. The version in the popover matches the version in the startup banner printed on stdout (`werust <version> — a Rust web browser (webview backend)`), and it is NOT `0.0.0`: on a dev checkout it reads like `0.2.6-<n>-g<sha>`; on a tagged CI build it reads exactly the released version.
5. Click `Debug`: the popover closes and a dialog appears saying the in-app debug view is not built yet, naming the same version.
6. The menu opens the same way in a release build (`cargo run --release -p werust`): it is NOT debug-gated (unlike F12, which is).

### Android

1. Build + install the debug APK (the release workflow's `android-apk` job shape, or `./gradlew installDebug` in `crates/werust-android`).
2. The toolbar shows a `⋮` button at its right-hand end.
3. Tap it: a `PopupMenu` opens with a greyed `werust <version>` line and an enabled `Debug` entry.
4. The version matches what the desktop menu shows for the same commit (both read `werust_core::version()`), and is NOT the Gradle `versionName`.
5. Tap `Debug`: a toast says the in-app debug view is not built yet.
6. Opening the menu does not block the UI thread (it reads a build constant, no core session call, so no ANR risk — the `android-anr-main-thread-diagnose-and-unblock` fix is untouched).

### iOS (Simulator)

1. Build + run the Simulator app (`crates/werust-ios`, the release workflow's `ios-simulator-app` job shape).
2. The toolbar shows a `⋮` button at its right-hand end.
3. Tap it once (not a long press): a `UIMenu` appears with a disabled `werust <version>` line and an enabled `Debug` entry.
4. The version matches desktop/Android for the same commit, and is NOT `CFBundleShortVersionString`.
5. Tap `Debug`: an alert says the in-app debug view is not built yet.
