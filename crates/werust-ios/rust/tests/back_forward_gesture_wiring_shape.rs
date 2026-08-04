//! Edge-swipe back/forward gesture guard (task
//! `enable-the-ios-back-forward-swipe-gesture`, spec
//! `chrome-conventional-controls` story 13).
//!
//! THE PROBLEM: `WKWebView.allowsBackForwardNavigationGestures` defaults to
//! `false`, and the iOS shell never set it, so the edge-swipe every other iOS
//! browser has did nothing in werust and history was reachable ONLY through the
//! on-screen `◀`/`▶` buttons
//! (`work/notes/observations/ios-edge-swipe-back-gesture-not-enabled-2026-07-26.md`).
//! The sibling task `ios-chrome-collapse-reload-stop-and-drop-history-buttons`
//! REMOVES those buttons on the strength of the gesture existing, so if the flag
//! silently returns to its default, iOS loses history navigation ENTIRELY.
//!
//! WHY A SOURCE-SHAPE GUARD: the flag lives in Swift, on a live `WKWebView`, and
//! nobody on this project has a Mac
//! (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`),
//! so no human can discover the regression by using the app. The pure-Rust
//! `verify` gate (`cargo fmt && clippy && build && test`, no Xcode) is the only
//! evidence this platform gets on every change, so the enablement is asserted by
//! PARSING the Swift edge, exactly as
//! `crates/werust-android/rust/tests/system_back_wiring_shape.rs` asserts the
//! Kotlin system-Back wiring. Every assertion here runs on the Linux gate with no
//! network and no simulator.
//!
//! Acceptance criteria mapped to assertions:
//! 1. The gesture is enabled on the shell's `WKWebView`
//!    (`the_shell_enables_the_back_forward_swipe_gesture_on_its_webview`).
//! 2. A test pins it, so a refactor cannot silently return to the default (this
//!    file, run by the gate).
//! 3. A swipe reports through the SAME load-lifecycle path a button-driven move
//!    does, so the chrome does not go stale
//!    (`a_gesture_driven_history_navigation_is_reported_into_the_core`,
//!    `only_a_main_frame_history_navigation_is_reported_as_the_page_the_user_is_on`,
//!    `the_lifecycle_handlers_a_gesture_navigation_lands_on_report_into_the_core`,
//!    and the headless chrome assertions below, which drive the SAME
//!    `CoreSession` the Swift edge drives across the C-ABI — including the two
//!    STICKY per-entry axes, the error banner and the invalid-entry badge, which
//!    a swipe used to carry onto the page it landed on).
//! 4. CI-runner-checkable without a Mac: source-shape + headless core, no Xcode.
//! 5. Network-isolated, in the repo's existing style.
//!
//! The gesture-vs-programmatic navigation differences this bakes in are recorded
//! in `docs/spikes/enable-the-ios-back-forward-swipe-gesture/DECISIONS.md`.

use std::path::{Path, PathBuf};

use werust_mobile::CoreSession;

/// The Swift OS edge this guard parses. `CARGO_MANIFEST_DIR` is
/// `crates/werust-ios/rust`, so the app module is its sibling.
fn shell_controller_source() -> String {
    let path: PathBuf =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../App/Sources/WKWebViewShellController.swift");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The first occurrence of `pat` in `bytes` at or after `from`, as an absolute
/// index. Byte-wise (never slices the `str`) so the scan below can walk a source
/// full of multi-byte characters (`◀︎`, `⟳`, `⋮`) without landing mid-char.
fn find_from(bytes: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if from >= bytes.len() {
        return None;
    }
    bytes[from..]
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|i| from + i)
}

/// The BODY of a Swift declaration: the text between the braces of the block
/// that opens after `signature`, bounded at its MATCHING closing brace.
///
/// Brace-matched rather than "up to the next declaration", for the reason the
/// Android guard records: a terminator picked by KIND rather than POSITION can
/// swallow later members, and then an assertion about THIS body is satisfied by
/// some OTHER method's line and stays green over an empty handler. `//` line
/// comments, `/* */` block comments and `"`/`"""` string literals are skipped so
/// a brace inside one cannot unbalance the count. Its own regression guard is
/// [`the_block_extractor_stops_at_the_matching_brace`].
fn swift_block_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let bytes = source.as_bytes();
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("the Swift source must declare `{signature}`"));
    let after = start + signature.len();
    let open =
        find_from(bytes, after, b"{").unwrap_or_else(|| panic!("`{signature}` must open a block"));

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
                    // `open` and `i` are ASCII brace positions, so both are char
                    // boundaries.
                    return &source[open + 1..i];
                }
            }
            _ => {}
        }
        i += 1;
    }
    panic!("`{signature}` opens a block that is never closed")
}

