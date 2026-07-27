//! Browser-menu edge-wiring shape guard (task
//! `general-browser-menu-with-version-and-debug-entry`, spec
//! `in-app-debug-menu-console-and-network`).
//!
//! WHAT LANDED: a GENERAL browser menu (the ⋮ menu every browser has, structured
//! to GROW into the usual items) on all three platforms, whose Phase-1 contents
//! are the werust VERSION line and a DEBUG entry that opens the in-app debug view.
//! The item LIST is the shared core's [`BrowserMenu`], and each OS edge renders it
//! in its own native menu widget.
//!
//! WHY A SOURCE-SHAPE GUARD: the core half (the menu model, the one version
//! source, the wire form) is unit-tested where it lives (`werust-core`'s `menu`
//! module, plus the FFI tests on both mobile cores). What is NOT otherwise
//! assertable is that the EDGES really read that shared model instead of
//! hardcoding a version or a menu of their own — and the mobile edges are Kotlin
//! and Swift, which this repo's pure-Rust `verify` gate (`cargo fmt && clippy &&
//! build && test`, no Android SDK, no Xcode) never compiles at all. So this test
//! PARSES the two mobile edges and asserts that shape, exactly as
//! `crates/werust-android/rust/tests/system_back_wiring_shape.rs` does for system
//! Back. It also covers the DESKTOP edge (which the gate DOES compile) for the one
//! thing compilation cannot prove: that the popover is built from the shared model
//! rather than from a literal.
//!
//! It lives in `werust-core` (not the Android core) because it spans all THREE
//! edges, and `werust-core` is the one crate every edge sits over — the same
//! reason its sibling guards `platform_capability_parity.rs` and
//! `release_plumbing_shape.rs` live here.
//!
//! Acceptance criteria mapped to assertions below:
//! 1. Every platform has a ⋮ menu affordance opening a menu surface, structured to
//!    grow (`every_edge_has_a_menu_affordance_opening_a_native_menu_surface`,
//!    `every_edge_renders_whatever_items_the_core_lists_so_the_menu_can_grow`).
//! 2. It shows the version from ONE source, over the FFI on mobile
//!    (`no_edge_hardcodes_a_version_string`).
//! 3. It has a Debug entry wired to an open-debug-view HOOK
//!    (`every_edge_routes_the_debug_item_to_an_open_debug_view_hook`).
//! 4. User-facing, never debug-build-gated
//!    (`the_menu_is_never_debug_build_gated_on_any_edge`).
//! 5. Applied on desktop, Android and iOS — plus the parity-matrix row, enforced by
//!    the sibling `platform_capability_parity.rs`.
//! 6. Tests cover what is testable; the native menu surfaces themselves carry
//!    recorded manual steps at
//!    `docs/spikes/general-browser-menu-with-version-and-debug-entry/README.md`.

