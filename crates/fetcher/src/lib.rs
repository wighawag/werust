//! The `Fetcher` seam: the networking interface.
//!
//! Networking lives behind the [`Fetcher`] trait so the rest of werust fetches
//! bytes for a URL ONLY through the seam — no HTTP-client type leaks past it.
//! This is the ordinary server-web fetch path: given an `http(s)://` URL it
//! returns the response bytes plus a little metadata (status, headers, the final
//! URL after redirects). The hash-verified content-addressed (`ipfs://`) path is
//! a sibling that layers verification ON TOP of this seam (task
//! `fetcher-hash-verified-content-addressed-path`); it is not implemented here.
//!
//! # TLS is bound, never hand-written
//!
//! The dangerous part — TLS — is delegated to a vetted implementation, never
//! written in-house (`CONTEXT.md`, `docs/adr/0001`). [`HttpFetcher`] binds
//! [`ureq`], a small synchronous HTTP client whose TLS backend is **rustls** (a
//! vetted, memory-safe TLS stack). rustls was chosen over a bound libcurl
//! because the project thesis is to stand on the mature *pure-Rust* stack: it
//! needs no C toolchain at the TLS edge, keeps the build Rust-only, and its
//! synchronous surface matches this seam (and the rest of werust's seams) with
//! no async runtime dragged in. The rationale + the rejected libcurl
//! alternative are recorded in `docs/spikes/fetcher-seam-bound-http-tls-stack/`.
//!
//! # Trust store: a safe default, policy deferred
//!
//! [`HttpFetcher`] uses ureq's default rustls configuration, which trusts the
//! bundled webpki root CAs — a working, SAFE default (real certificate + host
//! verification against public roots). The durable TLS trust-store / pinning
//! POLICY (custom roots, certificate pinning, whether content-addressed fetches
//! relax origin trust because verification moves to the hash) is an OPEN
//! QUESTION carried on the exploration spec
//! `rust-successor-native-renderer-architecture-benchmark` and is deliberately
//! NOT finalized here. This seam only binds a working, safe default fetch.
//!
//! # Failures are seam errors, never panics
//!
//! Every failure — a malformed URL, a TLS handshake/verification failure, a
//! connection or I/O error, a redirect loop — surfaces as a [`FetchError`]; the
//! seam never panics on a bad server or a bad certificate. A non-2xx HTTP status
//! is NOT an error at this layer: the response (status + body) is returned so the
//! caller decides what a `404`/`500` means.

use std::fmt;
use std::time::Duration;

use ureq::ResponseExt;

/// The connect timeout the default fetcher applies.
///
/// A safe default so a fetch to an unreachable or silently-dropping host fails
/// promptly as a [`FetchError`] instead of hanging on the OS connect retry
/// budget. It bounds only the TCP connect; the (larger) global read budget is
/// [`DEFAULT_GLOBAL_TIMEOUT`].
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// The whole-request timeout the default fetcher applies (connect + TLS + read),
/// a safe default upper bound so a fetch cannot hang indefinitely.
const DEFAULT_GLOBAL_TIMEOUT: Duration = Duration::from_secs(30);

/// A response fetched through the [`Fetcher`] seam.
///
/// Carries the bytes plus the little metadata a caller needs to interpret them.
/// A non-success HTTP status is reported here (in [`status`](Response::status)),
/// not as a [`FetchError`]: fetching a URL that answers `404` still SUCCEEDED as
/// a fetch, and the caller decides what the status means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The HTTP status code (e.g. `200`, `404`, `500`).
    pub status: u16,
    /// The value of the `Content-Type` header, if the server sent one.
    pub content_type: Option<String>,
    /// The response body bytes.
    pub body: Vec<u8>,
    /// The effective URL the body came from, after any redirects were followed.
    pub final_url: String,
}

