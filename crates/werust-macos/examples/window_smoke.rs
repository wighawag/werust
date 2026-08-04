//! Build the REAL AppKit window, drive it through a real load, and assert what
//! the real widgets show: the CI job's "exercise the window" step.
//!
//! Everything about this window that a Linux gate can check is checked there —
//! `crate::paint` is unit-tested against the real `werust-core`, and
//! `tests/macos_window_shape.rs` guards the wiring. What NO Linux gate can do is
//! CONSTRUCT an `NSWindow`, an `NSTextField` and an `NSMenu` and read back what
//! they hold. That is this smoke's whole job, and it is why it asserts on the
//! WIDGETS (`window.url_text()`, `window.trust_text()`, `window.error_banner()`)
//! rather than on the paint snapshot it already trusts.
//!
//! It is OFFLINE and deterministic: the `ipfs://` route is the PRODUCTION
//! verifying resolver over a pinned, in-memory retriever (the same fixture shape
//! the engine's `trust_hooks_smoke` uses), so a hash-verified page and a
//! hash-MISMATCHING control both come from memory. No gateway, no network, no
//! signing, no packaging.
//!
//! The window is opened FAR off-screen with the app as an accessory (no Dock
//! icon, no menu bar), so a CI run shows nothing and steals no focus.
//!
//! Run it with `cargo run -p werust-macos --example window_smoke` on a Mac; CI
//! runs it on the existing `macos-14` runner.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!(
        "window_smoke builds a real AppKit window and only runs on macOS.\n\
         Run it via .github/workflows/macos-renderer.yml on the `macos-14` runner, or on a Mac\n\
         with `cargo run -p werust-macos --example window_smoke`."
    );
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() {
    std::process::exit(macos::run());
}

#[cfg(target_os = "macos")]
mod macos {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use fetcher::{cid_v1_raw_sha256, ContentRetriever, RetrieveError, RetrievedContent};
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSEventModifierFlags};
    use objc2_foundation::{NSDate, NSRunLoop};
    use renderer::LoadState;
    use werust_core::debug::DebugCapture;
    use werust_core::ipfs::RedirectSink;
    use werust_core::menu::{BrowserMenu, MENU_ITEM_DEBUG};
    use werust_core::shortcuts::Focus;
    use werust_core::{status_line, trust_indicator, trust_indicator_detail, BrowserShell};
    use werust_macos::input;
    use werust_macos::paint::install_debug_capture;
    use werust_macos::window::{BrowserWindow, Placement};

    /// The canned page. Its `console.log` is what proves the CONSOLE capture
    /// point really reaches the debug view's Console tab.
    const PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>werust macos window smoke</title></head>
