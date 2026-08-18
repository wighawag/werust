//! Settings-mutation USER-INTENT edge-wiring shape guard (task
//! `settings-mutation-requires-marked-user-intent-in-core-and-on-gtk`, spec
//! `settings-mutations-require-user-intent`, `docs/adr/0013`).
//!
//! WHAT LANDED: a `werust://settings` MUTATION is applied only for a MAIN-FRAME
//! request whose navigation werust's own chrome MARKED as intended
//! (`werust_core::intent::NavigationIntent`). A request that fails the gate still
//! renders the page, read-only, with its real current values. READS are not gated.
//!
//! WHY A SOURCE-SHAPE GUARD: the RULE is unit-tested where it lives
//! (`werust_core::intent` and `werust_core::retrieval`'s gate tests). What is not
//! otherwise assertable is that each OS EDGE actually SUPPLIES the mark at its own
//! navigation hook and supplies it NOWHERE ELSE — and the marking is the whole
//! authorisation, so an edge that marks too eagerly (from the `_blank` hook, or
//! from any page-started navigation) silently reopens the hole the gate exists to
//! close, with every test still green. Four of the five edges are also
//! unreachable from this gate (macOS/Windows build only on their own OS, the
//! Android JNI module is `cfg(target_os = "android")`, iOS ships Swift), which is
//! the same reason `debug_capture_edge_wiring_shape.rs` and
//! `pin_store_edge_wiring_shape.rs` parse their edges rather than call them.
//!
//! It lives in `werust-core` (not one edge's crate) because it spans every edge,
//! and `werust-core` is the one crate they all sit over.
//!
//! # This file is APPENDED to, never rewritten
//!
//! The GTK block below is the only one today. The four sibling edge tasks
//! (`macos-`, `windows-`, `ios-`, `android-marks-user-intent-for-settings-mutations`)
//! each APPEND their own block at the end, in the same shape: a section banner,
//! that edge's source accessors, and its own `#[test]`s. Do not rewrite a
//! sibling's block, and do not fold two edges into one test — the blocks are
//! separate so four tasks can land in any order with a one-hunk diff each.

