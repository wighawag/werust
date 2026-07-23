//! Native `ipfs://` scheme resolution over the [`Renderer`] seam's
//! custom-scheme / request-interception hook, backed by the hash-verified
//! content-addressed [`Fetcher`](fetcher::ContentAddressedFetcher) path.
//!
//! This module is the toolkit-free heart of werust's SECOND trust hook
//! (`CONTEXT.md`, `docs/adr/0001`): an `ipfs://<cid>/…` URL typed in the URL bar
//! is intercepted at the seam, its CID resolved through the
//! [`ContentAddressedFetcher`](fetcher::ContentAddressedFetcher) — which returns
//! bytes ONLY after they verify against the CID's hash — and the verified bytes
//! rendered on the webview backend, at parity with a served page. Verification
//! GATES the load: a hash mismatch (or any other verify failure) must NOT render
//! unverified bytes, it must fail the load.
//!
//! It is split so the whole scheme -> verified-fetch -> render path is testable
//! WITHOUT a webview or a GTK main loop, mirroring the [`provider`](crate::provider)
//! split:
//!
//! * [`parse_ipfs_uri`] turns the intercepted `ipfs://<cid>[/path]` URI into the
//!   [`IpfsRef`] the resolver needs (the CID to verify + the path, from which the
//!   response MIME type is inferred for served-page parity).
//! * [`resolve_ipfs_request`] is the pure resolver: it parses the request URI,
//!   resolves the CID through a [`ContentAddressedFetcher`](fetcher::ContentAddressedFetcher),
//!   and returns a [`SchemeResponse`] with the verified bytes — or a
//!   [`RendererError`] that FAILS the load. It never returns bytes the fetcher did
//!   not verify.
//!
//! The concrete production [`ContentSource`](fetcher::ContentSource) (an IPFS
//! gateway over the HTTP [`Fetcher`](fetcher::Fetcher)) is wired where the backend
//! lives (the webview backend's `install_ipfs`), exactly as the provider's live
//! response push is; this module owns the pure resolution the installer delegates
//! to, exercised headlessly by its tests against a pinned fixture CID.

use fetcher::{ContentRetriever, RetrieveError};
use renderer::{RendererError, SchemeRequest, SchemeResponse};

/// The custom scheme this module resolves: `ipfs`.
///
/// Kept as one constant so the backend that registers the scheme handler
/// (`install_ipfs`) and this resolver agree on the single scheme name. A backend
/// registers a handler for `<IPFS_SCHEME>://…` requests and routes each through
/// [`resolve_ipfs_request`].
pub const IPFS_SCHEME: &str = "ipfs";

/// The default MIME type for an `ipfs://` response whose path gives no better
/// hint (the CID root, or a path with no recognized extension).
///
/// A content-addressed page is a page: the default is `text/html` so an
/// `ipfs://<cid>` (or `ipfs://<cid>/`) load renders as a document at parity with
/// a served page, rather than being offered for download.
const DEFAULT_MIME_TYPE: &str = "text/html";

/// A parsed `ipfs://<cid>[/path]` reference: the CID to resolve-and-verify plus
/// the path used only to infer the response MIME type.
///
/// The `cid` is the content identifier the [`ContentRetriever`] resolves and
/// verifies against; the `path` is the remainder after the CID authority (empty
/// for `ipfs://<cid>` or `ipfs://<cid>/`). The path is now LOAD-BEARING: it
/// selects the sub-resource within the verified UnixFS DAG (a directory root
/// resolves to its `index.html`; `ipfs://<cid>/sub/resource` resolves that
/// resource into the DAG), and it also informs the response MIME type for
/// served-page parity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpfsRef {
    /// The content identifier (the `<cid>` authority) to fetch and verify.
    pub cid: String,
    /// The path after the CID authority (e.g. `/index.html`), or empty.
    pub path: String,
}