/// Drive the in-flight load to done the way the Swift edge would from the
/// platform `WKWebView`'s `didCommit` + `didFinish` signals.
fn settle(session: &mut CoreSession) {
    let url = session
        .take_pending_load()
        .expect("a pending load to apply to the WKWebView");
    session.on_page_committed(&url);
    session.on_page_finished(&url);
}

/// A settled two-entry history (`a` then `b`), the state a swipe acts on.
fn session_on_b_with_a_behind() -> CoreSession {
    let mut s = CoreSession::new();
    assert!(s.navigate("https://a.example/"));
    settle(&mut s);
    assert!(s.navigate("https://b.example/"));
    settle(&mut s);
    s
}

/// The same two-entry history, but `b` FAILED to load: the chrome is showing an
/// error banner for the entry the user is about to leave. This is the state the
/// error-free fixture above cannot express, and the one a swipe used to carry
/// onto the page it landed on.
fn session_on_a_failed_b_with_a_behind() -> CoreSession {
    let mut s = CoreSession::new();
    assert!(s.navigate("https://a.example/"));
    settle(&mut s);
    assert!(s.navigate("https://b.example/"));
    let url = s
        .take_pending_load()
        .expect("a pending load to apply to the WKWebView");
    s.on_page_failed(&url, "the host could not be reached");
    assert!(
        s.chrome().last_error.is_some(),
        "the fixture must actually be showing an error banner"
    );
    s
}

/// Every chrome field a history move can move, compared field by field so a
/// failure NAMES the one that drifted rather than dumping two structs.
///
/// This is the parity claim of criterion 3 in one place: whatever the user swipes
/// OFF (a settled page, a failed one, a rejected URL-bar entry), the chrome must
/// land where the `◀` button would have left it.
fn assert_same_chrome(button: &CoreSession, swipe: &CoreSession, case: &str) {
    let (b, s) = (button.chrome(), swipe.chrome());
    assert_eq!(b.url_text, s.url_text, "{case}: the URL bar");
    assert_eq!(b.can_go_back, s.can_go_back, "{case}: Back availability");
    assert_eq!(
        b.can_go_forward, s.can_go_forward,
        "{case}: Forward availability"
    );
    assert_eq!(b.load_state, s.load_state, "{case}: the load state");
    assert_eq!(b.load_step, s.load_step, "{case}: the load step");
    assert_eq!(
        b.trust_posture, s.trust_posture,
        "{case}: the trust posture"
    );
    assert_eq!(b.last_error, s.last_error, "{case}: the error banner");
    assert_eq!(
        b.invalid_entry, s.invalid_entry,
        "{case}: the invalid-entry badge"
    );
}

/// Drive a swipe back onto `a` exactly as the Swift edge does: the policy hook
/// reports the gesture's target, then WebKit's own commit/finish land on it.
fn swipe_back_to_a(s: &mut CoreSession) {
    s.on_history_navigated("https://a.example/");
    s.on_page_committed("https://a.example/");
    s.on_page_finished("https://a.example/");
}

#[test]
fn the_shell_enables_the_back_forward_swipe_gesture_on_its_webview() {
    // Criterion 1, and the whole reason this file exists: the flag is SET, on the
    // shell's own `WKWebView`, where the rest of that view's configuration is
    // (`layoutChrome`, brace-bounded so a stray mention in a comment elsewhere
    // cannot satisfy it).
    let src = shell_controller_source();
    let layout = swift_block_body(&src, "private func layoutChrome()");
    assert!(
        layout.contains("webView.allowsBackForwardNavigationGestures = true"),
        "the shell must ENABLE the edge-swipe history gesture on its WKWebView \
         (WKWebView's default is false, which leaves iOS with no swipe at all); \
         `layoutChrome` instead reads:\n{layout}"
    );
    // The negative half: nothing may set it back to the default. A single
    // `= false` anywhere in the edge is the regression this guard exists to catch.
    assert!(
        !src.contains("allowsBackForwardNavigationGestures = false"),
        "the edge must never disable the gesture: the sibling task removes the \
         on-screen ◀/▶ buttons, so iOS would be left with NO history navigation"
    );
}

