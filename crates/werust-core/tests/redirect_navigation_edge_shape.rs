//! Edge-shape guard for the `_redirects` 3xx NAVIGATION (task
//! `ipfs-redirects-3xx-navigation-support`, spec
//! `ens-to-ipfs-resolution-phase1-rpc-skeleton`).
//!
//! THE PROBLEM THIS GUARD EXISTS FOR: the whole 3xx DECISION is shared core (a
//! `FallbackAction::Redirect`, an absolute same-root target pushed into a
//! `RedirectSink`, a bounded chain, and `BrowserShell::pump` navigating), and that
//! half is pinned headlessly by the unit + fixture tests. But the LAST STEP is
//! per-edge and lives outside Rust: the platform webview only performs the
//! redirect if the edge, after driving the core on a load signal, DRAINS the
//! pending load the pump produced. On desktop that drain is the shell's own GTK
//! pump (Rust, already covered); on mobile it is Kotlin / Swift, which the
//! pure-Rust `verify` gate (`cargo fmt && clippy && build && test`, no Android
//! SDK, no Xcode) otherwise never sees at all.
//!
//! A 3xx is exactly the case where the old wiring was NOT enough: the intercepted
//! request is answered fail-closed (nothing renders under the old URL), so the
//! signal that follows a redirect is a load FINISH/FAIL, and those handlers used
//! to only repaint the chrome (`refreshChrome`) rather than apply a pending load
//! (`afterCoreAction`). With only a repaint, the core would queue the navigation
//! and the webview would never perform it: a silently-desktop-only capability, the
//! precise failure mode `docs/adr/0005`'s parity guard exists to prevent. So this
//! test PARSES both mobile edges and asserts the drain is wired, in the same
//! spirit as `crates/werust-android/rust/tests/system_back_wiring_shape.rs` and
//! `crates/werust-core/tests/release_plumbing_shape.rs`.
//!
//! It also pins the NEGATIVE side of the mechanism on Android: the edge must NOT
//! try to answer an intercepted request with a 3xx status, because
//! `WebResourceResponse` refuses a 300-399 status code outright — the redirect is
//! performed by navigating, which is also what keeps the target hash-verified.

use std::path::{Path, PathBuf};

/// The workspace root, from `crates/werust-core`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("resolve workspace root from crates/werust-core")
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

const KOTLIN_EDGE: &str =
    "crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt";
const SWIFT_EDGE: &str = "crates/werust-ios/App/Sources/WKWebViewShellController.swift";

