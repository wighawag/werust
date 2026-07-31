//! Build the REAL Win32 window, drive it through a real load, and assert what
//! the real widgets show: the CI job's "exercise the window" step.
//!
//! Everything about this window that a Linux gate can check is checked there --
//! the shared `desktop-paint` carrier is unit-tested against the real
//! `werust-core`, the durable-profile rule is unit-tested, and
//! `tests/windows_window_shape.rs` guards the wiring. What NO Linux gate can do
//! is CREATE an `EDIT`, a `STATIC`, an `HMENU` and a `SysListView32` and read
//! back what they hold. That is this smoke's whole job, and it is why it asserts
//! on the WIDGETS (`window.url_text()`, `window.trust_text()`,
//! `window.trust_detail()`, `window.error_banner()`) rather than on the paint
//! snapshot it already trusts.
//!
//! It is OFFLINE and deterministic: the `ipfs://` route is the PRODUCTION
//! verifying resolver over a pinned, in-memory retriever (the same fixture shape
//! the engine's `trust_hooks_smoke` uses), so a hash-verified page and a
//! hash-MISMATCHING control both come from memory. No gateway, no network, no
//! signing, no packaging.
//!
//! It also MEASURES the chrome's geometry against the DPI seam
//! (`werust_windows::dpi`), which is the only run-time check of the DPI work
//! that exists anywhere: a CI runner has no scaled display, so at 96 DPI these
//! assertions prove the layout is COMPUTED from the seam rather than from
//! constants, and the SAME assertions run on a human's 150%/200% display prove it
//! scales. What only a human can judge is listed in
//! `docs/spikes/windows-chrome-must-scale-with-the-display-dpi/README.md`.
//!
//! The window is opened FAR off-screen and never activated, so a CI run shows
//! nothing and steals no focus.
//!
//! Run it with `cargo run -p werust-windows --example window_smoke` on Windows;
//! CI runs it on the `windows-latest` runner.

#[cfg(not(windows))]
fn main() {
    eprintln!(
        "window_smoke builds a real Win32 window and only runs on Windows.\n\
         Run it via .github/workflows/windows-renderer.yml on the `windows-latest` runner, or on\n\
         a Windows box with `cargo run -p werust-windows --example window_smoke`."
    );
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    std::process::exit(windows_smoke::run());
}

#[cfg(windows)]
mod windows_smoke {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use fetcher::{cid_v1_raw_sha256, ContentRetriever, RetrieveError, RetrievedContent};
    use renderer::LoadState;
    use werust_core::debug::DebugCapture;
    use werust_core::ipfs::RedirectSink;
    use werust_core::menu::{BrowserMenu, MENU_ITEM_DEBUG};
    use werust_core::{status_line, trust_indicator, trust_indicator_detail, BrowserShell};
    use werust_windows::dpi::{Dpi, Metrics};
    use werust_windows::paint::install_debug_capture;
    use werust_windows::window::{BrowserWindow, Placement};

