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