#[test]
fn a_gesture_driven_history_navigation_is_reported_into_the_core() {
    // Criterion 3, the edge half. A swipe navigates the WKWebView's OWN
    // back-forward list without ever calling the core, so unless the edge REPORTS
    // it the core's cursor never moves: the URL bar, the history capability flags
    // and the trust posture would all describe the document the user just swiped
    // AWAY from. `decidePolicyFor` is the earliest signal that names the
    // navigation `.backForward`, and it fires BEFORE the new document's bytes are
    // resolved, which is what lets the core reset the per-load trust axes without
    // clobbering the `ipfs` handler's later verification of the NEW page.
    let src = shell_controller_source();
    let policy = swift_block_body(&src, "decidePolicyFor navigationAction");
    assert!(
        policy.contains(".backForward"),
        "the policy hook must recognise a BACK-FORWARD navigation (the swipe, and \
         a page's own `history.back()`); it reads:\n{policy}"
    );
    assert!(
        policy.contains("core.onHistoryNavigated("),
        "a back-forward navigation must be reported into the core as a HISTORY \
         MOVE, or the core pushes a duplicate entry and its history flags go \
         stale; it reads:\n{policy}"
    );
    assert!(
        policy.contains("decisionHandler(.allow)"),
        "the navigation must be ALLOWED: WebKit performs the swipe itself"
    );
    // The edge must not fight WebKit's own navigation. Cancelling it would snap
    // the interactive gesture back, and re-issuing the load (`goBack`/`load`)
    // would either double-navigate or start a load the gesture is already doing.
    for forbidden in [".cancel", "core.goBack()", "core.goForward()", ".load("] {
        assert!(
            !policy.contains(forbidden),
            "the policy hook must not `{forbidden}`: WebKit performs a gesture \
             navigation itself, the edge only REPORTS it; it reads:\n{policy}"
        );
    }
}

#[test]
fn only_a_main_frame_history_navigation_is_reported_as_the_page_the_user_is_on() {
    // Criterion 3's correctness floor, and a security one. WebKit issues a policy
    // decision PER FRAME, and a back navigation onto a page that has iframes (or
    // an iframe of the current page calling `history.back()`) produces
    // `.backForward` decisions carrying the SUBFRAME's url. Reported into the
    // core, that url is neither of the current entry's neighbours, so the move
    // takes the drift branch: it TRUNCATES the forward history and pushes the
    // subresource as the current entry, and the URL bar then shows an address the
    // user is not on — chosen by whoever wrote the iframe, on a browser whose
    // whole thesis is an honest address. Only the MAIN FRAME's navigation is the
    // page the user is on, and the guard must come BEFORE the report.
    let src = shell_controller_source();
    let policy = swift_block_body(&src, "decidePolicyFor navigationAction");
    assert!(
        policy.contains("targetFrame?.isMainFrame == true"),
        "the policy hook must report only MAIN-FRAME back-forward navigations \
         (the same `targetFrame` idiom the `_blank` hook in this file already \
         uses); it reads:\n{policy}"
    );
    let guard = policy
        .find("targetFrame?.isMainFrame == true")
        .expect("the main-frame guard, asserted above");
    let report = policy
        .find("core.onHistoryNavigated(")
        .expect("the report into the core, asserted below");
    assert!(
        guard < report,
        "the main-frame guard must be checked BEFORE the core is told the user \
         moved, or a subframe's url still reaches the URL bar; it reads:\n{policy}"
    );
}

#[test]
fn the_lifecycle_handlers_a_gesture_navigation_lands_on_report_into_the_core() {
    // Criterion 3, the "SAME load-lifecycle path" half. A gesture-driven
    // navigation is a real cross-document load: WebKit drives the SAME
    // `WKNavigationDelegate` callbacks a button-driven `WKWebView.load` does, so
    // the chrome settles through the same handlers. Pin that they still report
    // into the core (a handler that only repainted would leave the load state
    // stuck on the previous document).
    let src = shell_controller_source();
    let commit = swift_block_body(&src, "func webView(_ wv: WKWebView, didCommit navigation");
    assert!(
        commit.contains("core.onPageCommitted("),
        "`didCommit` must report into the core; it reads:\n{commit}"
    );
    let finish = swift_block_body(&src, "func webView(_ wv: WKWebView, didFinish navigation");
    assert!(
        finish.contains("core.onPageFinished("),
        "`didFinish` must report into the core; it reads:\n{finish}"
    );
    // A swipe onto a SAME-DOCUMENT history entry (an SPA `pushState` entry) fires
    // no commit/finish at all; the KVO observer on `webView.url` is the only
    // signal, and it must keep reporting.
    assert!(
        src.contains("self.core.onUrlChanged("),
        "the KVO observer on `webView.url` must keep reporting same-document URL \
         changes, which is the only signal a same-document swipe produces"
    );
}