impl Response {
    /// Whether the status is a 2xx success code.
    #[must_use]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// An error from a [`Fetcher`] operation.
///
/// Every way a fetch can fail lands in one of these variants, so a caller
/// pattern-matches instead of catching a panic. TLS handshake / certificate
/// verification failures are called out as their own [`Tls`](FetchError::Tls)
/// variant so the trust boundary is legible at the seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// The URL was malformed or used a scheme this seam does not fetch (only
    /// `http` and `https` are the server-web path).
    InvalidUrl(String),
    /// The TLS handshake or certificate verification failed. The dangerous part
    /// is delegated to the bound TLS stack; when it refuses a peer, that refusal
    /// arrives here rather than as a panic.
    Tls(String),
    /// The connection failed or a transport/protocol error occurred (DNS, connect
    /// refused, a malformed HTTP response, a redirect loop, …).
    Transport(String),
    /// An I/O error occurred while reading the response body.
    Io(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::InvalidUrl(u) => write!(f, "invalid or unsupported url: {u}"),
            FetchError::Tls(m) => write!(f, "tls error: {m}"),
            FetchError::Transport(m) => write!(f, "transport error: {m}"),
            FetchError::Io(m) => write!(f, "io error: {m}"),
        }
    }
}

impl std::error::Error for FetchError {}

/// The networking interface: fetch bytes for a URL over HTTP(S).
///
/// The rest of werust fetches ONLY through this trait, so the concrete HTTP+TLS
/// stack behind it ([`HttpFetcher`], or a test double) never leaks past the seam.
/// Implementations MUST surface every failure as a [`FetchError`] (see the module
/// docs) rather than panicking.
pub trait Fetcher {
    /// Fetch the resource at `url`, returning its bytes and metadata.
    ///
    /// `url` must be an absolute `http(s)://` URL; anything else is rejected with
    /// [`FetchError::InvalidUrl`] without a network round-trip. A reachable server
    /// that answers with a non-2xx status still yields `Ok` — the status is
    /// reported on the [`Response`] — because that is a successful fetch. A TLS
    /// failure surfaces as [`FetchError::Tls`]; a connection/transport failure as
    /// [`FetchError::Transport`]; a body-read failure as [`FetchError::Io`].
    fn fetch(&self, url: &str) -> Result<Response, FetchError>;
}

/// A [`Fetcher`] over a bound HTTP+TLS stack ([`ureq`] with a rustls TLS
/// backend).
///
/// This is the real server-web fetch path. Construct with [`HttpFetcher::new`]
/// and share it (it is cheap to [`Clone`] and safe to use from multiple threads;
/// the underlying `ureq::Agent` pools connections internally). TLS is handled
/// entirely by the bound rustls stack against the default public root store — a
/// working, safe default; the durable pinning/trust-store policy is deferred (see
/// the module docs).
#[derive(Clone)]
pub struct HttpFetcher {
    agent: ureq::Agent,
}

impl HttpFetcher {
    /// Create a fetcher over the bound HTTP+TLS stack.
    ///
    /// The agent is configured so a non-2xx HTTP status is returned as a
    /// [`Response`] rather than raised as an error (the caller decides what a
    /// `404`/`500` means); TLS uses the bound rustls stack's safe default trust
    /// store; and connect / whole-request timeouts are bounded
    /// ([`DEFAULT_CONNECT_TIMEOUT`], [`DEFAULT_GLOBAL_TIMEOUT`]) so an
    /// unreachable host fails promptly as a seam error instead of hanging.
    #[must_use]
    pub fn new() -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            // A non-2xx status is data for the caller, not a fetch failure.
            .http_status_as_error(false)
            // Bounded timeouts: an unreachable/silent host surfaces as a
            // FetchError instead of blocking on the OS connect budget.
            .timeout_connect(Some(DEFAULT_CONNECT_TIMEOUT))
            .timeout_global(Some(DEFAULT_GLOBAL_TIMEOUT))
            .build()
            .into();
        Self { agent }
    }
}

impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Reject anything that is not an absolute `http(s)://` URL before touching the
/// network. Other schemes (e.g. `ipfs://`) are not this seam's job — they are the
/// content-addressed sibling path.
fn validate_http_url(url: &str) -> Result<(), FetchError> {
    match url.split_once("://") {
        Some((scheme, rest))
            if (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
                && !rest.is_empty() =>
        {
            Ok(())
        }
        _ => Err(FetchError::InvalidUrl(url.to_string())),
    }
}

/// Map a [`ureq::Error`] onto the seam's [`FetchError`], keeping the TLS trust
/// boundary as its own variant.
fn map_ureq_error(err: ureq::Error) -> FetchError {
    match err {
        ureq::Error::Tls(msg) => FetchError::Tls(msg.to_string()),
        ureq::Error::Io(e) => FetchError::Io(e.to_string()),
        other => FetchError::Transport(other.to_string()),
    }
}

impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str) -> Result<Response, FetchError> {
        validate_http_url(url)?;

        let mut response = self.agent.get(url).call().map_err(map_ureq_error)?;

        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        // The effective URL after redirects, falling back to the request URL if
        // the stack does not surface one.
        let final_url = response.get_uri().to_string();
        let body = response.body_mut().read_to_vec().map_err(map_ureq_error)?;

        Ok(Response {
            status,
            content_type,
            body,
            final_url,
        })
    }
}

