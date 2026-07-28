# Decisions: the mobile debug view (Android + iOS Console + Network tabs)

Task: `debug-view-console-network-tabs-mobile`.
Spec: `work/specs/tasked/in-app-debug-menu-console-and-network.md`.
Inherits: `docs/spikes/debug-capture-store-console-and-network-in-core/DECISIONS.md` (the store + the FFI shape), `docs/spikes/debug-console-network-capture-per-platform/DECISIONS.md` (the capture points and iOS's partial network coverage), `docs/spikes/debug-view-console-network-tabs-desktop/DECISIONS.md` (the desktop view this is the mobile twin of).

Code: Android in `crates/werust-android/app/src/main/java/com/github/wighawag/werust/DebugView.kt` (the view) + `BrowserActivity.kt` (the wiring); iOS in `crates/werust-ios/App/Sources/WKWebViewShellController.swift` (the `DebugViewController` + the wiring). Guarded by `crates/werust-core/tests/debug_view_mobile_wiring_shape.rs`.

These are the judgement calls this task bakes in, recorded so the reviewer and the Phase-2 toggle task inherit them explicitly. Manual device steps + a map of the wiring: [`README.md`](README.md).

## Decision 1 (Android): the debug view is a full-screen OVERLAY inside BrowserActivity, not a separate Activity

The menu's Debug entry toggles a `DebugView` (a programmatic `LinearLayout`) laid out MATCH_PARENT over the whole browser chrome in a `FrameLayout` root, hidden until opened.

- **Why.** The store lives behind the Activity's ONE `WerustCore` session, and a native session handle cannot cross an Activity boundary: a separate `DebugActivity` would need the session handed over through a static, which is exactly the kind of shared-mutable escape hatch this shell avoids everywhere else. An overlay needs no manifest entry, keeps the one core instance, and the SYSTEM Back button closes it through the same `OnBackPressedDispatcher` the rest of the shell uses (a second callback, registered after the history one so it wins only while the view is open).
- **The alternative considered.** A separate Activity (the task body suggested "maybe a debug Activity/Fragment"). Rejected for the session-boundary reason above; a Fragment would add the fragment dependency to a deliberately framework-only edge for no gain over a plain view.
- **What it touches.** Android only. If a future task wants the debug view reachable without a browsing session, the seam is the `DebugView` constructor (it takes any `WerustCore`).

## Decision 2: the tabs are TWO TOGGLED LISTS on both platforms, not a TabLayout/pager or a SwiftUI TabView

Android: two toggle buttons switching ONE `ListView`. iOS: a `UISegmentedControl` switching ONE `UITableView`.

- **Why.** The task body allowed exactly this ("a TabLayout + a pager, or two toggled lists"). A `TabLayout` + `ViewPager2` would add the material and viewpager2 dependencies to an Android edge that deliberately carries exactly one androidx dependency (`activity`, for the Back dispatcher); a SwiftUI `TabView` would pull a second UI framework into a UIKit-programmatic shell. Two tabs over one list is the same user-visible surface with zero new dependencies, and it keeps the two platforms structurally identical.
- **What it touches.** Only the debug views. If the tab set ever grows past two, the seam is the tab strip in each view; nothing else assumes the mechanism.

## Decision 3: the refresh is EVENT-DRIVEN on the existing chrome-refresh points (plus the console capture event); NO new timer, NO poll, and a full re-render per refresh

The mobile shells have NO periodic pump (unlike the desktop's 50ms `glib::timeout_add_local`): their chrome-refresh cadence is the event-driven `refreshChrome` after each core action and page lifecycle signal, which is what "the existing chrome-refresh cadence the ANR fix established" means on mobile. So the open debug view is refreshed (a) from `refreshChrome` itself, (b) on open, and (c) from the console capture event (Android's `onConsoleMessage`, iOS's `DebugCaptureHandler`), which already runs on the UI/main thread. Network rows captured on worker threads surface at the next chrome refresh (page lifecycle signals bound that gap).

- **Why no own timer.** The task is explicit: do not reintroduce a busy/tight main-thread loop; the Android ANR fix (`android-anr-main-thread-diagnose-and-unblock`) is respected. The FFI debug document reads OFF the native session lock (the store's Decision 6 in the capture DECISIONS), so even these event-driven refreshes can never block the UI thread behind an in-flight `ipfs://` retrieval.
- **Why a full re-render, not the desktop's incremental sequence-anchor.** The desktop needs the incremental plan because its pump ticks at 50ms whether or not anything changed. The mobile cadence is per page event, not per frame, and the store is bounded (300 entries x 2000 chars), so a full re-render per refresh is cheap and much simpler; the FFI debug JSON deliberately carries no sequence (the store's recorded FFI decision), so an incremental anchor is not even available over the wire. If a future profiler shows this hot, the seam is each view's `refresh()`.
- **What it touches.** The two views and the one new `onCapture` callback on iOS's `DebugCaptureHandler`. No new thread, timer, or executor is introduced anywhere.

## Decision 4: the Network tab speaks the mobile trust indicator's EXACT vocabulary, and the mapping is TOTAL and fail-closed

Each network row's trust is the indicator's glyph for the posture (`✓` / `◈` / `◇` / `⚠`, the same four `Chrome.trustIndicator()` paints on both platforms) plus the core's wire name the debug JSON already carries (`content-verified`, `unverified-origin`, `name-via-trusted-rpc`, `mutable-name`), in the same hues the desktop stylesheet gives the `trust-*` classes. This is the desktop view's Decision 4 applied to mobile, per its explicit hand-off.

- **The fail-closed part (new on mobile).** Both mobile mappings are total: an UNRECOGNISED posture string renders as `⚠ unverified-origin`, never verbatim. A verbatim render would smuggle a minted label into the one surface whose job is honest trust if the core ever added a posture the edge did not know; failing closed can only ever UNDERSTATE trust, which is the safe direction (the same rule the seams follow).
- **The alternative considered.** Reusing the page-badge strings (`✓ verified`, `⚠ unverified origin`) verbatim. Rejected for the desktop's reason: `✓ verified` is the PAGE-level summary wording; per-request the wire name is the more precise, and it is what the debug JSON already carries.
- **What it touches.** Nothing outside the two views; no new trust concept is introduced (ADR-0006).

## Decision 5: read-only by construction, unknowns render as `?`, newest at the bottom with stick-to-bottom scroll

Neither view constructs an input widget (no `EditText`, no `UITextField`/`UITextView`); a typeable REPL stays the native remote inspector's job (spec Out of Scope), and the wiring-shape guard asserts the absence. An unknown optional field (no status, no size, no mime, no source line, all JSON `null` in the debug document) renders as `?` or stays absent, never a fabricated `0` / `:0`, mirroring the store's honesty rule and the desktop mapping. Rows are oldest-first (the store's order), so newest is at the BOTTOM, the devtools-console idiom; the scroll sticks to the bottom only when the user is already there.

## Decision 6: iOS network coverage is inherited as-is (partial), and the view does not pretend otherwise

The capture task recorded exactly what iOS can see (its Decision 3): custom-scheme tasks, main-frame navigations, and page-issued fetch/XHR via the shim; never the browser-internal subresource loads WKWebView exposes no callback for. This view renders whatever the store holds and improves automatically as capture does; nothing in the view presents the iOS list as exhaustive, and the parity-matrix row names the asymmetry.

## Decision 7: the iOS view lives in the existing Swift files (no Xcode project edit)

`DebugViewController` is added to `WKWebViewShellController.swift` (which already carries several classes) rather than a new file, because a new Swift file would require editing `WerustShell.xcodeproj/project.pbxproj`, and this repo's verify gate has no Xcode to validate that edit. The class is cohesive (the whole debug view is one type), so the file grows one well-bounded member. If the file is ever split, the debug view is the natural first extraction.