use std::path::{Path, PathBuf};

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/werust-core`, so the root is two levels up.
fn source(relative: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The slice of `text` that FOLLOWS `marker`, bounded to `len` bytes: enough to
/// read one wiring site without pinning its exact formatting (which `cargo fmt`
/// owns).
fn after(text: &str, marker: &str, len: usize) -> String {
    let start = text
        .find(marker)
        .unwrap_or_else(|| panic!("the source no longer contains `{marker}`"));
    let rest = &text[start..];
    rest[..len.min(rest.len())].to_string()
}

/// The slice of `text` from `marker` up to the next `end` after it: one function
/// body, for asserting what a hook does NOT do.
fn between(text: &str, marker: &str, end: &str) -> String {
    let body = after(text, marker, text.len());
    match body[marker.len()..].find(end) {
        Some(at) => body[..marker.len() + at].to_string(),
        None => body,
    }
}

// ---------------------------------------------------------------------------
// SHARED: what every edge inherits, whatever it wires.
// ---------------------------------------------------------------------------

/// The one call that decides whether a request may mutate. It belongs to the
/// CORE: an edge supplies a signal and decides nothing.
const THE_DECISION: &str = "take_mark_for";

/// The one call an edge may make: leaving the mark.
const THE_SIGNAL: &str = ".mark(";

#[test]
fn the_seam_request_still_carries_only_a_uri() {
    // The tempting fix is to widen `renderer::SchemeRequest` with an
    // is-main-frame / is-user-initiated flag. It must NOT be widened, for two
    // separate reasons:
    //
    // 1. anything the REQUEST carries is under page control at the point the gate
    //    reads it, so a flag there would be a claim, not an authorisation; and
    // 2. that struct is constructed by edge halves this Linux gate cannot compile
    //    (macOS, Windows, iOS, and an Android leg that does not exist yet), so
    //    widening it is an invisible break whose only detectors are three
    //    workflows.
    //
    // The intent is therefore carried CORE-side, out of band. This assertion is
    // what keeps a later change from quietly taking the other road.
    let seam = source("crates/renderer/src/lib.rs");
    let request = between(&seam, "pub struct SchemeRequest {", "}");
    let fields: Vec<&str> = request
        .lines()
        // A FIELD, not the `pub struct` line the slice opens with.
        .filter(|line| line.trim_start().starts_with("pub ") && line.contains(':'))
        .collect();
    assert_eq!(
        fields.len(),
        1,
        "`SchemeRequest` must carry the URI and nothing else, or the intent has \
         become something the request claims: {request}"
    );
    assert!(
        fields[0].contains("uri: String"),
        "the one field is the URI: {request}"
    );
}

#[test]
fn only_the_core_decides_whether_a_marked_navigation_may_mutate() {
    // Every edge is a SIGNAL SOURCE: it may leave a mark, and it may not read one
    // and act on it. Concentrating the decision is what lets the rule be tested
    // once (in `werust_core::intent` + `werust_core::retrieval`) rather than five
    // times, four of them on hardware this project does not have.
    for edge in [
        "crates/webview-renderer/src/backend.rs",
        "crates/werust/src/main.rs",
        // Android (task `android-marks-user-intent-for-settings-mutations`): the
        // Rust edge and the Kotlin shell over it. The Kotlin file is listed for
        // the same reason its own block below exists — that layer is a signal
        // source, and a `take_mark_for` there would be it deciding.
        "crates/werust-android/rust/src/lib.rs",
        "crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt",
        // iOS (task `ios-marks-user-intent-for-settings-mutations`): the Rust edge
        // and the two Swift files over it (the shell that reports a navigation,
        // and the C-ABI binding it reports through). Listed for the same reason
        // the Kotlin file is: those layers are signal sources, and a
        // `take_mark_for` there would be them deciding.
        "crates/werust-ios/rust/src/lib.rs",
        "crates/werust-ios/App/Sources/WKWebViewShellController.swift",
        "crates/werust-ios/App/Sources/WerustCore.swift",
        // A sibling edge task APPENDS its own edge files here as it lands.
    ] {
        assert!(
            !source(edge).contains(THE_DECISION),
            "{edge} reads the intent mark itself; the decision belongs to the core \
             (`retrieval::apply_settings_request_with_intent`)"
        );
    }
}

// ---------------------------------------------------------------------------
// EDGE: desktop GTK / WebKitGTK (task
// `settings-mutation-requires-marked-user-intent-in-core-and-on-gtk`).
// ---------------------------------------------------------------------------

fn gtk_backend() -> String {
    source("crates/webview-renderer/src/backend.rs")
}

fn gtk_window() -> String {
    source("crates/werust/src/main.rs")
}

#[test]
fn gtk_serves_the_settings_page_through_the_gated_core_entry_point() {
    // The `werust://` scheme handler fires for the main document AND every
    // sub-resource, so it is where the gate has to be consulted. Routing through
    // the UNGATED `apply_settings_request` instead would leave the hole open with
    // every test still green, because that entry point still renders the page.
    let handler = after(
        &gtk_backend(),
        "register_uri_scheme(werust_core::retrieval::WERUST_SCHEME",
        700,
    );
    assert!(
        handler.contains("apply_settings_request_with_intent"),
        "the GTK settings handler must consult the intent carrier:\n{handler}"
    );
}

#[test]
fn gtk_hands_the_one_carrier_to_both_the_handler_and_the_shell() {
    // Two halves, ONE carrier: the shell marks what the chrome starts (the URL
    // bar's Enter), this edge marks a link/form navigation started inside werust's
    // own page, and the scheme handler reads both. A window that built the carrier
    // and forgot to hand it to the shell would refuse the user's own URL-bar
    // change — the failure this assertion catches, since it is invisible to a
    // headless gate.
    let window = gtk_window();
    let wiring = after(&window, "backend.install_settings_page(", 200);
    assert!(
        wiring.contains("&redirects"),
        "the settings page is wired with the ONE main-frame predicate (the redirect \
         sink), not a second notion of it:\n{wiring}"
    );
    assert!(
        window.contains(".with_navigation_intent("),
        "the GTK window must hand the carrier to the shell, or a URL-bar-committed \
         settings change is refused"
    );
}

