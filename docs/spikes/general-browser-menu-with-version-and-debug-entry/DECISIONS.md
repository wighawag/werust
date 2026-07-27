# Decisions: the general browser menu (version + Debug entry)

Task: `general-browser-menu-with-version-and-debug-entry`.
Spec: `work/specs/tasked/in-app-debug-menu-console-and-network.md`.
Code: `crates/werust-core/src/menu.rs` (the model + the one version source in `crates/werust-core/src/lib.rs`), the FFI accessors in `crates/werust-{android,ios}/rust/src/lib.rs` + `crates/werust-ios/Sources/werust_mobile.h`, and the three edges (`crates/werust/src/main.rs`, `BrowserActivity.kt` + `WerustCore.kt`, `WKWebViewShellController.swift` + `WerustCore.swift`).

These are the judgement calls this task bakes in, recorded so the follow-on tasks (`debug-view-console-network-tabs-desktop` / `-mobile`, and any future menu item) inherit them explicitly rather than re-deriving them. Manual steps + a map of the wiring: [`README.md`](README.md).

## Decision 1: the menu MODEL lives in `werust-core`, not three times at the edges

A new `werust_core::menu` module owns the ordered list of items (`BrowserMenu` / `MenuItem` / `MenuItemKind`), and each edge only RENDERS it in its native widget.