use std::path::{Path, PathBuf};

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/werust-core`, so the root is two levels up.
fn source(relative: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn desktop_shell() -> String {
    source("crates/werust/src/main.rs")
}

fn android_activity() -> String {
    source("crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt")
}

fn android_binding() -> String {
    source("crates/werust-android/app/src/main/java/com/github/wighawag/werust/WerustCore.kt")
}

fn ios_controller() -> String {
    source("crates/werust-ios/App/Sources/WKWebViewShellController.swift")
}

fn ios_binding() -> String {
    source("crates/werust-ios/App/Sources/WerustCore.swift")
}

#[test]
fn every_edge_has_a_menu_affordance_opening_a_native_menu_surface() {
    // Criterion 1: a ⋮ menu affordance in the shell, opening the platform's own
    // menu surface — desktop a GTK MenuButton + Popover, Android a PopupMenu, iOS
    // a UIMenu. Not a bespoke werust window on any of them.
    let desktop = desktop_shell();
    assert!(
        desktop.contains("MenuButton::builder()") && desktop.contains("Popover::builder()"),
        "desktop must open the menu in a GTK MenuButton + Popover"
    );
    assert!(
        desktop.contains("toolbar.append(&build_menu_button(&window));"),
        "the desktop menu button must be in the toolbar"
    );

    let android = android_activity();
    assert!(
        android.contains("PopupMenu(this, menuButton)"),
        "Android must open the menu in a native PopupMenu anchored on the ⋮ button"
    );
    assert!(
        android.contains("compactNavButton(\"⋮\") { showBrowserMenu() }"),
        "Android must have a ⋮ toolbar button that opens the menu"
    );
    assert!(
        android.contains("toolbar.addView(menuButton)"),
        "the Android menu button must be in the toolbar"
    );

    let ios = ios_controller();
    assert!(
        ios.contains("UIMenu(title: \"\", children: actions)"),
        "iOS must open the menu in a native UIMenu"
    );
    assert!(
        ios.contains("menuButton.setTitle(\"⋮\", for: .normal)")
            && ios.contains("menuButton.showsMenuAsPrimaryAction = true"),
        "iOS must have a ⋮ toolbar button whose primary action presents the menu"
    );
}

#[test]
fn every_edge_renders_whatever_items_the_core_lists_so_the_menu_can_grow() {
    // Criterion 1 (the GROWTH half) + criterion 4's "adding future items is
    // structurally trivial": each edge ITERATES the shared core's item list and
    // renders each item by its `kind`, rather than laying out two hardcoded
    // entries. That is what makes a future bookmarks/settings item a `werust-core`
    // change alone (plus one dispatch branch if it is an action).
    let desktop = desktop_shell();
    assert!(
        desktop.contains("for item in menu.items()"),
        "desktop must build its popover by iterating the core's items"
    );
    assert!(
        desktop.contains("MenuItemKind::Info") && desktop.contains("MenuItemKind::Action"),
        "desktop must render each item by its core kind, not by position"
    );

    let android = android_activity();
    assert!(
        android.contains("menu.items.forEachIndexed"),
        "Android must build its PopupMenu by iterating the core's items"
    );
    assert!(
        android.contains("entry.isEnabled = item.isAction()"),
        "Android must render each item by its core kind (an info line is not tappable)"
    );

    let ios = ios_controller();
    assert!(
        ios.contains("WerustCore.menu().items.map"),
        "iOS must build its UIMenu by mapping the core's items"
    );
    assert!(
        ios.contains("if !item.isAction() { action.attributes = [.disabled] }"),
        "iOS must render each item by its core kind (an info line is not tappable)"
    );

    // Both mobile bindings decode the SAME wire document, so neither invents its
    // own item list.
    assert!(
        android_binding().contains("fun menu(): Menu = Menu.fromJson(nativeMenuJson())"),
        "the Kotlin binding must read the menu from the core over JNI"
    );
    assert!(
        ios_binding().contains("werust_ios_menu_json()"),
        "the Swift binding must read the menu from the core over the C-ABI"
    );
}

#[test]
fn no_edge_hardcodes_a_version_string() {
    // Criterion 2: the version comes from ONE place — `werust_core::version()` —
    // reached directly on desktop and over the FFI on mobile. An edge-local
    // literal (or the Gradle `versionName` / the iOS bundle version) would let the
    // three menus disagree, which is precisely what this criterion forbids.
    let desktop = desktop_shell();
    assert!(
        desktop.contains("werust_core::version()"),
        "desktop must read the one shared version source"
    );
    assert!(
        !desktop.contains("env!(\"CARGO_PKG_VERSION\")"),
        "the desktop shell must not re-read CARGO_PKG_VERSION itself: the ONE source is \
         `werust_core::version()` (this is exactly the second reader that would drift)"
    );

    assert!(
        android_binding().contains("fun version(): String = nativeVersion()"),
        "the Kotlin binding must read the version from the core over JNI"
    );
    assert!(
        !android_activity().contains("BuildConfig.VERSION_NAME")
            && !android_activity().contains("versionName"),
        "the Android edge must not show the Gradle versionName: it would drift from the core"
    );

    assert!(
        ios_binding().contains("werust_ios_version()"),
        "the Swift binding must read the version from the core over the C-ABI"
    );
    assert!(
        !ios_controller().contains("CFBundleShortVersionString"),
        "the iOS edge must not show the bundle version: it would drift from the core"
    );

    // And the core's own accessor really resolves a version rather than shipping
    // the un-injected `0.0.0` placeholder every menu would then display
    // (`crates/werust-core/build.rs`; the resolution rules are unit-tested in
    // `crates/werust-core/src/version_resolution.rs`).
    assert!(!werust_core::version().is_empty());
    assert_ne!(
        werust_core::version(),
        "0.0.0",
        "the one version source must resolve a REAL version, or all three menus \
         confidently show `werust 0.0.0`"
    );
}

#[test]
fn every_edge_routes_the_debug_item_to_an_open_debug_view_hook() {
    // Criterion 3: the Debug entry OPENS the debug view, wired to an
    // open-debug-view HOOK the debug-view tasks fill (this menu task lands FIRST —
    // those tasks are blockedBy it). Each edge must therefore have exactly one
    // named hook, dispatched from the item's STABLE core id (never its label, which
    // is display text).
    let desktop = desktop_shell();
    assert!(
        desktop.contains("fn open_debug_view(") && desktop.contains("open_debug_view(&window)"),
        "desktop must route the Debug item to a named open_debug_view hook"
    );
    assert!(
        desktop.contains("if id == MENU_ITEM_DEBUG"),
        "desktop must dispatch on the item's stable core id, not its label"
    );

    let android = android_activity();
    assert!(
        android.contains("private fun openDebugView()") && android.contains("openDebugView()"),
        "Android must route the Debug item to a named openDebugView hook"
    );
    assert!(
        android.contains("WerustCore.Menu.ITEM_DEBUG ->"),
        "Android must dispatch on the item's stable core id, not its label"
    );

    let ios = ios_controller();
    assert!(
        ios.contains("private func openDebugView()") && ios.contains("openDebugView()"),
        "iOS must route the Debug item to a named openDebugView hook"
    );
    assert!(
        ios.contains("if id == WerustCore.Menu.itemDebug"),
        "iOS must dispatch on the item's stable core id, not its label"
    );

    // The hook is a hook, not a silent no-op: until the view exists each edge says
    // so, so the entry has an honest visible effect. (The desktop wording itself is
    // pinned by the desktop unit test
    // `the_debug_entry_hook_states_the_view_is_not_built_yet_rather_than_doing_nothing`.)
    //
    // Bounded to each hook's OWN body (brace-matched, so it stops at the hook's
    // closing brace and EXCLUDES the doc comment above it), or emptying the hook
    // would still pass on the doc comment's copy of the phrase — the vacuity the
    // sibling system-Back guard learned to avoid.
    for (name, src, signature) in [
        (
            "desktop",
            desktop.as_str(),
            "fn debug_view_placeholder_message() -> String",
        ),
        ("Android", android.as_str(), "private fun openDebugView()"),
        ("iOS", ios.as_str(), "private func openDebugView()"),
    ] {
        let body = block_body(src, signature);
        assert!(
            body.contains("is not built yet"),
            "{name}'s open-debug-view hook must state honestly that the view is not built yet, \
             rather than being a silent no-op; its body is instead: {body:?}"
        );
    }
}

/// The BODY of a brace-delimited declaration: the text between the braces of the
/// block that opens after `signature`, bounded at its MATCHING closing brace.
///
/// POSITION-bounded (not "up to the next declaration") for the reason the sibling
/// `crates/werust-android/rust/tests/system_back_wiring_shape.rs` records: a
/// kind-ordered terminator can overshoot and make an assertion vacuous. Here it
/// also matters that the body EXCLUDES the doc comment above the signature, which
/// legitimately quotes the same wording. `//` line comments, `/* */` block
/// comments and `"` / `"""` string literals are skipped, so a brace inside one
/// (a Kotlin `"${…}"` template, a Swift `"\(…)"` interpolation) cannot unbalance
/// the count. Works for Rust, Kotlin and Swift alike — all three are
/// brace-delimited with the same comment/string forms.
fn block_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let bytes = source.as_bytes();
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("the source must declare `{signature}`"));
    let open = find_from(bytes, start + signature.len(), b"{")
        .unwrap_or_else(|| panic!("`{signature}` must open a block"));

    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        let tail = &bytes[i..];
        if tail.starts_with(b"//") {
            i = find_from(bytes, i, b"\n").unwrap_or(bytes.len());
            continue;
        }
        if tail.starts_with(b"/*") {
            i = find_from(bytes, i + 2, b"*/").map_or(bytes.len(), |e| e + 2);
            continue;
        }
        if tail.starts_with(b"\"\"\"") {
            i = find_from(bytes, i + 3, b"\"\"\"").map_or(bytes.len(), |e| e + 3);
            continue;
        }
        if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                i += if bytes[i] == b'\\' { 2 } else { 1 };
            }
            i += 1;
            continue;
        }
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    // `open` and `i` are ASCII brace positions, so these are char
                    // boundaries even though the sources contain multi-byte glyphs.
                    return &source[open + 1..i];
                }
            }
            _ => {}
        }
        i += 1;
    }
    panic!("`{signature}` opens a block that is never closed")
}