#[test]
fn gtk_marks_only_a_navigation_activated_inside_a_surface_werust_drew() {
    // The mark is the whole authorisation, so WHAT it is derived from is the
    // security property. Both facts are required and neither is forgeable by a
    // page: the navigation was ACTIVATED in the page (a link click / form submit,
    // never a script's `location = …`, which reports `Other`), and the document it
    // starts FROM is one werust itself rendered (a `werust://` URL, which web
    // content can never be at).
    let hook = between(
        &gtk_backend(),
        "self.view.connect_decide_policy(",
        "\n    /// ",
    );
    assert!(
        hook.contains("NavigationType::LinkClicked")
            && hook.contains("NavigationType::FormSubmitted"),
        "the mark must require a navigation the user ACTIVATED in the page:\n{hook}"
    );
    assert!(
        hook.contains("!action.is_redirect()"),
        "and must exclude a REDIRECT of one, which re-fires this callback carrying \
         the original navigation type while the view's active URI is already \
         moving:\n{hook}"
    );
    assert!(
        hook.matches("WERUST_URL_PREFIX").count() >= 2,
        "the mark must require BOTH the document it starts from and its target to \
         be werust's own internal page:\n{hook}"
    );
    assert!(
        hook.contains(THE_SIGNAL),
        "and it must actually leave the mark:\n{hook}"
    );
}

#[test]
fn gtk_never_marks_the_blank_and_window_open_path() {
    // The counter-example the spec names. A `target="_blank"` link and
    // `window.open(url)` are navigations a PAGE chooses, and this hook loads that
    // target straight into the existing view (`docs/adr/0010`), deliberately
    // bypassing the shell. Marking there would hand any page a settings write with
    // one `window.open('werust://settings?backend=custom&url=http://attacker/')`.
    let hook = between(
        &gtk_backend(),
        "pub fn install_new_window_in_place(",
        "\n    /// ",
    );
    assert!(
        hook.contains("load_uri"),
        "the slice must really be the in-place load hook, or this test passes \
         vacuously:\n{hook}"
    );
    assert!(
        !hook.contains(THE_SIGNAL),
        "the new-window in-place hook must not mark user intent:\n{hook}"
    );
    assert!(!hook.contains(THE_DECISION), "nor read one:\n{hook}");
}

// ---------------------------------------------------------------------------
// EDGE: Android / the System WebView (task
// `android-marks-user-intent-for-settings-mutations`).
//
// Android is TWO files: the Rust edge (`werust-android-core`, which owns the
// scheme handler, the carrier and the marking RULE, and is gate-compiled — its
// unit tests run in this same `cargo test`) and the Kotlin shell (which reports
// the per-request facts and is reachable from this gate only by parsing, since
// the Gradle/Kotlin build is not in `verify` at all). Both are asserted here.
// ---------------------------------------------------------------------------

fn android_rust_edge() -> String {
    source("crates/werust-android/rust/src/lib.rs")
}

fn android_kotlin_shell() -> String {
    source("crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt")
}

/// The BODY of a Kotlin declaration: the text between the braces of the block
/// that opens after `signature`, bounded at its MATCHING closing brace.
///
/// Brace-matched rather than bounded by the next member declaration, which is the
/// lesson `crates/werust-android/rust/tests/system_back_wiring_shape.rs` records:
/// a kind-ordered terminator search silently swallowed a whole later method and
/// made that guard VACUOUS. Here it matters just as much, because the negative
/// assertions below ("this hook does NOT mark") are exactly the kind that pass
/// for free on a mis-bounded slice. `//` and `/* */` comments and `"`/`"""`
/// literals are skipped so a brace inside one cannot unbalance the count.
fn kotlin_block_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let bytes = source.as_bytes();
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("the Kotlin source must declare `{signature}`"));
    let find_from = |from: usize, pat: &[u8]| -> Option<usize> {
        if from >= bytes.len() {
            return None;
        }
        bytes[from..]
            .windows(pat.len())
            .position(|w| w == pat)
            .map(|i| from + i)
    };
    let open = find_from(start + signature.len(), b"{")
        .unwrap_or_else(|| panic!("`{signature}` must open a block"));

    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        let tail = &bytes[i..];
        if tail.starts_with(b"//") {
            i = find_from(i, b"\n").unwrap_or(bytes.len());
            continue;
        }
        if tail.starts_with(b"/*") {
            i = find_from(i + 2, b"*/").map_or(bytes.len(), |e| e + 2);
            continue;
        }
        if tail.starts_with(b"\"\"\"") {
            i = find_from(i + 3, b"\"\"\"").map_or(bytes.len(), |e| e + 3);
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
                    // Both are ASCII brace positions, so these are char boundaries.
                    return &source[open + 1..i];
                }
            }
            _ => {}
        }
        i += 1;
    }
    panic!("`{signature}` opens a block that is never closed")
}

