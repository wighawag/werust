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

use fetcher::{Cid, ContentAddressedFetcher, ContentSource, FetchError, Fetcher, VerifyError};
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
/// The `cid` is the content identifier the [`ContentAddressedFetcher`] verifies
/// against; the `path` is the remainder after the CID authority (empty for
/// `ipfs://<cid>` or `ipfs://<cid>/`). werust's content-addressed path currently
/// resolves a single verified block per CID (DAG/UnixFS traversal is out of
/// scope in the fetcher — see the task forward-pointer), so the path does not
/// select a sub-resource; it only informs the MIME type for served-page parity.
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
/// [`ContentAddressedFetcher`]'s job, which rejects a malformed CID as its own
/// verify failure so the trust boundary stays in one place.)
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

/// Map a content-addressed [`VerifyError`] onto the seam's [`RendererError`], so
/// EVERY verify failure fails the load instead of rendering.
///
/// This is the load-bearing gate: a [`HashMismatch`](VerifyError::HashMismatch)
/// (the content did not match its hash), an [`UnsupportedHash`](VerifyError::UnsupportedHash)
/// or [`InvalidCid`](VerifyError::InvalidCid) (unverifiable), or a
/// [`Source`](VerifyError::Source) failure (the bytes could not be obtained) ALL
/// become a [`RendererError::Backend`] the scheme handler returns, which the
/// backend surfaces as a failed load. None of them ever yields bytes to render:
/// rejecting-when-unsure is the whole trust stance (`docs/adr/0001`).
fn verify_error_to_renderer_error(err: VerifyError) -> RendererError {
    RendererError::Backend(format!("ipfs:// content-addressed load failed: {err}"))
}

/// Resolve an intercepted `ipfs://` [`SchemeRequest`] through the hash-verified
/// content-addressed [`Fetcher`](fetcher::ContentAddressedFetcher) path, returning
/// the verified bytes as a [`SchemeResponse`] to render — or a [`RendererError`]
/// that FAILS the load.
///
/// This is the pure heart of the scheme -> verified-fetch -> render path, split
/// out so it is testable WITHOUT a webview: a live backend registers an `ipfs`
/// scheme handler that calls this with each intercepted request and a
/// [`ContentAddressedFetcher`](fetcher::ContentAddressedFetcher) backed by a
/// production source (an IPFS gateway over the HTTP [`Fetcher`](fetcher::Fetcher)).
///
/// The CID is resolved through
/// [`fetch_verified`](fetcher::ContentAddressedFetcher::fetch_verified), which
/// returns bytes ONLY after they verify against the CID's hash. So a hash
/// mismatch (or any other verify failure) surfaces here as a [`RendererError`]
/// and NOTHING is rendered — verification gates the load. On success the verified
/// bytes are handed back with a MIME type inferred from the path for served-page
/// parity.
pub fn resolve_ipfs_request(
    fetcher: &dyn ContentAddressedFetcher,
    request: &SchemeRequest,
) -> Result<SchemeResponse, RendererError> {
    let reference = parse_ipfs_uri(&request.uri)?;
    // Route THROUGH the verifying fetcher: bytes come back only after they hash
    // to the CID. A mismatch is a hard failure that fails the load, never a
    // silent render of unverified bytes.
    let body = fetcher
        .fetch_verified(&reference.cid)
        .map_err(verify_error_to_renderer_error)?;
    Ok(SchemeResponse {
        mime_type: mime_type_for_path(&reference.path).to_string(),
        body,
    })
}

/// The default IPFS HTTP gateway the production content source fetches candidate
/// bytes from.
///
/// A gateway is an UNTRUSTED origin: whatever it returns is hash-verified against
/// the CID before it is ever rendered (see [`GatewayContentSource`] and
/// [`resolve_ipfs_request`]), so a hostile or buggy gateway cannot cause
/// unverified bytes to render. `dweb.link` is a public gateway; the durable
/// gateway/peer policy (which gateway, or a local node) is not this task's
/// concern and can be swapped by constructing the source with a different base.
pub const DEFAULT_IPFS_GATEWAY: &str = "https://dweb.link";