/// The first occurrence of `pat` in `bytes` at or after `from`, as an absolute
/// index. Byte-wise (never slices the `str`) so the scan can walk sources holding
/// multi-byte characters (`⋮`, `—`) without landing mid-char.
fn find_from(bytes: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if from >= bytes.len() {
        return None;
    }
    bytes[from..]
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|i| from + i)
}

#[test]
fn the_block_extractor_excludes_the_doc_comment_and_stops_at_the_matching_brace() {
    // The guard ON the guard: `block_body` is what makes the hook assertion above
    // mean anything. The trap here is specifically the DOC COMMENT, which quotes
    // the same wording the hook's string literal carries — an unbounded
    // `src.contains(…)` stays green over an EMPTIED hook. The extractor must also
    // stop at the hook's own closing brace rather than running into the next
    // member, and must not be unbalanced by a brace inside a comment or a string
    // template.
    let fixture = "\
 class Fixture {
    /** Says it is not built yet. */
    private fun openDebugView() {
        toast(\"werust ${v()} — not built yet.\")
    }

    private fun other() {
        val decoy = \"not built yet\"
    }
}
";
    let body = block_body(fixture, "private fun openDebugView()");
    assert!(body.contains("not built yet"), "{body:?}");
    assert!(
        !body.contains("decoy"),
        "the body must stop at the hook's matching brace: {body:?}"
    );

    // EMPTYING the hook must extract as empty, even though the doc comment above
    // it (and a later member) still carry the phrase — that is what makes the hook
    // assertion non-vacuous.
    let emptied = fixture.replace("        toast(\"werust ${v()} — not built yet.\")\n", "");
    assert!(
        !block_body(&emptied, "private fun openDebugView()").contains("not built yet"),
        "an emptied hook must extract as an empty body"
    );

    // Braces inside comments and strings must not end the body early or late.
    let tricky = "\
fn sample() {
    // a brace in a comment: }
    /* and a block one: } */
    let s = \"a literal brace }\";
    let marker = 1;
}