/// The ONE call the Kotlin shell may make about a navigation: reporting it.
const THE_ANDROID_KOTLIN_SIGNAL: &str = "core.notePageNavigation(";

#[test]
fn android_serves_the_settings_page_through_the_gated_core_entry_point() {
    // Android's `shouldInterceptRequest` fires for the main document AND every
    // sub-resource (this edge sees more requests than any other), so the handler
    // is where the gate has to be consulted. Routing through the UNGATED
    // `apply_settings_request` instead — which is what this edge did before, and
    // what it would fall back to on any refactor — leaves the hole open with every
    // test still green, because that entry point still renders the page.
    let edge = android_rust_edge();
    let handler = after(
        &edge,
        "backend.register_scheme_handler(\n        WERUST_SCHEME",
        300,
    );
    assert!(
        handler.contains("apply_settings_request_with_intent"),
        "the Android settings handler must consult the intent carrier:\n{handler}"
    );
}

#[test]
fn android_hands_the_one_carrier_to_both_the_handler_and_the_shell() {
    // Two halves, ONE carrier: the shell marks what the chrome starts (the URL
    // bar's Enter, through `BrowserShell::navigate`), the Kotlin navigation hook
    // marks a form submit inside werust's own page, and the scheme handler reads
    // both. A session that built the carrier for its handler and forgot to hand it
    // to the shell would refuse the user's own URL-bar change — invisible to a
    // headless gate, which is why it is asserted here as well as unit-tested in
    // `werust-android-core`.
    let edge = android_rust_edge();
    let wiring = after(&edge, "let intent = install_settings_page(", 200);
    assert!(
        wiring.contains("&redirects"),
        "the settings page is wired with the ONE main-frame predicate (the redirect \
         sink `install_ipfs` returned), not a second notion of it:\n{wiring}"
    );
    assert!(
        edge.contains(".with_navigation_intent("),
        "the Android session must hand the carrier to the shell, or a URL-bar-committed \
         settings change is refused"
    );
}

#[test]
fn android_marks_only_a_navigation_activated_inside_a_surface_werust_drew() {
    // The mark is the whole authorisation, so WHAT it is derived from is the
    // security property. The Android facts are its own vocabulary
    // (`isForMainFrame` / `hasGesture()` / `isRedirect`), but the shape is the one
    // every edge inherits: the navigation was ACTIVATED in the page, it is not a
    // REDIRECT of one, and the document it starts FROM is a `werust://` page — a
    // surface werust itself drew, which web content can never be at.
    let rule = between(
        &android_rust_edge(),
        "    pub fn note_page_navigation(",
        "\n    /// ",
    );
    assert!(
        rule.contains("user_gesture") && rule.contains("!redirect"),
        "the mark must require a navigation the user ACTIVATED in the page, and must \
         exclude a REDIRECT of one:\n{rule}"
    );
    assert!(
        rule.contains("main_frame"),
        "and must require the MAIN frame:\n{rule}"
    );
    assert!(
        rule.matches("WERUST_URL_PREFIX").count() >= 2,
        "the mark must require BOTH the document it starts from and its target to be \
         werust's own internal page:\n{rule}"
    );
    assert!(
        rule.contains(THE_SIGNAL),
        "and it must actually leave the mark:\n{rule}"
    );
}

