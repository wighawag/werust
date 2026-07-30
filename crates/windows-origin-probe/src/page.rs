//! The CANNED SITE both cases serve, and the URLs each case serves it from.
//!
//! Pure and host-independent (so the Ubuntu `verify` gate covers the routing),
//! and deliberately tiny: no werust core, no IPFS, no network. The bytes below
//! are the smallest site that still does exactly what a SvelteKit
//! `adapter-static` client-side navigation does, which is the only thing the
//! origin question is actually about.

use crate::facts::CaseId;

/// The `ronan.eth` fixture root's canonical base32 CIDv1 — the SAME CID the
/// committed Android probe (`SpaClientNavOriginTest.kt`) and `origin_map.rs`
/// use, so the two platforms' evidence is directly comparable.
pub const CID: &str = "bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq";

/// The scheme case A registers with `HasAuthorityComponent` + `TreatAsSecure`.
pub const IPFS_SCHEME: &str = "ipfs";

/// The internal host suffix case B serves under — the exact string
/// `crates/werust-android/rust/src/origin_map.rs` already implements.
pub const INTERNAL_IPFS_HOST_SUFFIX: &str = ".ipfs.werust.invalid";

/// The origin a case's document WOULD report if its mechanism gave a real
/// tuple origin. For the negative control that is deliberately a
/// counterfactual: it serves the same `ipfs://<cid>/` URL, and the whole point
/// is that it must NOT report this origin.
pub fn case_origin(case: CaseId, cid: &str) -> String {
    match case {
        CaseId::A | CaseId::Control => format!("{IPFS_SCHEME}://{cid}"),
        CaseId::B => format!("https://{cid}{INTERNAL_IPFS_HOST_SUFFIX}"),
    }
}

/// The URL the case navigates to (the site root).
pub fn case_page_url(case: CaseId, cid: &str) -> String {
    format!("{}/", case_origin(case, cid))
}

/// The `AddWebResourceRequestedFilter` pattern that covers the case's origin —
/// the origin itself plus a trailing wildcard, the same prefix shape `wry`
/// filters on. Deliberately ONE filter per case: WebView2 raises the event once
/// per matching filter, so overlapping filters would double-count the very
/// thing this probe measures.
pub fn case_filter(case: CaseId, cid: &str) -> String {
    format!("{}*", case_origin(case, cid))
}

/// The path the SvelteKit-shaped client navigation fetches. Absolute (leading
/// `/`) so it resolves against the document's origin no matter what
/// `pushState` did to `location`.
pub const DATA_PATH: &str = "/blog/__data.json";

/// The CSS `url()` subresource path (the WebView2Feedback #4362 shape).
pub const FONT_PATH: &str = "/probe.woff2";

/// One canned resource: the bytes and the `Content-Type` to serve them with.
pub struct Resource {
    pub body: &'static [u8],
    pub content_type: &'static str,
}

/// Route a request URI to canned bytes. Everything is matched on the PATH, so
/// the identical map serves both cases and the two mechanisms differ in nothing
/// but the origin — which is the entire experiment.
///
/// Returns `None` for anything the site does not define, so an unexpected
/// request fails loudly instead of being silently answered with HTML.
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
            // NOT a real font: the #4362 question is only ever "was the handler
            // ASKED for a CSS url() subresource", which is answered before a
            // single byte is parsed.
            body: NOT_REALLY_A_FONT,
            content_type: "font/woff2",
        }),
        _ => None,
    }
}

/// The path part of an absolute URI, for [`resource_for`]. Scheme-agnostic on
/// purpose: `ipfs://<cid>/blog/` and `https://<cid>.ipfs.werust.invalid/blog/`
/// must land on the same canned bytes.
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

/// The probe page. It reports its outcome TWICE — over
/// `window.chrome.webview.postMessage` (the normal channel) and into
/// `document.title` (the fallback the host reads if the message channel is
/// itself a casualty of the origin the page ended up with).
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
    try { window.chrome.webview.postMessage(json); } catch (e) { r.postMessage = String(e); }
  }

  r.origin = String(location.origin);
  r.secureContext = !!window.isSecureContext;

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
    fn case_a_serves_the_real_ipfs_origin_and_case_b_the_internal_one() {
        assert_eq!(case_origin(CaseId::A, CID), format!("ipfs://{CID}"));
        assert_eq!(
            case_origin(CaseId::B, CID),
            format!("https://{CID}.ipfs.werust.invalid")
        );
        assert_eq!(case_page_url(CaseId::A, CID), format!("ipfs://{CID}/"));
    }

    /// The control differs from case A in ONE registration flag, not in the
    /// URL, the bytes or the page: anything else would make it a different
    /// experiment instead of a control.
    #[test]
    fn the_negative_control_serves_case_as_url_from_case_as_bytes() {
        assert_eq!(
            case_page_url(CaseId::Control, CID),
            case_page_url(CaseId::A, CID)
        );
        assert_eq!(
            case_filter(CaseId::Control, CID),
            case_filter(CaseId::A, CID)
        );
    }

    /// The internal origin must be byte-identical to what `origin_map.rs`
    /// produces, or case B is not measuring the mechanism werust already owns.
    #[test]
    fn the_internal_origin_matches_the_android_origin_map() {
        assert_eq!(INTERNAL_IPFS_HOST_SUFFIX, ".ipfs.werust.invalid");
    }

    #[test]
    fn one_filter_per_case_covers_that_cases_origin_including_its_root() {
        // The trailing wildcard has to cover the document itself
        // (`<origin>/`), every subresource, and the bare origin.
        assert_eq!(case_filter(CaseId::A, CID), format!("ipfs://{CID}*"));
        assert_eq!(
            case_filter(CaseId::B, CID),
            format!("https://{CID}.ipfs.werust.invalid*")
        );
        for case in CaseId::ALL {
            let filter = case_filter(case, CID);
            let prefix = filter.trim_end_matches('*');
            assert!(case_page_url(case, CID).starts_with(prefix));
        }
    }

    #[test]
    fn both_schemes_route_to_the_same_canned_bytes() {
        let ipfs_uri = format!("ipfs://{CID}/blog/__data.json?x-sveltekit-invalidated=01");
        let internal_uri = format!(
            "https://{CID}.ipfs.werust.invalid/blog/__data.json?x-sveltekit-invalidated=01"
        );
        let ipfs = path_of(&ipfs_uri);
        let internal = path_of(&internal_uri);
        assert_eq!(ipfs, internal);
        assert_eq!(
            resource_for(ipfs)
                .expect("the data route is canned")
                .content_type,
            "application/json"
        );
    }

    #[test]
    fn the_site_root_and_the_pushed_route_both_serve_the_probe_page() {
        let bare = format!("ipfs://{CID}");
        let rooted = format!("ipfs://{CID}/");
        assert_eq!(path_of(&bare), "/");
        assert_eq!(path_of(&rooted), "/");
        for path in ["/", "/blog/"] {
            let resource = resource_for(path).expect("the page is canned");
            assert_eq!(resource.content_type, "text/html; charset=utf-8");
            assert_eq!(resource.body, PAGE_HTML.as_bytes());
        }
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
            "chrome.webview.postMessage",
        ] {
            assert!(
                PAGE_HTML.contains(needle),
                "the probe page must exercise {needle}"
            );
        }
    }
}