/// Parse an intercepted `ipfs://<cid>[/path]` URI into an [`IpfsRef`].
///
/// The CID is the authority between `ipfs://` and the first `/`; anything after
/// (including the `/`) is the path. A URI that is not `ipfs://…`, or that carries
/// no CID authority, is rejected with [`RendererError::InvalidUrl`] — a malformed
/// content reference cannot name anything to verify, so it fails the load rather
/// than guessing. (The CID string is NOT validated here; that is the
/// [`ContentRetriever`]'s job, which rejects a malformed CID as its own verify
/// failure so the trust boundary stays in one place.)
pub fn parse_ipfs_uri(uri: &str) -> Result<IpfsRef, RendererError> {
    let rest = uri
        .strip_prefix("ipfs://")
        .ok_or_else(|| RendererError::InvalidUrl(uri.to_string()))?;
    // The CID authority is up to the first '/'; the rest (with its leading '/')
    // is the path. `ipfs://<cid>` has no '/', so the whole remainder is the CID.
    let (cid, path) = match rest.split_once('/') {
        Some((cid, tail)) => (cid, format!("/{tail}")),
        None => (rest, String::new()),
    };
    if cid.is_empty() {
        return Err(RendererError::InvalidUrl(uri.to_string()));
    }
    Ok(IpfsRef {
        cid: cid.to_string(),
        path,
    })
}

/// Canonicalize an `ipfs://`-family URL to a STABLE key on the CID identity, for
/// keying the shell's `ens_pages` CID<->name map identically at insert and at
/// every lookup.
///
/// The problem this solves: werust stores an authority-form `ipfs://<cid>` at
/// forward-load time (what `current_url` reports right after `navigate`), but
/// WebKitGTK reports the SAME history entry back as an authority-LESS
/// `ipfs:///<cid>` (triple slash: the CID moved into the path, empty authority),
/// and may add or drop a trailing slash. Keyed on the raw display string, the
/// stored key and the post-back key differ, so the back/forward re-derive misses
/// and the raw CID leaks into the bar. Reducing BOTH forms to the same
/// `<cid>[/path]` key (dropping the scheme, any empty authority, and a bare
/// trailing slash) makes the forward-store key and the post-back key identical,
/// so a WebKit-normalized variant of the same entry still matches.
///
/// A non-`ipfs://` URL (a plain served page) has no CID identity to canonicalize
/// and is returned UNCHANGED, so a plain history entry keeps keying on its exact
/// URL and is wholly unaffected by the ENS association.
#[must_use]
pub fn normalize_ens_page_key(url: &str) -> String {
    // Accept both the authority form (`ipfs://<cid>[/path]`) and the WebKit
    // authority-less form (`ipfs:///<cid>[/path]`); the CID is the first non-empty
    // segment, the rest (with its leading `/`) is the path.
    let Some(rest) = url.strip_prefix("ipfs://") else {
        return url.to_string();
    };
    // `ipfs:///<cid>` leaves a leading `/` (the empty authority); drop it so the
    // CID is the first segment in both forms.
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    let (cid, path) = match rest.split_once('/') {
        Some((cid, tail)) => (cid, tail),
        None => (rest, ""),
    };
    // A bare trailing slash (`ipfs://<cid>/`) is the same entry as `ipfs://<cid>`;
    // normalize it away so the two forms share one key.
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        cid.to_string()
    } else {
        format!("{cid}/{path}")
    }
}

/// Split an `ipfs://`-family URL into its ROOT CID and the in-site path (with a
/// leading `/`, or `""` at the root), for the shell's root-CID-PREFIX ENS
/// association: recognise ANY `<rootcid>/<path>` as being UNDER a known ENS
/// site's root CID.
///
/// Returns [`None`] for a non-`ipfs://` URL (a plain served page has no CID
/// identity, so it never matches a known ENS site — plain pages are wholly
/// unaffected). Built on [`normalize_ens_page_key`] so it accepts BOTH the
/// authority form (`ipfs://<cid>[/path]`) and the WebKit authority-less form
/// (`ipfs:///<cid>[/path]`) and shares the same CID/path canonicalization the
/// `ens_pages` keys use, so the stored root CID and a post-back sub-path URL
/// split to the SAME root CID.
///
/// The v0.2.4 leak this closes: `ens_pages` was keyed on the exact normalized
/// entry (the bare `<rootcid>` root, or `<rootcid>/blog` for a `.eth/blog`
/// entry), so a history return / SPA nav onto a DIFFERENT sub-path
/// (`<rootcid>/blog/post-1`) missed the exact-key lookup and leaked the raw CID.
/// Splitting the current URL to its root CID lets the shell match it against a
/// known site's root CID and re-derive `name/<in-site-path>` for ANY sub-path.
#[must_use]
pub fn ipfs_root_cid_and_path(url: &str) -> Option<(String, String)> {
    url.strip_prefix("ipfs://")?;
    // `normalize_ens_page_key` reduces both forms to `<cid>` or `<cid>/path`
    // (dropping the scheme, any empty authority, and a bare trailing slash).
    let key = normalize_ens_page_key(url);
    match key.split_once('/') {
        Some((cid, path)) => Some((cid.to_string(), format!("/{path}"))),
        None => Some((key, String::new())),
    }
}