#[test]
fn the_android_kotlin_shell_reports_the_facts_and_decides_nothing() {
    // The discipline this edge is held to everywhere else (it READS
    // `werust_core::chrome_json` rather than re-deriving the chrome,
    // `docs/adr/0011`), applied to the authorisation: Kotlin hands over the facts
    // its callback was given and the Rust side decides. A Kotlin-side `if` that
    // decided when to mark would be the same hand-written twin — in the one place
    // where a drifted copy is a security hole rather than a wrong glyph.
    let shell = android_kotlin_shell();
    let hook = kotlin_block_body(
        &shell,
        "override fun shouldOverrideUrlLoading(\n            view: WebView,\n            request: WebResourceRequest,\n        ): Boolean",
    );
    assert!(
        hook.contains(THE_ANDROID_KOTLIN_SIGNAL),
        "the navigation hook must report the navigation to the core:\n{hook}"
    );
    for fact in [
        "request.url",
        "view.url",
        "request.isForMainFrame",
        "request.hasGesture()",
        "isRedirectOrUnknown(request)",
    ] {
        assert!(
            hook.contains(fact),
            "the hook must report `{fact}`, one of the facts the mark is derived \
             from:\n{hook}"
        );
    }
    assert!(
        !hook.contains("if (") && !hook.contains("when ("),
        "the hook must not decide WHETHER to report: the rule lives in the Rust edge \
         (`IntentMarker::note_page_navigation`), where the gate can test it:\n{hook}"
    );
    assert!(
        hook.contains("return false"),
        "and it must stay READ-ONLY observation (the WebView performs the navigation \
         exactly as it did before this hook existed):\n{hook}"
    );
}

#[test]
fn android_never_marks_the_blank_and_window_open_path() {
    // The counter-example the spec names, on the edge whose version of it is a
    // whole second WebView. `onCreateWindow` recovers the `_blank`/`window.open`
    // target through a throwaway transport WebView and loads it into the main one
    // (`docs/adr/0010`), deliberately bypassing the shell. That target is a URL the
    // PAGE chose, so neither the hook nor its transport client may report a
    // navigation: marking there would hand any page a settings write with one
    // `window.open('werust://settings?backend=custom&url=http://attacker/')`.
    let shell = android_kotlin_shell();
    let hook = kotlin_block_body(
        &shell,
        "override fun onCreateWindow(\n            view: WebView,\n            isDialog: Boolean,\n            isUserGesture: Boolean,\n            resultMsg: Message,\n        ): Boolean",
    );
    assert!(
        hook.contains("webView.loadUrl("),
        "the slice must really be the in-place routing hook, or this test passes \
         vacuously:\n{hook}"
    );
    assert!(
        hook.contains("shouldOverrideUrlLoading"),
        "including its transport WebView's own client, which is where a second \
         marking site would hide:\n{hook}"
    );
    assert!(
        !hook.contains(THE_ANDROID_KOTLIN_SIGNAL),
        "the new-window in-place hook must not report a navigation as intent:\n{hook}"
    );
    assert!(!hook.contains(THE_DECISION), "nor read a mark:\n{hook}");
}

#[test]
fn the_kotlin_block_extractor_stops_at_the_matching_brace() {
    // The guard ON the guard: the two negative assertions above ("this hook does
    // NOT mark") are worthless if the slice is empty or mis-bounded, so the
    // extractor is pinned on a fixture shaped like the trap that once made a
    // sibling Kotlin guard vacuous — a short `override fun` whose next member is
    // another `override fun`, with the decoy call further down.
    let fixture = "\
class Fixture {
    override fun onCreateWindow(): Boolean {
        transport.webViewClient = object : WebViewClient() { }
        return true
    }

    override fun shouldOverrideUrlLoading(): Boolean {
        core.notePageNavigation(\"a\", \"b\", true, true, false)
        return false
    }
}
";
    let routing = kotlin_block_body(fixture, "override fun onCreateWindow(): Boolean");
    assert!(
        routing.contains("transport.webViewClient"),
        "the extracted body is the hook's own: {routing:?}"
    );
    assert!(
        !routing.contains(THE_ANDROID_KOTLIN_SIGNAL),
        "and it STOPS at the matching brace rather than running on into the marking \
         hook below it (the vacuity this pins): {routing:?}"
    );
    let marking = kotlin_block_body(fixture, "override fun shouldOverrideUrlLoading(): Boolean");
    assert!(marking.contains(THE_ANDROID_KOTLIN_SIGNAL));

    // Braces inside comments and string literals must not unbalance the count.
    let tricky = "\
    private fun sample() {
        // a brace in a comment: }
        /* and a block one: } */
        val s = \"a literal brace }\"
        val t = \"\"\"a raw one }\"\"\"
        val marker = 1
    }

    private fun after() {
        val outside = 2
    }
