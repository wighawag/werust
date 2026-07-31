//! Drive the REAL macOS backend through BOTH trust hooks, offline, and report
//! pass/fail: the CI job's "exercise the backend" step.
//!
//! A backend qualifies for werust on the TRUST HOOKS, not on rendering
//! (`CONTEXT.md`, `docs/adr/0001`). So this is not a "did a page render" smoke:
//! it asserts the two things that make a backend REAL.
//!
//! 1. **`ipfs://` interception serves HASH-VERIFIED content.** A canned page is
//!    stored under its own CIDv1, served through the PRODUCTION resolver
//!    (`werust_core::ipfs::resolve_ipfs_request` over a pinned, in-memory
//!    `ContentRetriever`) across the SHARED off-thread boundary
//!    (`webview_shared::offthread`), and the load must end
//!    [`TrustPosture::ContentVerified`].
//! 2. **The page sees the native EIP-1193 provider.** The document reports back
//!    over a script-message channel whether `window.ethereum` exists and whether
//!    `request({ method: 'eth_chainId' })` RESOLVES -- which only happens if the
//!    page -> native -> page round-trip completed.
//!
//! Plus a **negative control**: a second CID whose stored bytes do NOT hash to
//! it. That load must FAIL and must never be reported verified. A smoke where
//! everything passes has measured nothing.
//!
//! No gateway, no network, no signing, no packaging. Run it with
//! `cargo run -p macos-renderer --example trust_hooks_smoke` on a Mac; CI runs it
//! on the existing `macos-14` runner.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!(
        "trust_hooks_smoke drives a real WKWebView and only runs on macOS.\n\
         Run it via .github/workflows/macos-renderer.yml on the `macos-14` runner, or on a Mac\n\
         with `cargo run -p macos-renderer --example trust_hooks_smoke`."
    );
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() {
    std::process::exit(macos::run());
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::{Arc, Mutex};

    use fetcher::{cid_v1_raw_sha256, ContentRetriever, RetrieveError, RetrievedContent};
    use macos_renderer::MacosRenderer;
    use renderer::{Renderer, TrustPosture};
    use werust_core::ipfs::RedirectSink;

    /// The canned page. It reports what the two trust hooks actually gave it:
    /// the document origin, whether the injected provider is there, and whether a
    /// provider request round-trips.
    const PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>werust macos trust hooks</title></head>
<body><p>werust macOS trust-hook smoke</p>
<script>
(async function () {
  var r = { origin: String(location.origin), provider: typeof window.ethereum };
  try {
    r.chainId = await window.ethereum.request({ method: 'eth_chainId' });
  } catch (e) {
    r.chainId = 'reject:' + ((e && e.name) || String(e));
  }
  try { window.webkit.messageHandlers.werustSmoke.postMessage(JSON.stringify(r)); }
  catch (e) { document.title = 'bridge-failed:' + String(e); }
})();
</script>
</body></html>
"#;

    /// A pinned, in-memory `ContentRetriever`: no gateway, no network. It holds
    /// bytes under a CID and RE-VERIFIES them against that CID exactly as the real
    /// path does, so honest bytes resolve and tampered bytes are a hard mismatch.
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
            // The SAME check the verifying path makes: the bytes must hash to the
            // CID that named them.
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

    /// Turn the AppKit run loop for a moment, draining the seam in step so the
    /// off-thread `ipfs://` completions and the provider's response push are
    /// applied on the main thread.
    fn pump(renderer: &mut MacosRenderer, slices: u32) {
        use objc2_foundation::{NSDate, NSRunLoop};
        for _ in 0..slices {
            let until = NSDate::dateWithTimeIntervalSinceNow(0.02);
            NSRunLoop::currentRunLoop().runUntilDate(&until);
            while renderer.poll_event().is_some() {}
        }
    }

    fn wait_until(
        renderer: &mut MacosRenderer,
        seconds: u32,
        done: impl Fn(&MacosRenderer) -> bool,
    ) -> bool {
        for _ in 0..(seconds * 50) {
            pump(renderer, 1);
            if done(renderer) {
                return true;
            }
        }
        false
    }

    pub fn run() -> i32 {
        let honest = PAGE.as_bytes().to_vec();
        let honest_cid = cid_v1_raw_sha256(&honest).expect("derive the fixture cid");
        // The NEGATIVE CONTROL: a real CID naming DIFFERENT bytes.
        let claimed = b"the page this cid actually names".to_vec();
        let tampered_cid = cid_v1_raw_sha256(&claimed).expect("derive the control cid");
        let retriever = Arc::new(PinnedRetriever {
            honest_cid: honest_cid.clone(),
            honest,
            tampered_cid: tampered_cid.clone(),
            tampered: b"tampered bytes that do not hash to the cid".to_vec(),
        });

        let Ok(mut renderer) = MacosRenderer::new() else {
            eprintln!("could not create the macOS backend (is this the main thread?)");
            return 1;
        };

        // TRUST HOOK 2: `ipfs://` over the PRODUCTION verifying resolver and the
        // SHARED off-thread boundary -- just with a pinned retriever instead of a
        // gateway, so the run is offline and deterministic.
        let redirects = RedirectSink::new();
        renderer.install_verifying_scheme(
            "ipfs",
            Arc::new(move |uri: String| {
                webview_shared::offthread::retrieve_off_thread(retriever.as_ref(), uri, &redirects)
            }),
        );
        // TRUST HOOK 1: the native EIP-1193 provider over the script bridge.
        renderer.install_provider();

        // The page's report channel (this smoke's own, never the provider's).
        let reported: Arc<Mutex<Vec<String>>> = Arc::default();
        let sink = reported.clone();
        renderer.register_script_message_handler(
            "werustSmoke",
            Box::new(move |message| {
                if let Ok(mut got) = sink.lock() {
                    got.push(message.body);
                }
            }),
        );

        // A BARE host window: this task builds no chrome (that is the sibling
        // `macos-appkit-window-and-chrome`).
        let _window = renderer.host_in_bare_window();

        let mut failures: Vec<String> = Vec::new();

        // --- the verified load -------------------------------------------------
        let url = format!("ipfs://{honest_cid}/index.html");
        if let Err(error) = renderer.navigate(&url) {
            eprintln!("navigate({url}) refused: {error}");
            return 1;
        }
        let reported_something = wait_until(&mut renderer, 20, |_| {
            reported.lock().map(|got| !got.is_empty()).unwrap_or(false)
        });

        if !reported_something {
            failures.push(format!(
                "the verified `ipfs://` page never reported back (load state {:?}, url {:?})",
                renderer.load_state(),
                renderer.current_url()
            ));
        }
        let report = reported
            .lock()
            .ok()
            .and_then(|got| got.first().cloned())
            .unwrap_or_default();
        println!("page report: {report}");
        println!("load state:  {:?}", renderer.load_state());
        println!("posture:     {:?}", renderer.trust_posture());

        if renderer.trust_posture() != TrustPosture::ContentVerified {
            failures.push(format!(
                "an `ipfs://<cid>` load whose bytes hash-verified must be ContentVerified, got {:?}",
                renderer.trust_posture()
            ));
        }
        if !report.contains("\"provider\":\"object\"") {
            failures.push(format!(
                "the page must see a native EIP-1193 `window.ethereum` object, got {report}"
            ));
        }
        if !report.contains(&format!(
            "\"chainId\":\"{}\"",
            werust_core::provider::CHAIN_ID
        )) {
            failures.push(format!(
                "an `eth_chainId` request must round-trip page -> native -> page, got {report}"
            ));
        }
        if !renderer.trust_hooks().is_qualifying() || renderer::qualify(&renderer).is_err() {
            failures.push("the backend must declare BOTH trust hooks".to_string());
        }

        // --- the negative control ---------------------------------------------
        // Bytes that do NOT hash to the CID that named them: the load must fail
        // and must NEVER be reported verified.
        let control = format!("ipfs://{tampered_cid}/index.html");
        if let Err(error) = renderer.navigate(&control) {
            eprintln!("navigate({control}) refused: {error}");
            return 1;
        }
        wait_until(&mut renderer, 20, |r| {
            r.load_state() == renderer::LoadState::Failed
        });
        println!("control state:   {:?}", renderer.load_state());
        println!("control posture: {:?}", renderer.trust_posture());
        if renderer.trust_posture() != TrustPosture::UnverifiedOrigin {
            failures.push(format!(
                "a load whose bytes did NOT verify must never be reported verified, got {:?}",
                renderer.trust_posture()
            ));
        }
        if renderer.load_state() != renderer::LoadState::Failed {
            failures.push(format!(
                "a hash mismatch must FAIL the load (fail-closed), got {:?}",
                renderer.load_state()
            ));
        }

        if failures.is_empty() {
            println!("\nPASS: both trust hooks work on a real WKWebView, and the negative control failed as it must.");
            0
        } else {
            eprintln!("\nFAIL:");
            for failure in &failures {
                eprintln!("  - {failure}");
            }
            1
        }
    }
}