- **What it touches.** All three edges, and every future menu item task.
- **Why.** This is the same "one shared fact, per-platform native rendering" shape the chrome state, the trust indicator, the load step and the debug capture store already use (`CONTEXT.md`; ADR-0005's parity concern). It is also what makes "structured to grow" real rather than aspirational: a future bookmarks/settings entry is ONE edit in `BrowserMenu::new` and it appears in all three menus, because each edge ITERATES the list. And it puts the menu inside the pure-Rust `verify` gate, which never compiles Kotlin or Swift.
- **The alternative considered.** Each edge declaring its own two menu entries natively (much less code today: two literal entries per platform). Rejected: it forks the item list three ways, so the first added item is three parallel edits that can silently disagree — exactly the class of drift the parity guard exists for.
- **Consequence.** An edge can only diverge by NOT iterating, which the source-shape guard `crates/werust-core/tests/browser_menu_edge_wiring_shape.rs` fails on.

## Decision 2: `werust_core::version()` is the ONE version source, and the desktop banner was switched onto it

The version is `env!("CARGO_PKG_VERSION")` read in exactly ONE place, `werust_core::version()`. Mobile reads it over the FFI (`nativeVersion` / `werust_ios_version`). The desktop startup banner, which previously had its own `env!("CARGO_PKG_VERSION")`, now reads the same accessor.

- **What it touches.** The desktop banner (a pre-existing surface, changed by this task), both mobile FFI surfaces, and any future version reader (an about box, a crash report, a UA string).
- **Why.** The task requires all three menus to agree. Two readers is already the drift condition, and this task was about to create three more. Note an `env!` inside a mobile crate would read THAT crate's version, which equals the workspace version today only because of `workspace.package.version` inheritance — a fragile coincidence to depend on.
- **Explicitly NOT used as the source:** the Android Gradle `versionName` (currently a hand-maintained `"0.0.0"` in `app/build.gradle.kts`, which does NOT track the workspace version) and the iOS bundle's `CFBundleShortVersionString`. Both would let the mobile menus show a different version from desktop. The guard asserts neither appears in the menu path.
- **NOT done here (deliberately out of scope):** making the Gradle `versionName` itself derive from the workspace version. That is a RELEASE-plumbing change (it affects the APK's user-visible app version and the release artifacts, `docs/adr/0002`), not a menu change, and it is worth its own task. The menu simply does not read it.

## Decision 3: the Debug entry calls a per-edge open-debug-view HOOK, and the hook says the view is not built yet

The menu lands BEFORE the debug view (the recommended sequencing in the task; the view tasks are `blockedBy` this one plus the store). Each edge has ONE named hook — `open_debug_view` (desktop) / `openDebugView` (Android, iOS) — which today shows a short message stating the in-app debug view (Console + Network) is not built yet.

- **What it touches.** `debug-view-console-network-tabs-desktop` and `-mobile`: replacing that one function per edge is their whole edge-side job. The menu item, its id, its dispatch and its placement do not change when they land.
- **Why a visible message rather than a no-op.** A menu entry that does nothing when tapped reads as a BROKEN browser, which is worse than an honest "not built yet" — and it is the same fail-visibly posture the rest of werust takes (the prominent error banner exists because a silent failure was missed in the field). It is also a user-facing string, so it is recorded here rather than buried.
- **The alternative considered.** Sequencing this task AFTER the debug-view tasks so the entry opens the real view immediately (the task offered this). Rejected: the view tasks are declared `blockedBy` this one, so that ordering is circular; the task's own RECOMMENDED order is the one taken.
- **Why a named function and not an inline closure.** So the swap is one grep-able site per edge, and so the shape guard can assert the routing exists.

## Decision 4: the menu is NOT debug-build-gated, and that is a deliberate contrast with `web-inspector`

The ⋮ menu (and its Debug entry) is available in every build. Each platform already HAS a debug gate for the NATIVE remote inspector — desktop `enable-developer-extras` under `cfg!(debug_assertions)`, Android `ApplicationInfo.FLAG_DEBUGGABLE`, iOS `#if DEBUG` (`web-inspector-devtools-gating-decisions-2026-07-23.md`) — and reusing one of those here was the plausible mistake.

- **Why.** The spec is explicit: the general menu is a USER-FACING feature, and the in-app debug view exists precisely FOR the untethered phone user (who is on a shipped build, by definition). Gating it on a debug build would delete the whole point. The remote inspector's gate stays exactly as it is: that gate is about not shipping a remotely-inspectable RELEASE, a different risk.
- **Pinned** by `the_menu_is_never_debug_build_gated_on_any_edge`, bounded to the menu-building code so the inspector's legitimate gate elsewhere in the same files does not satisfy it.

## Decision 5: a dedicated session-free `menu_json()` / `version()` FFI pair, not a chrome-JSON section

The mobile edges get two NEW exports each — `nativeVersion` / `nativeMenuJson` (JNI) and `werust_ios_version` / `werust_ios_menu_json` (C-ABI) — and both take NO session handle.

- **What it touches.** The mobile FFI surface (and `crates/werust-ios/Sources/werust_mobile.h`, which must stay in lock-step with the Rust exports).
- **Why dedicated.** The same reasoning the capture store recorded (its Decision 1): the chrome JSON is re-encoded on EVERY chrome refresh, and the menu is read once when the menu is built. Additive either way; this keeps every existing chrome reader byte-for-byte unaffected.
- **Why session-free.** The version and the menu are properties of the BUILD, not of a browsing session. Threading a `CoreSession` handle through them would (a) imply the menu depends on session state, which would undercut "always available", and (b) needlessly take the Android `SyncSession` lock to read a constant. Kotlin therefore declares them as companion-object externals, and Swift as `static` methods.
- **The alternative considered.** Reusing the existing per-session accessor shape for consistency with every other export. Rejected on the two points above; the asymmetry is deliberate and documented at each accessor.

## Decision 6: the AFFORDANCE is platform-idiomatic, not one glyph everywhere

Desktop's menu button carries GNOME's standard `open-menu-symbolic` (hamburger) icon; Android and iOS use a `⋮` glyph.

- **Why.** The task asks for "a ⋮/menu affordance", i.e. the recognisable primary-menu button, not a specific character. On GTK the recognisable one is the hamburger (every GNOME app's `MenuButton`), and using a literal `⋮` there would look foreign; on phones `⋮` is what a user reaches for. The MENU behind it is identical on all three, which is the property that actually matters.
- **What it touches.** The parity row's description and the source-shape guard (which asserts each edge's own affordance, not one shared glyph).

## Decision 7: item KIND is a small closed enum (`Info` / `Action`), not a `disabled: bool`

- **Why.** `Info` names WHAT the entry is (a line of information), while `disabled` would name how it happens to render today — and "disabled" already means something else in a menu (a temporarily-unavailable action, e.g. a greyed Back). Keeping them distinct means a future genuinely-unavailable action can be expressed without re-meaning the version line. As an enum, a future variant (a submenu, a toggle) makes each edge's `match` non-exhaustive, so no edge can silently render a new kind wrong.
- **Coherence check.** `Info` / `Action` do not collide with any existing term in `CONTEXT.md` or the ADRs; the ids (`version`, `debug`) are lower-kebab wire names, matching the existing `trustPosture` / `loadStep` / `ConsoleLevel` wire vocabulary. The word "menu" itself already appeared in the codebase only inside the phrase "in-app debug menu" (the spec's own name for this feature), which this menu IS the container for — no re-meaning.

## Decision 8: one `browser-menu` capability row, `implemented` on all three

The parity matrix gains one row for the MENU (not for the debug view), `implemented` everywhere.

- **Why now.** The capture-store task recorded (its Decision 5) that the row belongs with "the first task that makes the capability reachable by a user" — this one. The row names the MENU capability precisely, so it is honest: the menu really is wired on all three platforms. The debug VIEW is a different capability and will bring its own row (where iOS's honestly partial network coverage becomes the asymmetry the guard exists to catch).
- **Not** three `stubbed` cells pointing at the view tasks: that would claim the menu is a gap, which it is not.