";
    let body = kotlin_block_body(tricky, "private fun sample()");
    assert!(
        body.contains("val marker = 1") && !body.contains("val outside = 2"),
        "braces inside comments/strings must not end the body early or late; \
         extracted: {body:?}"
    );
}

// ---------------------------------------------------------------------------
// EDGE: iOS / WKWebView (task `ios-marks-user-intent-for-settings-mutations`).
//
// iOS is THREE files: the Rust edge (`werust-ios-core`, which owns the scheme
// handler registration, the carrier and the marking RULE, and is gate-compiled:
// its unit tests run in this same `cargo test`), the Swift shell (which reports
// the per-navigation facts `WKNavigationAction` carries and routes a
// `_blank`/`window.open` target in place), and the Swift C-ABI binding between
// them. The two Swift files are reachable from this gate only by PARSING, since
// no Xcode/SDK exists here, and nobody on this project has a Mac to notice a
// regression by using the app
// (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`),
// which is why the Swift half is pinned rather than argued.
// ---------------------------------------------------------------------------

fn ios_rust_edge() -> String {
    source("crates/werust-ios/rust/src/lib.rs")
}

fn ios_swift_shell() -> String {
    source("crates/werust-ios/App/Sources/WKWebViewShellController.swift")
}

/// The BODY of a Swift declaration: the text between the braces of the block that
/// opens after `signature`, bounded at its MATCHING closing brace.
///
/// Swift and Kotlin share exactly the lexical subset this scan needs (`//` and
/// `/* */` comments, `"` and `"""` literals, `{}` blocks), so this DELEGATES to
/// the extractor the Android block above already carries (which has its own
/// regression guard, `the_kotlin_block_extractor_stops_at_the_matching_brace`)
/// rather than landing a second copy of a tricky scanner in the same file. It is
/// edge-prefixed because this file is APPENDED to: a sibling Swift edge (macOS)
/// lands its own helpers here, and two `swift_block_body` definitions would not
/// compile.
fn ios_swift_block_body<'a>(source: &'a str, signature: &str) -> &'a str {
    assert!(
        source.contains(signature),
        "the Swift source must declare `{signature}`"
    );
    kotlin_block_body(source, signature)
}

/// The ONE call the Swift shell may make about a navigation: reporting it.
const THE_IOS_SWIFT_SIGNAL: &str = "core.notePageNavigation(";

#[test]
fn ios_serves_the_settings_page_through_the_gated_core_entry_point() {
    // A `WKWebView` hands a custom scheme ONLY to a registered
    // `WKURLSchemeHandler`, and that handler is asked for the main document AND
    // every sub-resource, so the registration is where the gate has to be
    // consulted. Routing through the UNGATED `apply_settings_request` instead
    // (which is what this edge did before) leaves the hole open with every test
    // still green, because that entry point still renders the page.
    let edge = ios_rust_edge();
    let handler = after(
        &edge,
        "    backend.register_scheme_handler(\n        WERUST_SCHEME",
        300,
    );
    assert!(
        handler.contains("apply_settings_request_with_intent"),
        "the iOS settings handler must consult the intent carrier:\n{handler}"
    );
}

#[test]
fn ios_hands_the_one_carrier_to_both_the_handler_and_the_shell() {
    // Two halves, ONE carrier: the shell marks what the chrome starts (the URL
    // bar's Enter, through `BrowserShell::navigate`), the Swift navigation-policy
    // hook marks a form submit inside werust's own page, and the scheme handler
    // reads both. A session that built the carrier for its handler and forgot to
    // hand it to the shell would refuse the user's own URL-bar change, invisible
    // to a headless gate and, on this edge, to every human as well.
    let edge = ios_rust_edge();
    let wiring = after(&edge, "let intent = install_settings_page(", 200);
    assert!(
        wiring.contains("&redirects"),
        "the settings page is wired with the ONE main-frame predicate (the redirect \
         sink `install_ipfs` returned), not a second notion of it:\n{wiring}"
    );
    assert!(
        edge.contains(".with_navigation_intent("),
        "the iOS session must hand the carrier to the shell, or a URL-bar-committed \
         settings change is refused"
    );
}