/// Returns the seam's crate name. A trivial anchor kept for callers that probe
/// the workspace; the real surface is the [`Fetcher`] trait.
#[must_use]
pub fn seam_name() -> &'static str {
    "fetcher"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    /// A throwaway, controlled local HTTP endpoint for the seam-contract tests.
    ///
    /// It binds `127.0.0.1:0` (a free ephemeral port on loopback — NO real
    /// network, no live-internet dependency) and serves a fixed response to each
    /// connection from a background thread, then the test tears it down. It
    /// speaks just enough HTTP/1.1 to exercise the seam; it is NOT a general
    /// server. Serving over loopback plaintext also lets a separate test point an
    /// `https://` fetch at it and observe the TLS handshake FAIL as a seam error.
    ///
    /// The accept loop is non-blocking and polls a shutdown flag, so [`Drop`]
    /// stops it WITHOUT connecting to itself. (This sandbox's loopback does not
    /// RST a connect to a closed port, so a connect-to-wake teardown would stall;
    /// the flag avoids touching the socket at all.)
    struct LocalHttpServer {
        addr: std::net::SocketAddr,
        shutdown: Arc<AtomicBool>,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl LocalHttpServer {
        /// Start a server that answers every accepted request with `status` /
        /// `content_type` / `body`. Returns once it is listening, so a fetch can
        /// connect immediately.
        fn start(status: u16, content_type: &str, body: &[u8]) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            listener
                .set_nonblocking(true)
                .expect("non-blocking listener");
            let addr = listener.local_addr().expect("local addr");
            let content_type = content_type.to_string();
            let body = body.to_vec();
            let shutdown = Arc::new(AtomicBool::new(false));
            let stop = shutdown.clone();
            let handle = thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => Self::serve_one(stream, status, &content_type, &body),
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
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

        /// Read the request (enough to not RST the client) and write one fixed
        /// HTTP/1.1 response with a `Content-Length`, then close.
        fn serve_one(mut stream: TcpStream, status: u16, content_type: &str, body: &[u8]) {
            // The listener is non-blocking, so the accepted stream inherits that;
            // put it back to blocking for the simple read/write exchange.
            let _ = stream.set_nonblocking(false);
            let mut buf = [0u8; 1024];
            // Read the request line + headers; we don't need the contents, only to
            // drain them so the peer's write side is satisfied.
            let _ = stream.read(&mut buf);
            let head = format!(
                "HTTP/1.1 {status} OK\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
                len = body.len(),
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body);
            let _ = stream.flush();
        }

        /// The `http://127.0.0.1:<port>/` base URL this server listens on.
        fn http_url(&self) -> String {
            format!("http://{}/", self.addr)
        }

        /// The `https://127.0.0.1:<port>/` URL — pointed at the SAME plaintext
        /// server so a TLS handshake against it must fail.
        fn https_url(&self) -> String {
            format!("https://{}/", self.addr)
        }
    }

    impl Drop for LocalHttpServer {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    #[test]
    fn seam_name_is_fetcher() {
        assert_eq!(seam_name(), "fetcher");
    }

    #[test]
    fn fetches_bytes_and_metadata_over_http_through_the_seam() {
        let server = LocalHttpServer::start(200, "text/plain", b"hello werust");
        let url = server.http_url();

        // Drive ONLY through the seam: a caller holds `dyn Fetcher`, never a
        // ureq/HTTP type.
        let fetcher: &dyn Fetcher = &HttpFetcher::new();
        let response = fetcher.fetch(&url).expect("fetch a local http endpoint");

        assert_eq!(response.status, 200);
        assert!(response.is_success());
        assert_eq!(response.body, b"hello werust");
        assert_eq!(response.content_type.as_deref(), Some("text/plain"));
        assert_eq!(response.final_url, url);
    }

    #[test]
    fn non_2xx_status_is_returned_not_raised_as_an_error() {
        // A reachable server answering 404 is a SUCCESSFUL fetch; the status is
        // reported on the response, so the caller (not the seam) decides meaning.
        let server = LocalHttpServer::start(404, "text/plain", b"nope");
        let fetcher = HttpFetcher::new();

        let response = fetcher
            .fetch(&server.http_url())
            .expect("a 404 still fetched");
        assert_eq!(response.status, 404);
        assert!(!response.is_success());
        assert_eq!(response.body, b"nope");
    }

    #[test]
    fn rejects_non_http_url_without_a_network_round_trip() {
        let fetcher = HttpFetcher::new();
        // A missing/empty scheme, or a scheme this seam does not fetch, is
        // rejected up front as a seam error.
        for bad in [
            "not-a-url",
            "https://",
            "ipfs://bafyexamplecid/x",
            "ftp://host/f",
        ] {
            let err = fetcher.fetch(bad).expect_err("non-http(s) url rejected");
            assert_eq!(err, FetchError::InvalidUrl(bad.to_string()));
        }
    }

    #[test]
    fn tls_handshake_failure_surfaces_as_a_seam_error_not_a_panic() {
        // Point an https:// fetch at a PLAINTEXT loopback server: the bound TLS
        // stack cannot complete a handshake with a non-TLS peer, so the failure
        // must arrive as a FetchError (Tls or Transport), never as a panic and
        // never as an Ok. This proves TLS/failures surface as seam errors.
        let server = LocalHttpServer::start(200, "text/plain", b"unreachable over tls");
        let fetcher = HttpFetcher::new();

        let result = fetcher.fetch(&server.https_url());
        let err = result.expect_err("tls handshake against a plaintext peer must fail");
        assert!(
            matches!(
                err,
                FetchError::Tls(_) | FetchError::Transport(_) | FetchError::Io(_)
            ),
            "expected a surfaced seam error, got: {err:?}"
        );
    }

    #[test]
    fn a_broken_response_surfaces_as_a_seam_error_not_a_panic() {
        // A reachable server that accepts the connection then closes WITHOUT
        // sending a valid HTTP response is a transport failure: ureq hits EOF
        // mid-response. It must surface as a FetchError, never a panic or an Ok.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener.set_nonblocking(true).expect("non-blocking");
        let addr = listener.local_addr().expect("local addr");
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let handle = thread::spawn(move || {
            while !stop_thread.load(Ordering::Relaxed) {
                match listener.accept() {
                    // Accept, then drop the stream immediately: the client sees a
                    // connection reset / premature EOF rather than a response.
                    Ok((stream, _)) => drop(stream),
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        let fetcher = HttpFetcher::new();
        let err = fetcher
            .fetch(&format!("http://{addr}/"))
            .expect_err("a closed-without-response connection must fail");
        assert!(
            matches!(err, FetchError::Transport(_) | FetchError::Io(_)),
            "expected a surfaced seam error, got: {err:?}"
        );

        stop.store(true, Ordering::Relaxed);
        let _ = handle.join();
    }
}