#[test]
fn a_swipe_back_moves_the_core_cursor_instead_of_pushing_a_duplicate_entry() {
    // Criterion 3, headless: the chrome after a swipe. The edge reports the
    // gesture's target; the core recognises it as the entry BEHIND the current
    // one and moves its cursor there, so Back/Forward availability describes the
    // page the user is actually on. Reported as a plain URL change (a push) it
    // would instead grow history to [a, b, a]: Forward would be false while the
    // user can plainly swipe forward, and every swipe would leak another entry.
    let mut s = session_on_b_with_a_behind();
    assert!(s.chrome().can_go_back);
    assert!(!s.chrome().can_go_forward);

    // The swipe: WebKit navigates its own list; the edge reports the target.
    s.on_history_navigated("https://a.example/");
    assert_eq!(s.chrome().url_text, "https://a.example/");
    assert!(
        !s.chrome().can_go_back,
        "back at the first entry: nowhere further back"
    );
    assert!(
        s.chrome().can_go_forward,
        "the entry swiped away from is still ahead"
    );
    assert_eq!(
        s.take_pending_load(),
        None,
        "WebKit is already performing the navigation: the core must NOT queue a \
         load that would re-navigate on top of the gesture"
    );

    // WebKit then reports the same navigation's real lifecycle, exactly as it
    // does for a button-driven load, and the chrome settles on it.
    s.on_page_committed("https://a.example/");
    s.on_page_finished("https://a.example/");
    assert_eq!(s.chrome().url_text, "https://a.example/");
    assert!(!s.chrome().is_loading());
    assert!(!s.chrome().can_go_back);
    assert!(s.chrome().can_go_forward);
}

#[test]
fn the_chrome_after_a_swipe_back_matches_the_chrome_after_a_button_back() {
    // The criterion in its strongest form: a gesture-driven move and a
    // button-driven one leave the SAME chrome. Two sessions, the same history,
    // one moved by `◀` and one by the swipe.
    let mut button = session_on_b_with_a_behind();
    button.go_back();
    settle(&mut button);

    let mut swipe = session_on_b_with_a_behind();
    swipe_back_to_a(&mut swipe);

    assert_same_chrome(&button, &swipe, "after a swipe back over a settled load");
}

#[test]
fn the_chrome_after_a_swipe_back_off_a_failed_load_matches_the_button_back() {
    // The case the error-free parity test above CANNOT see, and the one that was
    // wrong: load `a`, navigate to `b`, which FAILS (the error banner is up), then
    // leave `b` by swiping. `BrowserShell::go_back` clears `last_error` because a
    // history move that proceeds is not the failed load's problem — so a swipe
    // that only reported a URL change left the dead page's red banner standing
    // over a page that loaded perfectly well. The user cannot dismiss it: nothing
    // short of another navigation clears it.
    let mut button = session_on_a_failed_b_with_a_behind();
    button.go_back();
    settle(&mut button);
    assert_eq!(
        button.chrome().last_error,
        None,
        "the button-driven move is the reference: it clears the banner"
    );

    let mut swipe = session_on_a_failed_b_with_a_behind();
    swipe_back_to_a(&mut swipe);
    assert_eq!(
        swipe.chrome().last_error,
        None,
        "the swipe must clear the failed page's banner too: it is showing over a \
         page that loaded fine, and no chrome control on iOS can dismiss it"
    );

    assert_same_chrome(&button, &swipe, "after a swipe back off a failed load");
}

#[test]
fn the_chrome_after_a_swipe_back_off_a_rejected_url_entry_matches_the_button_back() {
    // The other sticky per-entry axis, orthogonal to the error banner: a typed URL
    // the core REFUSED leaves the invalid-entry badge up and PINS the typed text
    // in the bar (`fail_invalid_entry`). A history move the user then performs is
    // them moving on, so `go_back` drops both — and the swipe must, or iOS shows a
    // badge and a bar full of the rejected text over the page it swiped to.
    let mut button = session_on_b_with_a_behind();
    button.navigate("not-a-url");
    assert!(
        button.chrome().invalid_entry.is_some(),
        "the fixture must actually be showing the invalid-entry badge"
    );
    button.go_back();
    settle(&mut button);

    let mut swipe = session_on_b_with_a_behind();
    swipe.navigate("not-a-url");
    swipe_back_to_a(&mut swipe);
    assert_eq!(
        swipe.chrome().invalid_entry,
        None,
        "the swipe must drop the rejected entry's badge"
    );
    assert_eq!(
        swipe.chrome().url_text,
        "https://a.example/",
        "and the bar must follow the page swiped to, not keep the rejected text"
    );

    assert_same_chrome(&button, &swipe, "after a swipe back off a rejected entry");
}