#[test]
fn ios_marks_only_a_navigation_activated_inside_a_surface_werust_drew() {
    // The mark is the whole authorisation, so WHAT it is derived from is the
    // security property. The iOS facts are WebKit's own vocabulary (the
    // `WKNavigationType`, the target frame, the source frame's document), but the
    // shape is the one every edge inherits: the navigation was ACTIVATED in the
    // page, it targets the MAIN frame, and the document it starts FROM is a
    // `werust://` page: a surface werust itself drew, which web content can never
    // be at.
    let rule = between(
        &ios_rust_edge(),
        "    pub fn note_page_navigation(",
        "\n    /// ",
    );
    assert!(
        rule.contains("WK_NAVIGATION_TYPE_LINK_ACTIVATED")
            && rule.contains("WK_NAVIGATION_TYPE_FORM_SUBMITTED"),
        "the mark must require a navigation the user ACTIVATED in the page (a \
         script's `location = …` reports `.other`):\n{rule}"
    );
    assert!(
        rule.contains("main_frame"),
        "and must require the MAIN frame:\n{rule}"
    );
    // Named per SIDE rather than counted: the `use` line also mentions the
    // constant, so a count of two is satisfied by the import plus ONE check, and
    // dropping the DOCUMENT side is exactly the mutation that reopens the hole.
    for side in [
        "document.starts_with(WERUST_URL_PREFIX)",
        "target.starts_with(WERUST_URL_PREFIX)",
    ] {
        assert!(
            rule.contains(side),
            "the mark must require `{side}`: BOTH the document the navigation starts \
             from and its target must be werust's own internal page:\n{rule}"
        );
    }
    assert!(
        rule.contains(THE_SIGNAL),
        "and it must actually leave the mark:\n{rule}"
    );
}

#[test]
fn the_ios_swift_shell_reports_the_facts_and_decides_nothing() {
    // The discipline this edge is held to everywhere else (it READS
    // `werust_core::chrome_json` rather than re-deriving the chrome,
    // `docs/adr/0011`), applied to the authorisation: Swift hands over the facts
    // its callback was given and the Rust side decides. A Swift-side condition
    // choosing when to report would be the same hand-written twin, in the one
    // place where a drifted copy is a security hole rather than a wrong glyph.
    let shell = ios_swift_shell();
    let hook = ios_swift_block_body(
        &shell,
        "func webView(\n        _ wv: WKWebView,\n        decidePolicyFor navigationAction: WKNavigationAction,",
    );
    let report = hook.find(THE_IOS_SWIFT_SIGNAL).unwrap_or_else(|| {
        panic!("the navigation hook must report the navigation to the core:\n{hook}")
    });
    for fact in [
        "navigationAction.request.url",
        "navigationAction.sourceFrame.request.url",
        "navigationAction.targetFrame?.isMainFrame == true",
        "navigationAction.navigationType.rawValue",
    ] {
        assert!(
            hook.contains(fact),
            "the hook must report `{fact}`, one of the facts the mark is derived \
             from:\n{hook}"
        );
    }
    // The report is UNCONDITIONAL: nothing may decide WHETHER to report, because
    // the rule lives in the Rust edge (`IntentMarker::note_page_navigation`) where
    // the gate can test it. This hook DOES branch afterwards (on `.backForward`,
    // for the edge-swipe report), so the assertion is that no branch precedes the
    // report rather than that the body is branch-free.
    let before_the_report = &hook[..report];
    for decider in ["if ", "guard ", "switch ", "?", "&&", "||"] {
        assert!(
            !before_the_report.contains(decider),
            "nothing may decide whether to report the navigation (`{decider}` \
             appears before it); the rule belongs to the Rust edge:\n{before_the_report}"
        );
    }
    assert!(
        hook.contains("decisionHandler(.allow)"),
        "and the hook must stay READ-ONLY observation (WebKit performs the \
         navigation exactly as it did before this report existed):\n{hook}"
    );
}