    /// The canned page. Its `console.log` is what proves the CONSOLE capture
    /// point really reaches the debug view's Console tab.
    const PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>werust windows window smoke</title></head>
<body><p>werust Windows window smoke</p>
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
    }

    impl ContentRetriever for PinnedRetriever {
        fn retrieve(&self, cid: &str, _path: &str) -> Result<RetrievedContent, RetrieveError> {
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

    /// Turn the Win32 message loop and pump the window once, so the seam's
    /// events, the off-thread `ipfs://` completions and the chrome all advance
    /// together. WebView2 delivers EVERY event through the message loop, so a
    /// driver that never pumps sees nothing happen at all.
    fn pump(window: &BrowserWindow) {
        window.pump_messages();
        std::thread::sleep(std::time::Duration::from_millis(20));
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
        });

        // The SHELL's durable profile, not the engine's `%TEMP%` default: the
        // same path `window::run` passes, so this run exercises the real rule.
        let profile = werust_windows::profile::user_data_folder();
        println!("profile folder: {profile:?}");
        let backend = match profile.clone() {
            Some(folder) => windows_renderer::Webview2Renderer::with_user_data_folder(folder),
            None => windows_renderer::Webview2Renderer::new(),
        };
        let Ok(mut backend) = backend else {
            eprintln!("could not create the WebView2 backend (is the runtime installed?)");
            return 1;
        };
        match windows_renderer::Webview2Renderer::runtime_version() {
            Ok(version) => println!("WebView2 Runtime: {version}"),
            Err(e) => println!("WebView2 Runtime: unknown ({e})"),
        }

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
        let dev_tools = backend.dev_tools();

        let shell = Rc::new(RefCell::new(
            BrowserShell::new(Box::new(backend))
                .with_redirect_sink(redirects)
                .with_debug_capture(capture.clone()),
        ));
        let window = match BrowserWindow::open(
            shell.clone(),
            capture.clone(),
            dev_tools,
            // FAR off-screen: a CI run shows nothing.
            Placement::OffScreen,
        ) {
            Ok(window) => window,
            Err(e) => {
                eprintln!("could not open the window: {e}");
                return 1;
            }
        };

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

        // The DPI seam, measured off the REAL widgets. `app.manifest` declares
        // `PerMonitorV2`, so Windows scales nothing for this process and every
        // rectangle above had to be computed for this display's scale.
        println!("the chrome is laid out from the DPI seam, at this display's scale:");
        let dpi = window.dpi();
        let metrics = Metrics::at(Dpi::new(dpi));
        println!(
            "       GetDpiForWindow = {dpi} ({}% of the 96-DPI baseline)",
            dpi * 100 / 96
        );
        check(
            &mut failures,
            window.metrics() == metrics,
            "the window's metrics are the seam's, for the DPI Windows reported",
        );
        let page = window.page_client_rect();
        println!(
            "       page top: {} (toolbar: {})",
            page.top, metrics.toolbar_height
        );
        check(
            &mut failures,
            page.top == metrics.toolbar_height,
            "the page starts exactly one SCALED toolbar down",
        );
        let url = window.control_rect(window.url_bar());
        check(
            &mut failures,
            url.top == metrics.row_y && url.bottom - url.top == metrics.row_height,
            "the URL bar is exactly the seam's toolbar row, not a 96-DPI constant",
        );
        let trust = window.control_rect(window.trust());
        check(
            &mut failures,
            trust.right - trust.left == metrics.trust_width,
            "the trust indicator is exactly the seam's scaled width",
        );
        // Say plainly what this run can and cannot claim.
        if dpi == 96 {
            println!(
                "       NOTE: this display is UNSCALED, so the checks above prove the layout is\n\
                 \x20      computed from the seam, NOT that it scales. Only a human on a 150%/200%\n\
                 \x20      display can close that; see the spike README."
            );
        } else {
            println!(
                "       this display IS scaled, so the checks above also prove the chrome scales"
            );
        }

        println!("the ⋮ menu is the shared core's BrowserMenu, item for item:");
        let expected: Vec<String> = BrowserMenu::new()
            .items()
            .iter()
            .map(|item| item.label.clone())
            .collect();
        let titles = window.menu_titles();
        println!("       menu: {titles:?}");
        check(
            &mut failures,
            titles == expected,
            "the HMENU's titles are the core's item labels, in order",
        );

        println!("a hash-VERIFIED ipfs:// page (offline, pinned):");
        let url = format!("ipfs://{honest_cid}/");
        let page_before = window.page_rect();
        if shell.borrow_mut().navigate(&url).is_err() {
            eprintln!("the shell refused the fixture URL");
            return 1;
        }
        let settled = wait_until(&window, 30, || {
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
                window.trust_detail().as_deref() == Some(trust_indicator_detail(chrome)),
                "the settled badge's EXPLANATION is the core's, not a stale one",
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
            !window.progress_visible(),
            "the URL bar's progress strip is gone once the load settled",
        );
        check(
            &mut failures,
            window.page_rect() == page_before,
            "the page did NOT move or resize across a whole load",
        );

        println!("the DURABLE profile (not the engine's %TEMP% default):");
        let durable = profile
            .as_ref()
            .is_some_and(|folder| folder.exists() && !folder.starts_with(std::env::temp_dir()));
        if let Some(folder) = &profile {
            println!("       {}", folder.display());
        }
        check(
            &mut failures,
            durable,
            "the WebView2 profile really exists under %LOCALAPPDATA%, outside %TEMP%",
        );

        println!("the debug view, opened through the core menu's Debug entry:");
        check(
            &mut failures,
            window.activate_menu_item(MENU_ITEM_DEBUG),
            "the Debug menu item is enabled and activatable",
        );
        // The page's own `console.log` must have travelled: page -> injected
        // shared shim -> capture channel -> shared store -> a rendered row.
        let captured = wait_until(&window, 10, || {
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

        println!("devtools are the PLATFORM's own (OpenDevToolsWindow):");
        check(
            &mut failures,
            window.open_dev_tools(),
            "the shell opens Edge's real DevTools window over the live page",
        );
        pump(&window);

        println!("the NEGATIVE CONTROL: bytes that do not hash to their CID:");
        let control = format!("ipfs://{tampered_cid}/");
        if shell.borrow_mut().navigate(&control).is_err() {
            eprintln!("the shell refused the control URL");
            return 1;
        }
        let failed = wait_until(&window, 30, || {
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
        let page_after = window.page_rect();
        check(
            &mut failures,
            (page_after.bottom - page_after.top) < (page_before.bottom - page_before.top),
            "a FAILURE is the one state allowed to displace the page",
        );
        check(
            &mut failures,
            !window.trust_text().contains("✓"),
            "a failed load is never reported verified",
        );

        println!("closing the debug window drops the slot:");
        window.open_debug_view();
        pump(&window);
        window.close_debug_view();
        pump(&window);
        check(
            &mut failures,
            window.debug_row_counts().is_none(),
            "closing the debug window clears the slot, so Debug opens a fresh one",
        );

        window.close();

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