#[test]
fn a_swipe_forward_returns_to_the_entry_the_swipe_back_left() {
    // The asymmetry the spec accepts: unlike Android's system Back, the iOS
    // gesture navigates BOTH ways, so the forward direction must move the cursor
    // forward rather than push.
    let mut s = session_on_b_with_a_behind();
    s.on_history_navigated("https://a.example/");
    s.on_page_committed("https://a.example/");
    s.on_page_finished("https://a.example/");

    s.on_history_navigated("https://b.example/");
    s.on_page_committed("https://b.example/");
    s.on_page_finished("https://b.example/");
    assert_eq!(s.chrome().url_text, "https://b.example/");
    assert!(s.chrome().can_go_back, "a is behind again");
    assert!(!s.chrome().can_go_forward, "back at the newest entry");
}

#[test]
fn a_repeated_report_of_the_current_entry_changes_nothing() {
    // The edge has THREE signals for one gesture navigation (the policy hook, the
    // KVO url observer, and the commit), and they can report the same URL. The
    // move must therefore be idempotent, or a swipe would walk the cursor twice.
    let mut s = session_on_b_with_a_behind();
    s.on_history_navigated("https://a.example/");
    s.on_history_navigated("https://a.example/");
    s.on_url_changed("https://a.example/");
    assert_eq!(s.chrome().url_text, "https://a.example/");
    assert!(!s.chrome().can_go_back);
    assert!(s.chrome().can_go_forward);

    // "Changes nothing" includes the chrome reset a real move makes: the entry
    // being reported is the one already shown, so ITS own failure is current and a
    // repeat report must not quietly dismiss the banner the user needs to see.
    s.on_page_failed("https://a.example/", "the host could not be reached");
    assert!(s.chrome().last_error.is_some(), "the fixture's own failure");
    s.on_history_navigated("https://a.example/");
    assert!(
        s.chrome().last_error.is_some(),
        "a repeat report moved nothing, so nothing about this entry is stale"
    );
}

#[test]
fn a_gesture_navigation_the_core_history_does_not_know_is_followed_not_dropped() {
    // WebKit's back-forward list and the core's session history are two stacks
    // that CAN drift (a core-driven history move is performed as a fresh
    // `WKWebView.load`, which appends to WebKit's list), so a gesture can land on
    // a URL that is neither of the core's adjacent entries. The bar must FOLLOW
    // the page the user is actually looking at rather than silently keep the old
    // address, which would be the worst outcome for a browser whose thesis is an
    // honest address + trust posture.
    let mut s = session_on_b_with_a_behind();
    s.on_history_navigated("https://c.example/");
    assert_eq!(s.chrome().url_text, "https://c.example/");
    assert!(s.chrome().can_go_back);
    assert!(!s.chrome().can_go_forward);
}

#[test]
fn the_block_extractor_stops_at_the_matching_brace() {
    // The guard ON the guard: a vacuous extractor (one that ran to the end of the
    // file) would make every source assertion above pass regardless of the edge's
    // real shape. Pin it on a fixture shaped like the trap: a short method whose
    // decoy line lives in a LATER method.
    let fixture = "\
final class Fixture {
    private func layoutChrome() {
        webView.allowsBackForwardNavigationGestures = true
    }

    func webView(_ wv: WKWebView, didCommit navigation: WKNavigation!) {
        core.onPageCommitted(\"x\")
    }
}
";
    let layout = swift_block_body(fixture, "private func layoutChrome()");
    assert!(layout.contains("allowsBackForwardNavigationGestures = true"));
    assert!(
        !layout.contains("onPageCommitted"),
        "the extractor must stop at `layoutChrome`'s closing brace; it read:\n{layout}"
    );

    // An emptied body must extract as empty, which is what makes the assertions
    // above FAIL when the edge stops doing the thing.
    let emptied = fixture.replace(
        "        webView.allowsBackForwardNavigationGestures = true\n",
        "",
    );
    assert!(
        !swift_block_body(&emptied, "private func layoutChrome()")
            .contains("allowsBackForwardNavigationGestures"),
        "an emptied body must extract as empty (otherwise the guard is vacuous)"
    );

    // Braces inside comments and string literals must not unbalance the count.
    let tricky = "\
    private func sample() {
        // a brace in a comment: }
        /* and a block one: } */
        let s = \"a literal brace }\"
        let t = \"\"\"a raw one }\"\"\"
        let marker = 1
    }

    private func after() {
        let outside = 2
    }
";
    let body = swift_block_body(tricky, "private func sample()");
    assert!(
        body.contains("let marker = 1") && !body.contains("let outside = 2"),
        "braces inside comments/strings must not end the body early or late; it read:\n{body}"
    );
}