#[test]
fn ios_never_marks_the_blank_and_window_open_path() {
    // The counter-example the spec names, in its iOS shape: `WKUIDelegate`'s
    // `createWebViewWith` loads a `_blank`/`window.open` target straight into the
    // existing view (`docs/adr/0010`), deliberately bypassing the shell. That
    // target is a URL the PAGE chose, so this hook must report nothing: reporting
    // there would hand any page a settings write with one
    // `window.open('werust://settings?backend=custom&url=http://attacker/')`.
    let shell = ios_swift_shell();
    let hook = ios_swift_block_body(
        &shell,
        "func webView(\n        _ wv: WKWebView,\n        createWebViewWith configuration: WKWebViewConfiguration,",
    );
    assert!(
        hook.contains("wv.load(navigationAction.request)"),
        "the slice must really be the in-place load hook, or this test passes \
         vacuously:\n{hook}"
    );
    assert!(
        !hook.contains(THE_IOS_SWIFT_SIGNAL),
        "the new-window in-place hook must not report a navigation as intent:\n{hook}"
    );
    assert!(!hook.contains(THE_DECISION), "nor read a mark:\n{hook}");
}

#[test]
fn the_ios_swift_binding_marshals_the_report_and_adds_no_rule() {
    // The layer between the two: `WerustCore.notePageNavigation` is a C-ABI
    // marshalling shim, so it must carry the facts across unchanged. A condition
    // HERE would be as invisible as one in the shell, and it is the more tempting
    // site (it is the file that knows the FFI's shape).
    let binding = source("crates/werust-ios/App/Sources/WerustCore.swift");
    let shim = ios_swift_block_body(
        &binding,
        "func notePageNavigation(\n        target: String, document: String, mainFrame: Bool, navigationType: Int\n    ) -> Bool",
    );
    assert!(
        shim.contains("werust_ios_note_page_navigation("),
        "the binding must call the C-ABI export:\n{shim}"
    );
    for decider in ["if ", "guard ", "switch ", "&&", "||"] {
        assert!(
            !shim.contains(decider),
            "the binding must add no rule of its own (`{decider}`):\n{shim}"
        );
    }
}

#[test]
fn the_ios_swift_block_extractor_stops_at_the_matching_brace() {
    // The guard ON the guard, in this edge's own idiom: the negative assertions
    // above ("this hook does NOT report") are worthless on a mis-bounded slice, and
    // Swift's multi-line signatures are exactly where a naive scan goes wrong: the
    // signature this file matches on spans three lines and ends in a comma, so the
    // brace it opens is the one after the RETURN clause, not the next `{` on the
    // line.
    let fixture = "\
final class Fixture {
    func webView(
        _ wv: WKWebView,
        createWebViewWith configuration: WKWebViewConfiguration,
        for navigationAction: WKNavigationAction
    ) -> WKWebView? {
        wv.load(navigationAction.request)
        return nil
    }

    func webView(
        _ wv: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        core.notePageNavigation(target: \"a\", document: \"b\", mainFrame: true, navigationType: 1)
        decisionHandler(.allow)
    }
}
";
    let routing = ios_swift_block_body(
        fixture,
        "func webView(\n        _ wv: WKWebView,\n        createWebViewWith configuration: WKWebViewConfiguration,",
    );
    assert!(
        routing.contains("wv.load(navigationAction.request)"),
        "the extracted body is the hook's own: {routing:?}"
    );
    assert!(
        !routing.contains(THE_IOS_SWIFT_SIGNAL),
        "and it STOPS at the matching brace rather than running on into the reporting \
         hook below it (the vacuity this pins): {routing:?}"
    );
    let policy = ios_swift_block_body(
        fixture,
        "func webView(\n        _ wv: WKWebView,\n        decidePolicyFor navigationAction: WKNavigationAction,",
    );
    assert!(policy.contains(THE_IOS_SWIFT_SIGNAL));
}
