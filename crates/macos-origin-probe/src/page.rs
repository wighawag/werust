//! The CANNED SITE both cases serve, and the URL case A serves it from.
//!
//! Pure and host-independent (so the Ubuntu `verify` gate covers the routing),
//! and deliberately tiny: no werust core, no IPFS, no network. The bytes below
//! are the smallest site that still does exactly what a SvelteKit
//! `adapter-static` client-side navigation does, which is the only thing the
//! origin question is actually about.
//!
//! It is a deliberate SIBLING of `crates/windows-origin-probe/src/page.rs` rather
//! than a shared module: the two probes differ in their host bridge (WebKit's
//! `window.webkit.messageHandlers` vs WebView2's `window.chrome.webview`) and in
//! their case sets. What IS shared -- on purpose, so the three platforms'
//! evidence is directly comparable -- is the CID, the paths, and the names of the
//! measured facts.

use crate::facts::CaseId;

/// The `ronan.eth` fixture root's canonical base32 CIDv1 -- the SAME CID the
/// committed Android probe (`SpaClientNavOriginTest.kt`), `origin_map.rs` and
/// `crates/windows-origin-probe` use, so all three platforms' evidence lines up.
pub const CID: &str = "bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq";

/// The scheme case A registers a `WKURLSchemeHandler` for.
pub const IPFS_SCHEME: &str = "ipfs";

/// The scheme the probe MEASURES as natively handled, to show why there is no
/// case B on WebKit (see the crate docs).
pub const NATIVELY_HANDLED_SCHEME: &str = "https";

/// The channel the page reports its outcome on.
pub const REPORT_BRIDGE: &str = "werustProbe";

/// The origin a case's document is EXPECTED to report. For the negative control
/// that is deliberately a counterfactual: it serves the same bytes, and the whole
/// point is that it must NOT report a tuple origin.
#[must_use]
pub fn case_origin(case: CaseId, cid: &str) -> String {
    match case {
        CaseId::A | CaseId::Control => format!("{IPFS_SCHEME}://{cid}"),
    }
}

/// The URL case A navigates to (the site root).
#[must_use]
pub fn case_page_url(case: CaseId, cid: &str) -> String {
    match case {
        CaseId::A => format!("{}/", case_origin(case, cid)),
        // The control is loaded from BYTES with a NIL base URL, so it has no URL
        // of its own: WebKit reports `about:blank`.
        CaseId::Control => "about:blank".to_string(),
    }
}

/// The path the SvelteKit-shaped client navigation fetches. Absolute (leading
/// `/`) so it resolves against the document's origin no matter what `pushState`
/// did to `location`.
pub const DATA_PATH: &str = "/blog/__data.json";

/// The CSS `url()` subresource path.
pub const FONT_PATH: &str = "/probe.woff2";

/// One canned resource: the bytes and the `Content-Type` to serve them with.
pub struct Resource {
    pub body: &'static [u8],
    pub content_type: &'static str,
}

/// Route a request URI to canned bytes. Everything is matched on the PATH, so a
/// case differs in nothing but the origin -- which is the entire experiment.
///
/// Returns `None` for anything the site does not define, so an unexpected request
/// fails loudly instead of being silently answered with HTML.
#[must_use]
pub fn resource_for(path: &str) -> Option<Resource> {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    match path {
        "/" | "/blog/" => Some(Resource {
            body: PAGE_HTML.as_bytes(),
            content_type: "text/html; charset=utf-8",
        }),
        DATA_PATH => Some(Resource {
            body: DATA_JSON.as_bytes(),
            content_type: "application/json",
        }),
        "/probe.mjs" => Some(Resource {
            body: MODULE_JS.as_bytes(),
            content_type: "text/javascript; charset=utf-8",
        }),
        "/sw.js" => Some(Resource {
            body: SERVICE_WORKER_JS.as_bytes(),
            content_type: "text/javascript; charset=utf-8",
        }),
        FONT_PATH => Some(Resource {
            // NOT a real font: the only question is "was the handler ASKED for a
            // CSS url() subresource", which is answered before a byte is parsed.
            body: NOT_REALLY_A_FONT,
            content_type: "font/woff2",
        }),
        _ => None,
    }
}

/// The path part of an absolute URI, for [`resource_for`].
#[must_use]
pub fn path_of(uri: &str) -> &str {
    let after_scheme = match uri.find("://") {
        Some(i) => &uri[i + 3..],
        None => return uri,
    };
    match after_scheme.find('/') {
        Some(i) => &after_scheme[i..],
        None => "/",
    }
}

const DATA_JSON: &str = r#"[{"type":"data","nodes":[]}]"#;
const MODULE_JS: &str = "export const probe = 'module';\n";
const SERVICE_WORKER_JS: &str = "self.addEventListener('install', function () {});\n";
const NOT_REALLY_A_FONT: &[u8] = b"wOF2-not-a-real-font";