/// The body of `name`'s block, brace-matched from its opening `{` so the
/// extraction stops at the handler's OWN closing brace (not the file's).
fn block_body(source: &str, name: &str) -> String {
    let start = source
        .find(name)
        .unwrap_or_else(|| panic!("`{name}` must exist in the edge source"));
    let open = source[start..]
        .find('{')
        .unwrap_or_else(|| panic!("`{name}` must open a block"))
        + start;
    let mut depth = 0usize;
    for (offset, ch) in source[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return source[open + 1..open + offset].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("`{name}`'s block is unbalanced");
}

#[test]
fn the_android_edge_drains_a_pending_load_on_the_load_signals_a_redirect_lands_on() {
    // A queued 3xx surfaces as a PENDING LOAD once the core pumps, and the pump
    // runs when the edge reports a load signal. So every load-signal handler that
    // can follow a redirect must call `afterCoreAction` (drive -> drain pending
    // load -> repaint), not merely `refreshChrome` (repaint only). A redirect's
    // intercepted request is answered fail-closed, so `onReceivedError` is the
    // handler it most often lands on; `onPageFinished` and
    // `doUpdateVisitedHistory` cover the ordinary + same-document cases.
    let source = read(KOTLIN_EDGE);
    for handler in [
        "override fun onPageFinished",
        "override fun doUpdateVisitedHistory",
        "override fun onReceivedError",
    ] {
        let body = block_body(&source, handler);
        assert!(
            body.contains("afterCoreAction()"),
            "`{handler}` must drain the pending load (afterCoreAction), or a core-queued \
             _redirects 3xx navigation is never performed by the WebView; body was:\n{body}"
        );
    }
    // The drain really is drive-then-apply-then-repaint.
    let after = block_body(&source, "private fun afterCoreAction");
    assert!(
        after.contains("syncPendingLoad()") && after.contains("refreshChrome()"),
        "afterCoreAction must apply the pending load AND repaint, got:\n{after}"
    );
}

#[test]
fn the_android_edge_never_answers_an_intercepted_request_with_a_3xx_status() {
    // The mechanism constraint that forced the navigation design: Android's
    // `WebResourceResponse` REFUSES a 300-399 status code (it throws), so a
    // redirect can NOT be expressed as an intercepted response. The edge maps only
    // the SERVED statuses a site's `_redirects` may ask for; a 3xx never reaches
    // here at all (the core answers that request fail-closed and queues a
    // navigation instead).
    let source = read(KOTLIN_EDGE);
    let phrases = block_body(&source, "private fun statusReasonPhrase");
    for redirect_status in ["301", "302", "303", "307", "308"] {
        assert!(
            !phrases.contains(redirect_status),
            "the edge must not map a {redirect_status} onto a WebResourceResponse status \
             (it would throw); a 3xx is performed as a NAVIGATION"
        );
    }
}

#[test]
fn the_ios_edge_drains_a_pending_load_on_the_load_signals_a_redirect_lands_on() {
    // The iOS twin of the Android assertion: a `WKURLSchemeTask` cannot navigate
    // either, so the queued target only becomes a real load if the navigation
    // delegate's handlers drain the pending load.
    let source = read(SWIFT_EDGE);
    for handler in [
        "func webView(_ wv: WKWebView, didFinish navigation",
        "func webView(_ wv: WKWebView, didFail navigation",
    ] {
        let body = block_body(&source, handler);
        assert!(
            body.contains("afterCoreAction()"),
            "`{handler}` must drain the pending load (afterCoreAction), or a core-queued \
             _redirects 3xx navigation is never performed by the WKWebView; body was:\n{body}"
        );
    }
    let after = block_body(&source, "private func afterCoreAction");
    assert!(
        after.contains("syncPendingLoad()") && after.contains("refreshChrome()"),
        "afterCoreAction must apply the pending load AND repaint, got:\n{after}"
    );
}

#[test]
fn the_desktop_edge_hands_the_redirect_sink_to_the_shell() {
    // Desktop's drain is the shell's own pump, but only if the shell and the
    // scheme handler share ONE sink: `install_ipfs` returns it and `main.rs` must
    // pass it to `with_redirect_sink`. Dropping the returned sink compiles fine
    // and silently disables the whole capability on desktop, so pin the wiring.
    let main = read("crates/werust/src/main.rs");
    assert!(
        main.contains("let redirects = backend.install_ipfs();"),
        "desktop must KEEP the redirect sink `install_ipfs` returns"
    );
    assert!(
        main.contains("with_redirect_sink(redirects)"),
        "desktop must hand that sink to the shell, or a queued 3xx never navigates"
    );
}

#[test]
fn the_block_extractor_stops_at_the_matching_brace() {
    // The guard's own guard: a vacuous extractor (one that ran to the end of the
    // file) would make every assertion above pass regardless of the edge's real
    // shape, exactly the way an over-greedy match hid the earlier Kotlin wiring
    // gap. Pin it on a known nested shape.
    let source = "fun a() {\n  if (x) { inner() }\n  target()\n}\nfun b() { other() }\n";
    let body = block_body(source, "fun a");
    assert!(body.contains("target()") && body.contains("inner()"));
    assert!(
        !body.contains("other()"),
        "the extractor must stop at `fun a`'s closing brace, got:\n{body}"
    );
}