/// A [`ContentSource`] that fetches candidate bytes for a CID from an IPFS HTTP
/// gateway over the bound HTTP [`Fetcher`](fetcher::Fetcher).
///
/// This is the PRODUCTION origin behind the `ipfs://` scheme: it GETs
/// `<gateway>/ipfs/<cid>` and hands back the raw bytes. Those bytes are UNTRUSTED
/// — [`VerifyingContentFetcher`](fetcher::VerifyingContentFetcher) hash-verifies
/// them against the CID before [`resolve_ipfs_request`] ever renders them, so the
/// gateway is never trusted, only the hash. Construct with
/// [`new`](GatewayContentSource::new) (the [`DEFAULT_IPFS_GATEWAY`]) or
/// [`with_gateway`](GatewayContentSource::with_gateway).
///
/// It is generic over the [`Fetcher`](fetcher::Fetcher) so tests can drive it
/// against a controlled local endpoint, off the live network, exactly as the
/// fetcher seam's own tests do.
pub struct GatewayContentSource<F: Fetcher> {
    fetcher: F,
    gateway: String,
}

impl<F: Fetcher> GatewayContentSource<F> {
    /// A gateway source over the given HTTP [`Fetcher`](fetcher::Fetcher), using
    /// the [`DEFAULT_IPFS_GATEWAY`].
    pub fn new(fetcher: F) -> Self {
        Self::with_gateway(fetcher, DEFAULT_IPFS_GATEWAY)
    }

    /// A gateway source pointed at a specific gateway base URL (e.g. a local
    /// node, or a test endpoint). A trailing `/` on `gateway` is tolerated.
    pub fn with_gateway(fetcher: F, gateway: &str) -> Self {
        Self {
            fetcher,
            gateway: gateway.trim_end_matches('/').to_string(),
        }
    }
}