/// The probe page. It reports its outcome TWICE -- over
/// `window.webkit.messageHandlers.werustProbe.postMessage` (the normal channel)
/// and into `document.title` (the fallback the host reads if the message channel
/// is itself a casualty of the origin the page ended up with).
pub const PAGE_HTML: &str = r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>werust origin probe</title>
<style>@font-face { font-family: 'ProbeFont'; src: url('/probe.woff2') format('woff2'); }</style>
</head>
<body>
<p style="font-family: 'ProbeFont'">werust origin probe</p>
<script>
(async function () {
  var r = {};
  function withTimeout(p, ms) {
    return Promise.race([
      p,
      new Promise(function (_resolve, reject) {
        setTimeout(function () { reject(new Error('timeout')); }, ms);
      })
    ]);
  }
  async function attempt(fn) {
    try { return await withTimeout(fn(), 5000); }
    catch (e) { return 'reject:' + ((e && e.name) || String(e)); }
  }
  function report() {
    var json = JSON.stringify(r);
    try { document.title = json; } catch (e) { /* fallback only */ }
    try { window.webkit.messageHandlers.werustProbe.postMessage(json); }
    catch (e) { r.postMessage = String(e); }
  }

  r.origin = String(location.origin);
  r.secureContext = !!window.isSecureContext;
  // Carried so a measured result always names the WebKit build it came from.
  r.userAgent = String(navigator.userAgent);

  // The one measurement that matters: the client router's data fetch, exactly
  // the URL shape SvelteKit uses.
  r.fetch = await attempt(async function () {
    var resp = await fetch('/blog/__data.json?x-sveltekit-invalidated=01');
    return 'ok:' + resp.status;
  });

  // The other one: the router's URL update. Throws SecurityError on an opaque
  // origin (the Android failure).
  try {
    history.pushState({}, '', '/blog/');
    r.pushState = 'ok:' + location.pathname;
  } catch (e) {
    r.pushState = 'throw:' + ((e && e.name) || String(e));
  }

  // Informational from here down; never grounds for the verdict.
  r.moduleScript = await attempt(async function () {
    var m = await import('/probe.mjs');
    return 'ok:' + m.probe;
  });
  r.cssFont = await attempt(async function () {
    await document.fonts.load("12px ProbeFont");
    return 'ok';
  });
  r.serviceWorker = await attempt(async function () {
    if (!navigator.serviceWorker) { return 'unavailable'; }
    await navigator.serviceWorker.register('/sw.js');
    return 'ok';
  });

  report();
})();
</script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_a_serves_the_real_ipfs_origin() {
        assert_eq!(case_origin(CaseId::A, CID), format!("ipfs://{CID}"));
        assert_eq!(case_page_url(CaseId::A, CID), format!("ipfs://{CID}/"));
    }

    /// The control differs from case A in the SERVING MECHANISM only -- not in
    /// the bytes or the page -- because anything else would make it a different
    /// experiment instead of a control. WebKit exposes no
    /// `HasAuthorityComponent`-style flag to flip (that was the Windows probe's
    /// one-variable control), so the one variable here is whether the document
    /// came from the registered handler at all.
    #[test]
    fn the_negative_control_serves_case_as_bytes_from_no_handler_origin() {
        assert_eq!(case_page_url(CaseId::Control, CID), "about:blank");
        assert_eq!(
            case_origin(CaseId::Control, CID),
            case_origin(CaseId::A, CID),
            "the control is measured against the SAME expected origin, and must fail to report it"
        );
    }

    #[test]
    fn the_cid_matches_the_android_and_windows_probes() {
        // Same fixture root as `SpaClientNavOriginTest.kt`,
        // `crates/werust-android/rust/src/origin_map.rs` and
        // `crates/windows-origin-probe`, so the three platforms are comparable.
        assert!(CID.starts_with("bafybei"));
        assert_eq!(CID.len(), 59);
    }

    #[test]
    fn the_canned_site_routes_every_path_the_page_asks_for() {
        let uri = format!("ipfs://{CID}/blog/__data.json?x-sveltekit-invalidated=01");
        assert_eq!(
            path_of(&uri),
            "/blog/__data.json?x-sveltekit-invalidated=01"
        );
        assert_eq!(
            resource_for(path_of(&uri))
                .expect("the data route is canned")
                .content_type,
            "application/json"
        );
        for path in ["/", "/blog/"] {
            let resource = resource_for(path).expect("the page is canned");
            assert_eq!(resource.content_type, "text/html; charset=utf-8");
            assert_eq!(resource.body, PAGE_HTML.as_bytes());
        }
        assert!(resource_for("/probe.mjs").is_some());
        assert!(resource_for(FONT_PATH).is_some());
        assert!(resource_for("/sw.js").is_some());
    }

    #[test]
    fn the_bare_origin_and_the_rooted_origin_both_mean_the_site_root() {
        let bare = format!("ipfs://{CID}");
        let rooted = format!("ipfs://{CID}/");
        assert_eq!(path_of(&bare), "/");
        assert_eq!(path_of(&rooted), "/");
    }

    #[test]
    fn an_undefined_path_is_not_silently_answered_with_html() {
        assert!(resource_for("/favicon.ico").is_none());
        assert!(resource_for("/nope").is_none());
    }

    /// The page must actually exercise the four measured facts, or the probe
    /// would report defaults and look like a pass.
    #[test]
    fn the_probe_page_measures_every_fact_the_verdict_needs() {
        for needle in [
            "location.origin",
            "isSecureContext",
            "/blog/__data.json?x-sveltekit-invalidated=01",
            "history.pushState",
            "navigator.userAgent",
            "webkit.messageHandlers.werustProbe",
        ] {
            assert!(
                PAGE_HTML.contains(needle),
                "the probe page must exercise {needle}"
            );
        }
        assert!(
            PAGE_HTML.contains(REPORT_BRIDGE),
            "the page must post on the channel the host registers"
        );
    }
}