/// Infer the response MIME type from an `ipfs://` reference's path, for
/// served-page parity.
///
/// A content-addressed resource is rendered like the same bytes served over
/// `http(s)://`, so the MIME type is derived from the path's extension the way a
/// static file server would. Unknown or absent extensions fall back to
/// [`DEFAULT_MIME_TYPE`] (`text/html`), so a bare `ipfs://<cid>` opens as a page.
fn mime_type_for_path(path: &str) -> &'static str {
    let ext = path.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    match ext.to_ascii_lowercase().as_str() {
        "html" | "htm" | "" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" => "text/plain",
        "wasm" => "application/wasm",
        "xml" => "application/xml",
        _ => DEFAULT_MIME_TYPE,
    }
}

/// Map a content-retrieval [`RetrieveError`] onto the seam's [`RendererError`],
/// so EVERY retrieval failure fails the load instead of rendering.
///
/// This is the load-bearing gate: a [`BlockHashMismatch`](RetrieveError::BlockHashMismatch)
/// (a block did not match its CID: tamper), a [`MissingBlock`](RetrieveError::MissingBlock)
/// / [`IncompleteCar`](RetrieveError::IncompleteCar) (the DAG was incomplete),
/// a [`PathNotFound`](RetrieveError::PathNotFound) (the sub-resource did not
/// resolve), a [`BudgetExceeded`](RetrieveError::BudgetExceeded) (a runaway
/// DAG), an [`UnsupportedCodec`](RetrieveError::UnsupportedCodec) /
/// [`UnsupportedHash`](RetrieveError::UnsupportedHash) /
/// [`InvalidCid`](RetrieveError::InvalidCid) (unverifiable), or a
/// [`Source`](RetrieveError::Source) failure ALL become a
/// [`RendererError::Backend`] the scheme handler returns, which the backend
/// surfaces as a failed load. None of them ever yields bytes to render:
/// rejecting-when-unsure is the whole trust stance (`docs/adr/0001`). The
/// distinct cause is preserved in the message so the failure is legible.
fn retrieve_error_to_renderer_error(err: RetrieveError) -> RendererError {
    RendererError::Backend(format!("ipfs:// content-addressed load failed: {err}"))
}