impl<F: Fetcher> ContentSource for GatewayContentSource<F> {
    fn get(&self, cid: &Cid) -> Result<Vec<u8>, FetchError> {
        // Ask the gateway for the raw block bytes by CID. The bytes are candidate
        // (untrusted) content: the verify happens above this source.
        let url = format!("{gateway}/ipfs/{cid}", gateway = self.gateway);
        let response = self.fetcher.fetch(&url)?;
        if !response.is_success() {
            return Err(FetchError::Transport(format!(
                "gateway returned status {status} for {cid}",
                status = response.status,
            )));
        }
        Ok(response.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fetcher::{cid_v1_raw_sha256, VerifyingContentFetcher};

    /// A pinned, in-memory content source, isolated from the live network.
    ///
    /// It plays the role of the untrusted origin (an IPFS gateway / blockstore /
    /// peer): it hands back whatever bytes it holds for a CID's canonical string.
    /// The verify happens ABOVE it in [`VerifyingContentFetcher`], so it can be
    /// pointed at honest content (stored under its real CID) or TAMPERED content
    /// (bytes that do not match the CID) to exercise both the render-verified and
    /// mismatch-fails-the-load cases — with NO network access.
    #[derive(Default)]
    struct PinnedContentSource {
        blobs: std::collections::HashMap<String, Vec<u8>>,
    }

    impl PinnedContentSource {
        /// Store honest content under its real CID and return that CID: fetching
        /// this CID must verify and return these exact bytes.
        fn put(&mut self, bytes: &[u8]) -> String {
            let cid = cid_v1_raw_sha256(bytes).expect("derive cid for fixture content");
            self.blobs.insert(cid.clone(), bytes.to_vec());
            cid
        }

        /// Store TAMPERED bytes under `cid`: the bytes do NOT hash to `cid`, so a
        /// fetch of `cid` must fail the load with a hash mismatch, never render.
        fn put_tampered_under(&mut self, cid: &str, tampered: &[u8]) {
            self.blobs.insert(cid.to_string(), tampered.to_vec());
        }
    }

    impl ContentSource for PinnedContentSource {
        fn get(&self, cid: &Cid) -> Result<Vec<u8>, FetchError> {
            self.blobs
                .get(&cid.to_string())
                .cloned()
                .ok_or_else(|| FetchError::Transport("pinned source miss".into()))
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
    fn rejects_a_non_ipfs_or_cid_less_uri() {
        // A non-ipfs scheme, or an ipfs uri with no CID authority, cannot name
        // content to verify, so it is rejected up front (it fails the load).
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
        // A content-addressed page renders like a served page: the root and
        // .html paths are text/html so `ipfs://<cid>` opens as a document.
        assert_eq!(mime_type_for_path(""), "text/html");
        assert_eq!(mime_type_for_path("/"), "text/html");
        assert_eq!(mime_type_for_path("/index.html"), "text/html");
        assert_eq!(mime_type_for_path("/style.css"), "text/css");
        assert_eq!(mime_type_for_path("/app.js"), "text/javascript");
    }

    #[test]
    fn resolves_and_renders_verified_content_at_parity_with_a_served_page() {
        // Acceptance: an `ipfs://<cid>...` request is served through the resolver,
        // resolved by the hash-verified Fetcher path, and its VERIFIED bytes are
        // returned to render — with a text/html MIME type so it renders like the
        // same page served over http(s). Pinned fixture CID, no live network.
        let mut source = PinnedContentSource::default();
        let page = b"<!doctype html><title>ipfs</title><h1>verifiable, content-addressed</h1>";
        let cid = source.put(page);

        let fetcher = VerifyingContentFetcher::new(source);
        let request = SchemeRequest {
            uri: format!("ipfs://{cid}/index.html"),
        };
        let response =
            resolve_ipfs_request(&fetcher, &request).expect("verified content resolves to render");

        // The RENDERED bytes are exactly the verified content, at parity with a
        // served page (same bytes, an html document).
        assert_eq!(response.body, page);
        assert_eq!(response.mime_type, "text/html");
    }

    #[test]
    fn a_bare_cid_url_resolves_and_renders_the_verified_page() {
        // Typing `ipfs://<cid>` (no path) navigates and renders the verified
        // content-addressed page: the root resolves the CID and renders as html.
        let mut source = PinnedContentSource::default();
        let page = b"<!doctype html><title>root</title><p>bare cid page</p>";
        let cid = source.put(page);

        let fetcher = VerifyingContentFetcher::new(source);
        let response = resolve_ipfs_request(
            &fetcher,
            &SchemeRequest {
                uri: format!("ipfs://{cid}"),
            },
        )
        .expect("a bare cid resolves and renders");
        assert_eq!(response.body, page);
        assert_eq!(response.mime_type, "text/html");
    }

    #[test]
    fn a_hash_mismatch_fails_the_load_and_never_renders_unverified_bytes() {
        // The load-bearing gate: the source holds TAMPERED bytes under a real
        // CID. Resolving that `ipfs://<cid>` must FAIL the load (an Err the
        // backend surfaces as a failed load) and must NEVER return the tampered
        // bytes to render. Verification gates the load.
        let mut source = PinnedContentSource::default();
        let honest = b"the page this cid actually names";
        let cid = cid_v1_raw_sha256(honest).expect("derive fixture cid");
        source.put_tampered_under(&cid, b"tampered bytes that do not match the cid");

        let fetcher = VerifyingContentFetcher::new(source);
        let result = resolve_ipfs_request(
            &fetcher,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/index.html"),
            },
        );

        let err = result.expect_err("a hash mismatch must fail the load, not render");
        // It is a backend/load failure carrying the verify reason, NOT a response.
        assert!(
            matches!(&err, RendererError::Backend(msg) if msg.contains("mismatch")),
            "the mismatch fails the load with a verify reason, got: {err:?}"
        );
    }

    #[test]
    fn an_unverifiable_cid_fails_the_load_rather_than_rendering() {
        // A malformed CID cannot be verified, so it must fail the load (never a
        // silent render). The verify boundary refuses it inside fetch_verified;
        // the resolver surfaces that as a failed load.
        let source = PinnedContentSource::default();
        let fetcher = VerifyingContentFetcher::new(source);
        let err = resolve_ipfs_request(
            &fetcher,
            &SchemeRequest {
                uri: "ipfs://not-a-valid-cid/x".into(),
            },
        )
        .expect_err("an unverifiable cid fails the load");
        assert!(matches!(err, RendererError::Backend(_)));
    }

    /// A throwaway loopback HTTP endpoint that answers `/ipfs/<cid>` with fixed
    /// bytes — a controlled stand-in for an IPFS gateway, isolated from the live
    /// network (binds `127.0.0.1:0`). It serves one fixed body to every request
    /// and is torn down on [`Drop`]. Mirrors the fetcher seam's own
    /// `LocalHttpServer` test harness.
    struct LocalGateway {
        addr: std::net::SocketAddr,
        shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl LocalGateway {
        fn start(body: &[u8]) -> Self {
            use std::io::{Read, Write};
            use std::net::TcpListener;
            use std::sync::atomic::{AtomicBool, Ordering};
            use std::sync::Arc;

            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            listener
                .set_nonblocking(true)
                .expect("non-blocking listener");
            let addr = listener.local_addr().expect("local addr");
            let body = body.to_vec();
            let shutdown = Arc::new(AtomicBool::new(false));
            let stop = shutdown.clone();
            let handle = std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let _ = stream.set_nonblocking(false);
                            let mut buf = [0u8; 1024];
                            let _ = stream.read(&mut buf);
                            let head = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
                                len = body.len(),
                            );
                            let _ = stream.write_all(head.as_bytes());
                            let _ = stream.write_all(&body);
                            let _ = stream.flush();
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            });
            Self {
                addr,
                shutdown,
                handle: Some(handle),
            }
        }

        fn base_url(&self) -> String {
            format!("http://{}", self.addr)
        }
    }

    impl Drop for LocalGateway {
        fn drop(&mut self) {
            self.shutdown
                .store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    #[test]
    fn end_to_end_gateway_source_resolves_and_renders_a_verified_page_off_the_network() {
        // The full production path, headless and off the live network: an
        // `ipfs://<cid>` request goes through the resolver -> the gateway
        // ContentSource (over the bound HTTP Fetcher, pointed at a LOCAL loopback
        // gateway) -> hash verification -> render. The pinned fixture CID is
        // derived from the served bytes so it verifies; the rendered bytes are
        // exactly the fixture, at parity with a served page.
        use fetcher::HttpFetcher;

        let page = b"<!doctype html><title>gateway</title><h1>content-addressed via gateway</h1>";
        let cid = cid_v1_raw_sha256(page).expect("derive pinned fixture cid");
        let gateway = LocalGateway::start(page);

        let source = GatewayContentSource::with_gateway(HttpFetcher::new(), &gateway.base_url());
        let fetcher = VerifyingContentFetcher::new(source);

        let response = resolve_ipfs_request(
            &fetcher,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/index.html"),
            },
        )
        .expect("verified content from the gateway resolves to render");
        assert_eq!(response.body, page);
        assert_eq!(response.mime_type, "text/html");
    }

    #[test]
    fn a_gateway_serving_tampered_bytes_fails_the_load_never_renders() {
        // The gateway is UNTRUSTED: if it serves bytes that do not hash to the
        // requested CID, the verify above the source rejects them and the load
        // FAILS — the tampered bytes are never rendered. Proves the gateway is
        // never trusted, only the hash.
        use fetcher::HttpFetcher;

        let honest = b"the real content-addressed page";
        let cid = cid_v1_raw_sha256(honest).expect("derive pinned fixture cid");
        // The gateway serves DIFFERENT bytes under the same CID request.
        let gateway = LocalGateway::start(b"tampered bytes a hostile gateway returned");

        let source = GatewayContentSource::with_gateway(HttpFetcher::new(), &gateway.base_url());
        let fetcher = VerifyingContentFetcher::new(source);

        let err = resolve_ipfs_request(
            &fetcher,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/index.html"),
            },
        )
        .expect_err("a gateway serving tampered bytes must fail the load");
        assert!(
            matches!(&err, RendererError::Backend(msg) if msg.contains("mismatch")),
            "the tampered gateway response fails the load with a verify reason, got: {err:?}"
        );
    }

    #[test]
    fn a_source_miss_fails_the_load_not_a_silent_empty_render() {
        // A well-formed CID the source does not hold must fail the load, never
        // render empty/unverified content.
        let source = PinnedContentSource::default();
        // A real CID (derived, not stored) so parsing succeeds but the get misses.
        let cid = cid_v1_raw_sha256(b"never stored").expect("derive cid");
        let fetcher = VerifyingContentFetcher::new(source);
        let err = resolve_ipfs_request(
            &fetcher,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/"),
            },
        )
        .expect_err("a missing blob fails the load");
        assert!(matches!(err, RendererError::Backend(_)));
    }
}