<body><p>werust macOS window smoke</p>
<script>console.log('werust window smoke page loaded');</script>
</body></html>
"#;

    /// A pinned, in-memory `ContentRetriever`: the fixture is served from RAM and
    /// still goes through the production per-block verify, so the run is offline
    /// and a tampered CID genuinely fails.
    struct PinnedRetriever {
        honest_cid: String,
        honest: Vec<u8>,
        tampered_cid: String,
        tampered: Vec<u8>,
        /// How many retrievals this fixture has served. A RELOAD is otherwise
        /// indistinguishable from "the page was already loaded", so the shortcut
        /// checks below watch this rather than a settled load state.
        retrievals: AtomicUsize,
    }

    impl PinnedRetriever {
        fn retrievals(&self) -> usize {
            self.retrievals.load(Ordering::SeqCst)
        }
    }

    impl ContentRetriever for PinnedRetriever {
        fn retrieve(&self, cid: &str, _path: &str) -> Result<RetrievedContent, RetrieveError> {
            self.retrievals.fetch_add(1, Ordering::SeqCst);
            let bytes = if cid == self.honest_cid {
                &self.honest
            } else if cid == self.tampered_cid {
                &self.tampered
            } else {
                return Err(RetrieveError::MissingBlock {
                    cid: cid.to_string(),
                });
            };
            // The production verify, applied here so the control really fails.
            let derived = cid_v1_raw_sha256(bytes).expect("derive a cid for the held bytes");
            if derived != cid {
                return Err(RetrieveError::BlockHashMismatch {
                    cid: cid.to_string(),
                });
            }
            Ok(RetrievedContent {
                bytes: bytes.clone(),
                codec: 0x55,
            })
        }
    }

    /// Turn the AppKit run loop for a moment and pump the window once, so the
    /// seam's events, the off-thread `ipfs://` completions and the chrome all
    /// advance together.
    fn pump(window: &BrowserWindow) {
        let until = NSDate::dateWithTimeIntervalSinceNow(0.02);
        NSRunLoop::currentRunLoop().runUntilDate(&until);
        window.tick();
    }

    fn wait_until(window: &BrowserWindow, seconds: u32, done: impl Fn() -> bool) -> bool {
        for _ in 0..(seconds * 50) {
            pump(window);
            if done() {
                return true;
            }
        }
        false
    }

    /// Press Escape on the real window. Spelled once because the two focus
    /// contexts must be driven by the IDENTICAL key press, or the pair below
    /// would compare two different things.
    fn press_escape(window: &BrowserWindow) {
        window.press_key(
            input::KEY_CODE_ESCAPE,
            "\u{1b}",
            NSEventModifierFlags::empty(),
        );
    }

    /// One assertion, reported rather than panicked, so the run prints every
    /// result instead of stopping at the first failure.
    fn check(failures: &mut Vec<String>, ok: bool, what: &str) {
        if ok {
            println!("  ok   {what}");
        } else {
            println!("  FAIL {what}");
            failures.push(what.to_string());
        }
    }

    pub fn run() -> i32 {
        let Some(mtm) = MainThreadMarker::new() else {
            eprintln!("the macOS shell must be started on the main thread");
            return 1;
        };
        // No Dock icon, no menu bar, nothing raised over the user (or the runner).
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

        let honest = PAGE.as_bytes().to_vec();
        let honest_cid = cid_v1_raw_sha256(&honest).expect("derive the fixture cid");
        // The NEGATIVE CONTROL: a real CID naming DIFFERENT bytes, so the load
        // must FAIL and the window must say so.
        let claimed = b"the page this cid actually names".to_vec();
        let tampered_cid = cid_v1_raw_sha256(&claimed).expect("derive the control cid");
        let retriever = Arc::new(PinnedRetriever {
            honest_cid: honest_cid.clone(),
            honest,
            tampered_cid: tampered_cid.clone(),
            tampered: b"tampered bytes that do not hash to the cid".to_vec(),
            retrievals: AtomicUsize::new(0),
        });
        // A second handle on the SAME fixture, kept out of the closure so the
        // shortcut checks can watch what the page really fetched.
        let served = Arc::clone(&retriever);

        let Ok(mut backend) = macos_renderer::MacosRenderer::new() else {
            eprintln!("could not create the macOS backend (is this the main thread?)");
            return 1;
        };
        // The PRODUCTION verifying route, with a pinned retriever instead of a
        // gateway: offline, deterministic, still hash-gated.
        let redirects = RedirectSink::new();
        let sink = redirects.clone();
        backend.install_verifying_scheme(
            "ipfs",
            Arc::new(move |uri: String| {
                webview_shared::offthread::retrieve_off_thread(retriever.as_ref(), uri, &sink)
            }),
        );
        backend.install_provider();
        let capture = DebugCapture::new();
        install_debug_capture(&mut backend, capture.clone());

        let shell = Rc::new(RefCell::new(
            BrowserShell::new(Box::new(backend))
                .with_redirect_sink(redirects)
                .with_debug_capture(capture.clone()),
        ));
        let window = BrowserWindow::open(
            mtm,
            shell.clone(),
            capture.clone(),
            // FAR off-screen: a CI run shows nothing.
            Placement::OffScreen,
        );

        let mut failures = Vec::new();

        println!("the window opens with the core's own default chrome:");
        {
            let chrome = shell.borrow();
            let chrome = chrome.chrome();
            check(
                &mut failures,
                window.trust_text() == trust_indicator(chrome),
                "the trust indicator shows the core's badge for the default state",
            );
            check(
                &mut failures,
                window.trust_detail().as_deref() == Some(trust_indicator_detail(chrome)),
                "the trust badge carries the core's EXPLANATION as its tooltip",
            );
            check(
                &mut failures,
                window.status_text() == status_line(chrome),
                "the status line shows the core's status",
            );
        }
        check(
            &mut failures,
            window.error_banner().is_none(),
            "no error banner on a window that has not failed anything",
        );
        check(
            &mut failures,
            !window.invalid_badge_visible(),
            "no invalid-entry badge on a fresh window",
        );

        println!("the ⋮ menu is the shared core's BrowserMenu, item for item:");
        let expected: Vec<String> = BrowserMenu::new()
            .items()
            .iter()
            .map(|item| item.label.clone())
            .collect();
        check(
            &mut failures,
            window.menu_titles() == expected,
            "the NSMenu's titles are the core's item labels, in order",
        );

        println!("a hash-VERIFIED ipfs:// page (offline, pinned):");
        let url = format!("ipfs://{honest_cid}/");
        let page_before = window.page_frame();
        if shell.borrow_mut().navigate(&url).is_err() {
            eprintln!("the shell refused the fixture URL");
            return 1;
        }
        let settled = wait_until(&window, 20, || {
            shell.borrow().chrome().load_state == LoadState::Finished
        });
        check(&mut failures, settled, "the verified load settles");
        check(
            &mut failures,
            window.url_text().contains(&honest_cid),
            "the URL bar shows the loaded content-addressed URL",
        );
        {
            let chrome = shell.borrow();
            let chrome = chrome.chrome();
            check(
                &mut failures,
                window.trust_text() == trust_indicator(chrome),
                "the trust indicator paints the core's verdict for the settled load",
            );
            check(
                &mut failures,
                window.trust_text().contains("verified"),
                "a hash-verified page reads as verified in the window",
            );
            check(
                &mut failures,
                window.status_text() == status_line(chrome),
                "the status line still mirrors the core",
            );
        }
        check(
            &mut failures,
            window.error_banner().is_none(),
            "a successful load raises NO banner (progress lived in the URL bar)",
        );
        check(
            &mut failures,
            window.page_frame() == page_before,
            "the page view did NOT move or resize across a whole load",
        );

        println!("the conventional shortcuts, pressed as REAL NSEvents on this window:");
        // The one thing no Linux gate can do for the shortcut layer: build an
        // actual `NSEvent` and push it through the real window's `sendEvent:`,
        // which is where werust's chords beat the focused page and the URL bar's
        // field editor. The translation TABLE behind them (`werust_macos::input`)
        // is unit-tested against the real core on every Ubuntu run; what is
        // proved HERE is that a pressed key reaches the resolution at all and
        // that the resolved action really moves the chrome.
        //
        // The modifier flags come from AppKit's OWN `NSEventModifierFlags`, so
        // this also checks the plain-bit constants the Linux-side table is
        // written against against the real thing.
        check(
            &mut failures,
            NSEventModifierFlags::Command.bits() as u64 == input::MODIFIER_FLAG_COMMAND
                && NSEventModifierFlags::Control.bits() as u64 == input::MODIFIER_FLAG_CONTROL
                && NSEventModifierFlags::Option.bits() as u64 == input::MODIFIER_FLAG_OPTION
                && NSEventModifierFlags::Shift.bits() as u64 == input::MODIFIER_FLAG_SHIFT,
            "the translation table's modifier bits are AppKit's own NSEventModifierFlags",
        );

        // Story 1 / 4: Cmd+L, the chord this whole edge exists for, and the ONLY
        // place the shared resolution's Cmd branch is reached by a real key press
        // anywhere in this project.
        check(
            &mut failures,
            window.blur_url_bar(),
            "the URL bar can be blurred before the first chord is pressed",
        );
        pump(&window);
        check(
            &mut failures,
            window.reported_focus() == Focus::Page,
            "the window reports PAGE focus while the URL bar is not being edited",
        );
        window.press_key(0, "l", NSEventModifierFlags::Command);
        pump(&window);
        check(
            &mut failures,
            window.reported_focus() == Focus::UrlBar,
            "Cmd+L focuses the URL bar",
        );

        // The NEGATIVE CONTROL for the Cmd branch: a Mac user's Ctrl+L is NOT a
        // browser shortcut. Without this, "Cmd+L works" would pass just as well
        // on an edge that claimed the key under any modifier at all.
        check(
            &mut failures,
            window.blur_url_bar(),
            "the URL bar gives the keyboard back after Cmd+L took it",
        );
        pump(&window);
        window.press_key(0, "l", NSEventModifierFlags::Control);
        pump(&window);
        check(
            &mut failures,
            window.reported_focus() == Focus::Page,
            "Ctrl+L is NOT the Mac URL-bar chord and leaves focus alone",
        );

        // Story 6: Escape in the URL bar REVERTS the edit and restores the URL
        // the chrome believes, and navigates nowhere.
        let believed = shell.borrow().chrome().url_text.clone();
        let typed = "rubbish the user typed and thought better of";
        window.focus_url_bar();
        window.set_url_text(typed);
        pump(&window);
        check(
            &mut failures,
            window.reported_focus() == Focus::UrlBar,
            "the window reports URL-BAR focus while the bar is being edited",
        );
        press_escape(&window);
        pump(&window);
        check(
            &mut failures,
            window.url_text() == believed,
            "Escape in the URL bar restores the URL the chrome believes",
        );
        check(
            &mut failures,
            shell.borrow().chrome().url_text == believed,
            "…and reverting the bar navigates nowhere",
        );

        // STORIES 5 + 6, THE DISCRIMINATING PAIR: the SAME key, the same window,
        // a different reported focus must do something ELSE.
        //
        // Watched at the EFFECT on the LOAD (the shape
        // `crates/werust-windows/examples/window_smoke.rs` already uses), never at
        // the URL bar's text. The bar cannot tell the two branches apart:
        // `ChromeAction::Stop` calls `refresh_chrome`, and a chrome repaint
        // rewrites the bar from the BELIEVED url — exactly where
        // `ChromeAction::RevertUrlBar` leaves it. The check that used to stand
        // here read the bar, so it proved nothing in EITHER focus and this edge's
        // focus REPORTING was unguarded from the day it landed
        // (`work/notes/observations/the-macos-page-focused-escape-check-was-never-discriminating-2026-08-04.md`).
        //
        // Neither half pumps between the navigation and the key press, so the load
        // is genuinely in flight at the seam when Escape arrives and nothing about
        // the timing is lucky.
        let in_flight = format!("ipfs://{honest_cid}/");

        // The BAR half: Escape reverts the edit and LEAVES THE LOAD ALONE.
        let started = shell.borrow_mut().navigate(&in_flight).is_ok();
        check(
            &mut failures,
            started && shell.borrow().chrome().is_loading(),
            "a load is in flight when Escape is pressed with the URL bar focused",
        );
        let believed_in_flight = shell.borrow().chrome().url_text.clone();
        window.focus_url_bar();
        window.set_url_text(typed);
        check(
            &mut failures,
            window.reported_focus() == Focus::UrlBar,
            "…and the window reports URL-BAR focus for it",
        );
        press_escape(&window);
        check(
            &mut failures,
            window.url_text() == believed_in_flight,
            "Escape in the URL bar reverts the edit",
        );
        check(
            &mut failures,
            shell.borrow().chrome().is_loading(),
            "…and does NOT cancel the in-flight load",
        );
        let settled = wait_until(&window, 20, || {
            shell.borrow().chrome().load_state == LoadState::Finished
        });
        check(
            &mut failures,
            settled,
            "the load the URL bar's Escape left alone goes on to settle",
        );

        // The PAGE half: the same key CANCELS the load. This is the whole safety
        // net for the focus half of the shortcut layer — if this edge reported
        // focus wrongly (or decided Escape itself), the load would survive here
        // exactly as it just did above.
        let started = shell.borrow_mut().navigate(&in_flight).is_ok();
        check(
            &mut failures,
            started && shell.borrow().chrome().is_loading(),
            "a load is in flight when Escape is pressed with the page focused",
        );
        check(
            &mut failures,
            window.blur_url_bar(),
            "blurring the URL bar ends its field-editor session",
        );
        check(
            &mut failures,
            window.reported_focus() == Focus::Page,
            "…so the window reports PAGE focus for the identical key press",
        );
        press_escape(&window);
        check(
            &mut failures,
            !shell.borrow().chrome().is_loading(),
            "Escape with the PAGE focused CANCELS the in-flight load",
        );

        // The cancel leaves the shell settled on nothing in flight, so the checks
        // below start from a loaded page again, exactly as they did before.
        if shell.borrow_mut().navigate(&in_flight).is_err() {
            eprintln!("the shell refused the fixture URL");
            return 1;
        }
        let settled = wait_until(&window, 20, || {
            shell.borrow().chrome().load_state == LoadState::Finished
        });
        check(
            &mut failures,
            settled,
            "the page is loaded again after the cancelled load",
        );

        // Story 2: Cmd+R reloads. Watched at the FIXTURE, because a settled load
        // state cannot tell a reload from "it was already loaded".
        let served_before = served.retrievals();
        window.press_key(0, "r", NSEventModifierFlags::Command);
        let reloaded = wait_until(&window, 20, || served.retrievals() > served_before);
        check(
            &mut failures,
            reloaded,
            "Cmd+R really re-fetches the page through the shell",
        );
        let settled = wait_until(&window, 20, || {
            shell.borrow().chrome().load_state == LoadState::Finished
        });
        check(&mut failures, settled, "the reloaded page settles again");

        // Story 7: the mouse's side buttons navigate history, through the SAME
        // resolution and the SAME performer. AppKit's synthetic-mouse constructor
        // carries no `buttonNumber`, so this drives the production path from the
        // number onwards (see `press_side_button`).
        check(
            &mut failures,
            window.press_side_button(input::BUTTON_NUMBER_BACK),
            "the rear side button is claimed by the chrome",
        );
        check(
            &mut failures,
            window.press_side_button(input::BUTTON_NUMBER_FORWARD),
            "the forward side button is claimed by the chrome",
        );
        check(
            &mut failures,
            !window.press_side_button(2),
            "an ordinary (middle) button stays the page's",
        );
        // Unlike before, there IS history behind those buttons by now (the two
        // in-flight loads the Escape pair needed left entries), so the rear button
        // really starts a navigation. Let it settle before the debug view's
        // store-clearing check reads the capture store, rather than racing a page
        // that is still running its own `console.log`.
        let settled = wait_until(&window, 20, || {
            shell.borrow().chrome().load_state == LoadState::Finished
        });
        check(
            &mut failures,
            settled,
            "the history the side buttons moved through settles",
        );

        // The web inspector is DELIBERATELY unhandled on this edge: macOS reaches
        // no inspector at all (`docs/platform-capability-matrix.toml`, owned by
        // `macos-web-inspector-safari-devtools`). F12 must therefore open nothing
        // and disturb nothing, rather than crash or claim the key.
        window.press_key(
            input::KEY_CODE_F12,
            "\u{f70f}",
            NSEventModifierFlags::empty(),
        );
        pump(&window);
        check(
            &mut failures,
            window.debug_row_counts().is_none(),
            "F12 opens no inspector here (macOS has none) and opens no other window",
        );

        println!("the debug view, opened through the core menu's Debug entry:");
        check(
            &mut failures,
            window.activate_menu_item(MENU_ITEM_DEBUG),
            "the Debug menu item is enabled and activatable",
        );
        // The page's own `console.log` must have travelled: page -> injected
        // shared shim -> capture channel -> shared store -> a rendered row.
        let captured = wait_until(&window, 5, || {
            window
                .debug_row_counts()
                .is_some_and(|(console, _)| console > 0)
        });
        check(
            &mut failures,
            captured,
            "the page's console.log reaches the debug view's Console tab",
        );
        check(
            &mut failures,
            window.debug_row_counts().is_some(),
            "the Debug entry opened the view",
        );
        // Clearing the SHARED store empties the view on the next tick.
        capture.clear();
        pump(&window);
        check(
            &mut failures,
            window.debug_row_counts() == Some((0, 0)),
            "clearing the shared store empties both tabs",
        );

        println!("the NEGATIVE CONTROL: bytes that do not hash to their CID:");
        let control = format!("ipfs://{tampered_cid}/");
        if shell.borrow_mut().navigate(&control).is_err() {
            eprintln!("the shell refused the control URL");
            return 1;
        }
        let failed = wait_until(&window, 20, || {
            shell.borrow().chrome().load_state == LoadState::Failed
        });
        check(&mut failures, failed, "the tampered load FAILS");
        let banner = window.error_banner();
        check(
            &mut failures,
            banner.is_some(),
            "a failed load raises the prominent error banner",
        );
        if let Some(text) = &banner {
            println!("       banner: {text}");
            check(
                &mut failures,
                text.contains("failed to load") || text.contains("timed out"),
                "the banner carries the core's protocol-named reason",
            );
        }
        check(
            &mut failures,
            window.page_frame().size.height < page_before.size.height,
            "a FAILURE is the one state allowed to displace the page",
        );
        check(
            &mut failures,
            !window.trust_text().contains("✓"),
            "a failed load is never reported verified",
        );

        println!("closing the debug window drops the slot:");
        if let Some(counts) = window.debug_row_counts() {
            let _ = counts;
        }
        // Re-open (it was left open), then close it the way a user does.
        window.open_debug_view();
        pump(&window);
        window.close_debug_view();
        pump(&window);
        check(
            &mut failures,
            window.debug_row_counts().is_none(),
            "closing the debug window clears the slot, so Debug opens a fresh one",
        );

        window.window().close();

        if failures.is_empty() {
            println!("\nwindow_smoke: PASS");
            0
        } else {
            println!("\nwindow_smoke: FAIL ({} checks)", failures.len());
            for failure in failures {
                println!("  - {failure}");
            }
            1
        }
    }
}