/// Resolve an intercepted `ipfs://` [`SchemeRequest`] through the verifiable
/// content-retrieval [`ContentRetriever`](fetcher::ContentRetriever) seam,
/// returning the verified bytes as a [`SchemeResponse`] to render, or a
/// [`RendererError`] that FAILS the load.
///
/// This is the pure heart of the scheme -> verified-retrieve -> render path,
/// split out so it is testable WITHOUT a webview: a live backend registers an
/// `ipfs` scheme handler that calls this with each intercepted request and a
/// [`ContentRetriever`](fetcher::ContentRetriever) backed by a production backend
/// (the trustless-gateway CAR fetcher over the HTTP [`Fetcher`](fetcher::Fetcher)).
///
/// The CID AND the path are resolved through
/// [`retrieve`](fetcher::ContentRetriever::retrieve), which walks the UnixFS DAG,
/// verifies EVERY block against its own CID, resolves the path (a directory root
/// to its `index.html`; each `ipfs://<cid>/sub/resource` into the DAG), and
/// reassembles the leaf bytes locally. So a tamper (a mis-hashing block), an
/// incomplete DAG, an unresolved path, or a budget overflow each surface here as
/// a [`RendererError`] and NOTHING is rendered: verification gates the load. On
/// success the verified bytes are handed back with a MIME type inferred from the
/// path for served-page parity.
pub fn resolve_ipfs_request(
    retriever: &dyn ContentRetriever,
    request: &SchemeRequest,
) -> Result<SchemeResponse, RendererError> {
    let reference = parse_ipfs_uri(&request.uri)?;
    // Route THROUGH the verifying retriever: bytes come back only after every
    // block in the resolved resource's DAG hashed to its own CID. Any failure is
    // a hard failure that fails the load, never a silent render of unverified
    // bytes.
    let content = retriever
        .retrieve(&reference.cid, &reference.path)
        .map_err(retrieve_error_to_renderer_error)?;
    Ok(SchemeResponse {
        mime_type: mime_type_for_path(&reference.path).to_string(),
        body: content.bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fetcher::RetrievedContent;
    use std::collections::HashMap;

    /// A pinned, in-memory [`ContentRetriever`] double, isolated from the live
    /// network, that returns pre-registered verified bytes for a `(cid, path)`
    /// pair or a chosen [`RetrieveError`].
    ///
    /// The real per-block CAR verify / DAG walk / budget mechanics are exercised
    /// against real CAR fixtures in the `fetcher` crate's `retriever` tests and
    /// the native-renderer content-addressed floor tests; here we drive the pure
    /// `resolve_ipfs_request` glue (URI parse -> retrieve -> MIME + response, and
    /// every failure fails the load) at the seam, headlessly.
    #[derive(Default)]
    struct PinnedRetriever {
        ok: HashMap<(String, String), Vec<u8>>,
        err: HashMap<(String, String), RetrieveError>,
    }

    impl PinnedRetriever {
        /// Register verified bytes for a `(cid, path)`.
        fn put(&mut self, cid: &str, path: &str, bytes: &[u8]) {
            self.ok
                .insert((cid.to_string(), path.to_string()), bytes.to_vec());
        }

        /// Register a fail-closed failure for a `(cid, path)`.
        fn fail(&mut self, cid: &str, path: &str, err: RetrieveError) {
            self.err.insert((cid.to_string(), path.to_string()), err);
        }
    }

    impl ContentRetriever for PinnedRetriever {
        fn retrieve(&self, cid: &str, path: &str) -> Result<RetrievedContent, RetrieveError> {
            let key = (cid.to_string(), path.to_string());
            if let Some(err) = self.err.get(&key) {
                return Err(err.clone());
            }
            self.ok
                .get(&key)
                .map(|bytes| RetrievedContent {
                    bytes: bytes.clone(),
                    codec: 0x70,
                })
                .ok_or_else(|| RetrieveError::PathNotFound {
                    path: path.to_string(),
                })
        }
    }

    #[test]
    fn parses_an_ipfs_uri_into_cid_and_path() {
        let r = parse_ipfs_uri("ipfs://bafyfixturecid/index.html").expect("a valid ipfs uri");
        assert_eq!(r.cid, "bafyfixturecid");
        assert_eq!(r.path, "/index.html");
    }

    #[test]
    fn parses_a_bare_cid_uri_with_an_empty_path() {
        // `ipfs://<cid>` (no trailing slash) is the CID with no path.
        let r = parse_ipfs_uri("ipfs://bafyfixturecid").expect("a bare cid uri");
        assert_eq!(r.cid, "bafyfixturecid");
        assert_eq!(r.path, "");
        // A trailing slash is a root path, still no sub-resource.
        let root = parse_ipfs_uri("ipfs://bafyfixturecid/").expect("a root uri");
        assert_eq!(root.cid, "bafyfixturecid");
        assert_eq!(root.path, "/");
    }

    #[test]
    fn parses_a_deep_sub_resource_path() {
        // A real site's relative asset: the whole tail after the cid is the path
        // the retriever resolves into the DAG.
        let r = parse_ipfs_uri("ipfs://bafydir/assets/app.css").expect("a deep path");
        assert_eq!(r.cid, "bafydir");
        assert_eq!(r.path, "/assets/app.css");
    }

    #[test]
    fn normalize_ens_page_key_collapses_the_webkit_authority_variance() {
        // The regression's core: the authority form we store and the
        // authority-less (triple-slash) form WebKit reports for the SAME entry
        // must reduce to ONE key, so the forward-store key and the post-back key
        // match.
        let stored = normalize_ens_page_key("ipfs://bafycid");
        let webkit = normalize_ens_page_key("ipfs:///bafycid");
        assert_eq!(stored, webkit, "ipfs:// and ipfs:/// collapse to one key");
        assert_eq!(stored, "bafycid");
    }

    #[test]
    fn normalize_ens_page_key_ignores_a_bare_trailing_slash() {
        // A bare root `/` is the same entry as no path.
        assert_eq!(
            normalize_ens_page_key("ipfs://bafycid"),
            normalize_ens_page_key("ipfs://bafycid/")
        );
        assert_eq!(
            normalize_ens_page_key("ipfs:///bafycid/"),
            "bafycid",
            "authority-less + trailing slash still reduces to the bare cid"
        );
    }

    #[test]
    fn normalize_ens_page_key_keeps_a_real_sub_resource_path() {
        // A genuine deep path is part of the entry's identity and is preserved
        // (only a BARE trailing slash is trimmed), and both URL forms still agree.
        assert_eq!(
            normalize_ens_page_key("ipfs://bafydir/assets/app.css"),
            "bafydir/assets/app.css"
        );
        assert_eq!(
            normalize_ens_page_key("ipfs://bafydir/sub/"),
            normalize_ens_page_key("ipfs:///bafydir/sub")
        );
    }

    #[test]
    fn normalize_ens_page_key_leaves_a_non_ipfs_url_unchanged() {
        // A plain served page has no CID identity to canonicalize: it keys on its
        // exact URL, so the ENS association never touches it.
        assert_eq!(
            normalize_ens_page_key("https://example.com/"),
            "https://example.com/"
        );
        assert_eq!(normalize_ens_page_key("about:blank"), "about:blank");
    }

    #[test]
    fn ipfs_root_cid_and_path_splits_the_root_cid_from_the_in_site_path() {
        // The root-CID-PREFIX association fuel: split ANY `<rootcid>/<path>` (in
        // either URL form) into its root CID + in-site path, so a sub-path return
        // matches the SAME site's stored root CID and re-derives the name.
        assert_eq!(
            ipfs_root_cid_and_path("ipfs://bafyroot"),
            Some(("bafyroot".to_string(), String::new())),
            "the bare root splits to the cid + an empty in-site path"
        );
        assert_eq!(
            ipfs_root_cid_and_path("ipfs://bafyroot/"),
            Some(("bafyroot".to_string(), String::new())),
            "a bare trailing slash is still the root"
        );
        assert_eq!(
            ipfs_root_cid_and_path("ipfs://bafyroot/blog/post-1"),
            Some(("bafyroot".to_string(), "/blog/post-1".to_string()))
        );
        // BOTH URL forms (authority + WebKit authority-less) split to the SAME
        // root CID, so a stored root CID matches a post-back sub-path URL.
        assert_eq!(
            ipfs_root_cid_and_path("ipfs:///bafyroot/blog"),
            Some(("bafyroot".to_string(), "/blog".to_string()))
        );
        // A plain served page has no CID identity, so it never matches a site.
        assert_eq!(ipfs_root_cid_and_path("https://example.com/blog"), None);
    }

    #[test]
    fn rejects_a_non_ipfs_or_cid_less_uri() {
        assert_eq!(
            parse_ipfs_uri("https://example.com/"),
            Err(RendererError::InvalidUrl("https://example.com/".into()))
        );
        assert_eq!(
            parse_ipfs_uri("ipfs:///no-cid"),
            Err(RendererError::InvalidUrl("ipfs:///no-cid".into()))
        );
    }

    #[test]
    fn infers_html_mime_for_the_root_and_html_paths_for_parity() {
        assert_eq!(mime_type_for_path(""), "text/html");
        assert_eq!(mime_type_for_path("/"), "text/html");
        assert_eq!(mime_type_for_path("/index.html"), "text/html");
        assert_eq!(mime_type_for_path("/style.css"), "text/css");
        assert_eq!(mime_type_for_path("/app.js"), "text/javascript");
    }

    #[test]
    fn resolves_a_directory_root_to_verified_index_html_at_parity() {
        // A directory root (`ipfs://<cid>/`) resolves to the verified index.html
        // bytes, rendered as an html document (served-page parity).
        let cid = "bafydirroot";
        let index = b"<!doctype html><title>site</title><h1>verified multi-block</h1>";
        let mut retriever = PinnedRetriever::default();
        retriever.put(cid, "/", index);

        let response = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/"),
            },
        )
        .expect("directory root resolves index.html");
        assert_eq!(response.body, index);
        assert_eq!(response.mime_type, "text/html");
    }

    #[test]
    fn resolves_a_relative_sub_resource_into_the_dag_with_its_mime() {
        // A relative sub-resource path resolves into the verified DAG and is
        // returned with the MIME inferred from its extension (css here).
        let cid = "bafydirroot";
        let css = b"body { color: rebeccapurple; }";
        let mut retriever = PinnedRetriever::default();
        retriever.put(cid, "/style.css", css);

        let response = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/style.css"),
            },
        )
        .expect("a sub-resource resolves into the dag");
        assert_eq!(response.body, css);
        assert_eq!(response.mime_type, "text/css");
    }

    #[test]
    fn a_bare_cid_url_resolves_and_renders_the_verified_page() {
        // Typing `ipfs://<cid>` (no path) resolves the root resource (a single
        // raw page here) and renders as html.
        let cid = "bafyraw";
        let page = b"<!doctype html><title>root</title><p>bare cid page</p>";
        let mut retriever = PinnedRetriever::default();
        retriever.put(cid, "", page);

        let response = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}"),
            },
        )
        .expect("a bare cid resolves and renders");
        assert_eq!(response.body, page);
        assert_eq!(response.mime_type, "text/html");
    }

    #[test]
    fn a_block_hash_mismatch_fails_the_load_and_never_renders_unverified_bytes() {
        // The load-bearing gate: a mis-hashing block in the DAG is a tamper
        // failure that FAILS the load (an Err the backend surfaces), never
        // returns bytes to render.
        let cid = "bafytamper";
        let mut retriever = PinnedRetriever::default();
        retriever.fail(
            cid,
            "/index.html",
            RetrieveError::BlockHashMismatch {
                cid: cid.to_string(),
            },
        );

        let result = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/index.html"),
            },
        );
        let err = result.expect_err("a hash mismatch must fail the load, not render");
        assert!(
            matches!(&err, RendererError::Backend(msg) if msg.contains("mismatch")),
            "the mismatch fails the load with a verify reason, got: {err:?}"
        );
    }

    #[test]
    fn an_incomplete_dag_fails_the_load() {
        // A missing linked block / incomplete CAR fails the load closed.
        let cid = "bafyincomplete";
        let mut retriever = PinnedRetriever::default();
        retriever.fail(
            cid,
            "/",
            RetrieveError::MissingBlock {
                cid: "bafymissingchild".into(),
            },
        );
        let err = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/"),
            },
        )
        .expect_err("an incomplete dag fails the load");
        assert!(matches!(err, RendererError::Backend(_)));
    }

    #[test]
    fn a_budget_overflow_fails_the_load() {
        // A runaway DAG that trips the retrieval budget fails the load closed.
        let cid = "bafyrunaway";
        let mut retriever = PinnedRetriever::default();
        retriever.fail(cid, "/", RetrieveError::BudgetExceeded("too big".into()));
        let err = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/"),
            },
        )
        .expect_err("a budget overflow fails the load");
        assert!(
            matches!(&err, RendererError::Backend(msg) if msg.contains("budget")),
            "the budget overflow fails the load with a legible reason, got: {err:?}"
        );
    }

    #[test]
    fn an_unverifiable_cid_fails_the_load_rather_than_rendering() {
        // A malformed CID cannot be verified, so it must fail the load.
        let cid = "not-a-valid-cid";
        let mut retriever = PinnedRetriever::default();
        retriever.fail(cid, "/x", RetrieveError::InvalidCid(cid.to_string()));
        let err = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/x"),
            },
        )
        .expect_err("an unverifiable cid fails the load");
        assert!(matches!(err, RendererError::Backend(_)));
    }

    #[test]
    fn an_unresolved_path_fails_the_load_not_a_silent_empty_render() {
        // A path with no such resource fails the load, never renders empty.
        let cid = "bafydir";
        let retriever = PinnedRetriever::default();
        let err = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/missing.js"),
            },
        )
        .expect_err("a missing sub-resource fails the load");
        assert!(matches!(err, RendererError::Backend(_)));
    }
}