fn after() {
    let outside = 2;
}
";
    let body = block_body(tricky, "fn sample()");
    assert!(
        body.contains("marker") && !body.contains("outside"),
        "{body:?}"
    );
}

#[test]
fn the_menu_is_never_debug_build_gated_on_any_edge() {
    // Criterion 4: the menu is a USER-FACING feature, always available — NOT
    // debug-build-gated. Each platform already HAS a debug gate for the native
    // remote inspector (desktop `developer_extras`, Android
    // `FLAG_DEBUGGABLE`, iOS `#if DEBUG`), and reusing one of those for the menu is
    // the plausible mistake this pins against: the menu-building code must contain
    // no such gate.
    //
    // Bounded to the menu-building code (not the whole file, which legitimately
    // gates the INSPECTOR) by slicing from the menu builder's declaration to its
    // dispatch hook.
    let desktop = desktop_shell();
    let desktop_menu = between(&desktop, "fn build_menu_button(", "\nfn main(");
    assert!(
        !desktop_menu.contains("debug_assertions"),
        "the desktop menu must not be gated on a debug build: {desktop_menu:?}"
    );

    let android = android_activity();
    let android_menu = between(
        &android,
        "private fun showBrowserMenu()",
        "private fun openDebugView",
    );
    assert!(
        !android_menu.contains("FLAG_DEBUGGABLE") && !android_menu.contains("BuildConfig.DEBUG"),
        "the Android menu must not be gated on a debug build: {android_menu:?}"
    );

    let ios = ios_controller();
    let ios_menu = between(
        &ios,
        "private func browserMenu()",
        "private func openDebugView",
    );
    assert!(
        !ios_menu.contains("#if DEBUG"),
        "the iOS menu must not be gated on a debug build: {ios_menu:?}"
    );
}

/// The slice of `source` from the start of `from` up to (not including) `to`.
/// Both markers must be present and in order, so a rename cannot silently make a
/// bounded assertion vacuous by matching an empty slice.
fn between<'a>(source: &'a str, from: &str, to: &str) -> &'a str {
    let start = source
        .find(from)
        .unwrap_or_else(|| panic!("the source must contain `{from}`"));
    let end = source[start..]
        .find(to)
        .unwrap_or_else(|| panic!("the source must contain `{to}` after `{from}`"));
    &source[start..start + end]
}

#[test]
fn the_slice_helper_is_bounded_so_the_gating_assertions_are_not_vacuous() {
    // The guard ON the guard (the sibling system-Back guard learned this the hard
    // way: a mis-bounded extractor made its assertion pass over an EMPTY handler).
    // `between` must stop at its end marker, and must panic rather than silently
    // return an empty slice if a marker is renamed away.
    let fixture = "fn a() { keep }\nfn b() { drop }\n";
    let slice = between(fixture, "fn a()", "fn b()");
    assert!(
        slice.contains("keep"),
        "the slice keeps its own body: {slice:?}"
    );
    assert!(
        !slice.contains("drop"),
        "the slice stops at the end marker: {slice:?}"
    );

    // A missing marker is a PANIC, not an empty (vacuously-passing) slice.
    assert!(
        std::panic::catch_unwind(|| between(fixture, "fn missing()", "fn b()")).is_err(),
        "a renamed start marker must fail loudly"
    );
    assert!(
        std::panic::catch_unwind(|| between(fixture, "fn a()", "fn missing()")).is_err(),
        "a renamed end marker must fail loudly"
    );
}
