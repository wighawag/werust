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

/// The multi-block content-retrieval seam ([`ContentRetriever`]) and its default
/// trustless-gateway CAR backend live in a sibling module; the key types are
/// re-exported below so callers use `fetcher::ContentRetriever` alongside
/// `fetcher::ContentAddressedFetcher`.
pub mod retriever;
pub use retriever::{
    ContentRetriever, RetrievalBudget, RetrieveError, RetrievedContent,
    TrustlessGatewayCarRetriever, DEFAULT_TRUSTLESS_GATEWAY,
};

/// The connect timeout the default fetcher applies.
///
/// A safe, DELIBERATELY TIGHT default so a fetch to an unreachable or
/// silently-dropping host fails promptly as a [`FetchError`] instead of hanging
/// on the OS connect retry budget. It bounds only the TCP connect; the (larger)
/// global read budget is [`DEFAULT_GLOBAL_TIMEOUT`]. Keeping connect tight is
/// what lets the global read budget be raised for a slow-but-progressing fetch
/// WITHOUT making a dead host hang: a dead/unreachable host still fails on this
/// connect bound (~10s), never the raised global bound.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// The whole-request wall-clock the default fetcher applies (connect + TLS +
/// read), a safe upper bound so a fetch cannot hang indefinitely.
///
/// This is the CONTENT-fetch budget: a cold trustless-gateway CAR fetch of a
/// real multi-block site (an `index.html` + assets, each a separate
/// `dag-scope=entity` request) can legitimately take far longer than a single
/// server-web GET, so the old 30s killed a merely-slow-but-progressing first
/// load (the `ronan.eth` field finding, v0.2.2). Raised to 120s so a cold,
/// slow, PROGRESSING load completes, while the DAG bytes/blocks budgets
/// ([`RetrievalBudget`]) remain the size ceilings and this stays a BOUNDED
/// wall-clock (a hostile/silent host still fails eventually). The IPNS record
/// fetch, a small single GET, uses the shorter [`DEFAULT_IPNS_RECORD_TIMEOUT`]
/// instead. Overridable per fetcher via [`HttpFetcher::with_timeouts`]. The
/// chosen budgets + rationale are recorded in
/// `docs/spikes/fetch-timeout-raise-and-split-for-ipns-and-content/DECISIONS.md`.
pub const DEFAULT_GLOBAL_TIMEOUT: Duration = Duration::from_secs(120);

/// The whole-request wall-clock for an IPNS RECORD fetch (connect + TLS + read).
///
/// An IPNS load does an EXTRA round-trip before any content: fetch + verify the
/// signed IPNS record, THEN fetch the content. The record itself is a small,
/// single signed blob (`GET /ipns/{name}?format=ipns-record`), so it does not
/// need the full content budget; but a cold gateway resolving a name (a DHT /
/// routing lookup behind the gateway) can still be slow, so this sits ABOVE the
/// tight connect bound and BELOW the content budget. Split out from
/// [`DEFAULT_GLOBAL_TIMEOUT`] so the record step and the content step each get a
/// budget appropriate to their size, and neither spuriously times out the
/// other. Overridable per fetcher via [`HttpFetcher::with_timeouts`]; the IPNS
/// record source wires an [`HttpFetcher`] built with it.
pub const DEFAULT_IPNS_RECORD_TIMEOUT: Duration = Duration::from_secs(45);

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
    /// Create a fetcher over the bound HTTP+TLS stack with the default timeouts.
    ///
    /// The agent is configured so a non-2xx HTTP status is returned as a
    /// [`Response`] rather than raised as an error (the caller decides what a
    /// `404`/`500` means); TLS uses the bound rustls stack's safe default trust
    /// store; and connect / whole-request timeouts are bounded
    /// ([`DEFAULT_CONNECT_TIMEOUT`], [`DEFAULT_GLOBAL_TIMEOUT`]) so an
    /// unreachable host fails promptly as a seam error instead of hanging. This
    /// is the CONTENT-fetch budget; the IPNS record path builds its own fetcher
    /// with the shorter [`DEFAULT_IPNS_RECORD_TIMEOUT`] via [`with_timeouts`].
    ///
    /// [`with_timeouts`]: HttpFetcher::with_timeouts
    #[must_use]
    pub fn new() -> Self {
        Self::with_timeouts(DEFAULT_CONNECT_TIMEOUT, DEFAULT_GLOBAL_TIMEOUT)
    }

    /// Create a fetcher with EXPLICIT connect + global (whole-request) timeouts,
    /// the override lever for a step whose realistic budget differs from the
    /// default content budget.
    ///
    /// Mirrors the crate's `DEFAULT_* const + with_*()` override pattern (as
    /// [`TrustlessGatewayCarRetriever::with_gateway`] /
    /// [`with_budget`](RetrievalBudget) do): the constants are the defaults and
    /// this is how a caller adjusts them WITHOUT a config subsystem. The IPNS
    /// record source uses it to fetch the small signed record on the shorter
    /// [`DEFAULT_IPNS_RECORD_TIMEOUT`] while the content path keeps the larger
    /// [`DEFAULT_GLOBAL_TIMEOUT`].
    ///
    /// BOTH timeouts must stay BOUNDED: `connect` should be the tight bound that
    /// fails a dead host fast, and `global` the (larger) wall-clock a
    /// slow-but-progressing fetch may take. `global` MUST be >= `connect` for
    /// the split to make sense (the whole request includes the connect), but an
    /// unbounded (absent) timeout is intentionally not offered here: a
    /// hostile/silent host must always fail eventually.
    #[must_use]
    pub fn with_timeouts(connect: Duration, global: Duration) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            // A non-2xx status is data for the caller, not a fetch failure.
            .http_status_as_error(false)
            // Bounded timeouts: the tight connect bound fails an
            // unreachable/silent host fast; the (larger) global bound is the
            // wall-clock a slow-but-progressing fetch may take.
            .timeout_connect(Some(connect))
            .timeout_global(Some(global))
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

// ---------------------------------------------------------------------------
// The hash-verified content-addressed fetch path (the `ipfs://` fetch half).
// ---------------------------------------------------------------------------
//
// This is the technical CORE of the thesis (`CONTEXT.md`, `docs/adr/0001`):
// content fetched by a content identifier (CID) is trusted because it VERIFIES
// against the hash the CID names, NOT because some origin served it. The origin
// (a gateway, a peer, a local store) is untrusted; verification moves to the
// hash. A mismatch is a HARD failure, never a silent pass.
//
// # Where the bytes come from is a seam ([`ContentSource`])
//
// This path splits cleanly in two: *getting* candidate bytes for a CID (from an
// IPFS gateway over the HTTP [`Fetcher`], a local blockstore, a peer, or (in
// tests) a temp-dir store) and *verifying* those bytes hash to the CID. Only
// the verification is this task's technical core, and it must be identical
// wherever the bytes came from. So the origin is abstracted behind the
// [`ContentSource`] trait and [`VerifyingContentFetcher`] layers verification on
// top of ANY source. The concrete gateway/network source is the consuming
// task's job (`ipfs-scheme-resolution-through-renderer-seam`); this seam owns
// the verify.
//
// # CID scope: any codec, sha2-256 multihash (the IPFS default)
//
// A CID names its content by a self-describing multihash. This path parses the
// full CID (via the vetted `cid`/`multihash` crates, since CID parsing is byte
// layout, not a cryptographic primitive) and RE-COMPUTES the digest over the
// fetched bytes with the hash function the multihash names, then compares. It
// supports the `sha2-256` multihash (multihash code `0x12`), the dominant IPFS
// default, for ANY CID version/codec whose block bytes are addressed directly
// (raw / the leaf block). A CID naming a different, not-yet-supported hash
// function is rejected as [`VerifyError::UnsupportedHash`]: an explicit refusal,
// NEVER a silent pass (rejecting-when-unsure is the whole trust stance). DAG/UnixFS
// traversal (a CID whose block is an IPLD node linking child blocks rather than
// the content itself) is out of scope here and belongs to the render/resolution
// tasks; this seam verifies the block bytes it is handed against the CID.

use sha2::{Digest, Sha256};

// The `ContentSource` trait's `get` takes a `&Cid`, so an out-of-crate
// implementor (e.g. the `ipfs://` scheme resolver's content source) needs the
// type. Re-export it from the seam rather than making callers depend on the
// `cid` crate directly and risk a version skew with the one this seam verifies
// against.
pub use cid::Cid;

/// The multihash code for `sha2-256` (the IPFS default content hash).
///
/// A CID whose multihash uses this code is verified by re-hashing the fetched
/// bytes with SHA-256 and comparing against the digest the CID carries. Any
/// other code is refused as [`VerifyError::UnsupportedHash`] rather than trusted.
const MULTIHASH_SHA2_256: u64 = 0x12;

/// Where candidate bytes for a CID come from, before verification.
///
/// The content-addressed path is deliberately split: a `ContentSource` PRODUCES
/// bytes for a CID (an IPFS gateway over the HTTP [`Fetcher`], a local
/// blockstore, a peer, or, in tests, a temp-dir store), and
/// [`VerifyingContentFetcher`] VERIFIES them against the CID's hash. A source is
/// UNTRUSTED: whatever it returns is verified before it is ever handed back, so
/// a hostile or buggy source cannot cause unverified bytes to be returned as if
/// valid. Implementations surface a miss / transport failure as a
/// [`FetchError`]; the verification (and its hard-failure on mismatch) is not
/// their concern.
pub trait ContentSource {
    /// Produce the candidate bytes a source has for `cid`.
    ///
    /// The returned bytes are NOT yet trusted: the caller verifies them against
    /// the CID. A source that does not have the content, or fails to retrieve
    /// it, returns a [`FetchError`] (e.g. [`FetchError::Transport`]).
    fn get(&self, cid: &Cid) -> Result<Vec<u8>, FetchError>;
}

/// A failure of the hash-verified content-addressed fetch path.
///
/// Kept distinct from [`FetchError`] because the failure modes are different:
/// the CID itself can be malformed ([`InvalidCid`](VerifyError::InvalidCid)),
/// name a hash function this path does not yet implement
/// ([`UnsupportedHash`](VerifyError::UnsupportedHash)), the source can fail to
/// produce bytes ([`Source`](VerifyError::Source)), or, the load-bearing one,
/// the produced bytes can FAIL to hash to the CID
/// ([`HashMismatch`](VerifyError::HashMismatch)). The mismatch is the whole
/// point: it means the content did not match its hash, so it is rejected, never
/// returned as if valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// The CID string could not be parsed as a content identifier.
    InvalidCid(String),
    /// The CID names a multihash function this path does not verify (only
    /// `sha2-256` is supported today). Refused rather than trusted: an
    /// unverifiable CID must not silently pass.
    UnsupportedHash {
        /// The multihash code the CID carried.
        code: u64,
    },
    /// The [`ContentSource`] failed to produce bytes for the CID.
    Source(FetchError),
    /// The produced bytes did NOT hash to the CID's digest: the content is
    /// tampered/incorrect and is rejected. This is the loud failure the whole
    /// path exists to guarantee.
    HashMismatch {
        /// The CID the caller asked for (its canonical string form).
        cid: String,
    },
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyError::InvalidCid(c) => write!(f, "invalid content identifier: {c}"),
            VerifyError::UnsupportedHash { code } => {
                write!(
                    f,
                    "unsupported content hash function (multihash code {code:#x})"
                )
            }
            VerifyError::Source(e) => write!(f, "content source error: {e}"),
            VerifyError::HashMismatch { cid } => {
                write!(f, "content hash mismatch: bytes do not match cid {cid}")
            }
        }
    }
}

impl std::error::Error for VerifyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VerifyError::Source(e) => Some(e),
            _ => None,
        }
    }
}

/// The content-addressed half of the [`Fetcher`] seam: fetch by CID, return the
/// bytes ONLY after they verify against the CID's hash.
///
/// The rest of werust obtains content-addressed bytes ONLY through this trait,
/// so the verify can never be skipped by a caller: there is no way to get the
/// bytes without going through the hash check. The `ipfs://` scheme wiring
/// (`ipfs-scheme-resolution-through-renderer-seam`) resolves a CID through this
/// path and renders the verified bytes.
pub trait ContentAddressedFetcher {
    /// Fetch the content named by `cid` and return its bytes only after they
    /// verify against the CID's hash.
    ///
    /// `cid` is a content identifier string (e.g. the `<cid>` in
    /// `ipfs://<cid>/…`). The bytes are obtained from the underlying source and
    /// their hash recomputed and compared to the digest the CID carries: on a
    /// match the bytes are returned; on a mismatch the result is
    /// [`VerifyError::HashMismatch`] and NOTHING is returned. A malformed CID is
    /// [`VerifyError::InvalidCid`]; a CID naming an unsupported hash is
    /// [`VerifyError::UnsupportedHash`]; a source failure is
    /// [`VerifyError::Source`]. Verification NEVER trusts the origin: it trusts
    /// the hash.
    fn fetch_verified(&self, cid: &str) -> Result<Vec<u8>, VerifyError>;
}

/// A [`ContentAddressedFetcher`] that layers hash verification over any
/// [`ContentSource`].
///
/// Construct with [`VerifyingContentFetcher::new`], handing it the source the
/// bytes come from (a temp-dir store in tests; an IPFS gateway over the HTTP
/// [`Fetcher`] in production, the consuming task's job). Every
/// [`fetch_verified`](ContentAddressedFetcher::fetch_verified) call parses the
/// CID, asks the source for bytes, and verifies them BEFORE returning: the
/// source is never trusted.
pub struct VerifyingContentFetcher<S: ContentSource> {
    source: S,
}

impl<S: ContentSource> VerifyingContentFetcher<S> {
    /// Wrap a [`ContentSource`] with hash verification.
    pub fn new(source: S) -> Self {
        Self { source }
    }

    /// The wrapped source (untrusted; every fetch through this fetcher verifies
    /// what it produces).
    pub fn source(&self) -> &S {
        &self.source
    }
}

/// Verify that `bytes` hash to the digest `cid` names, returning the exact
/// [`VerifyError`] on any failure.
///
/// This is the technical core in one place: recompute the digest with the hash
/// function the CID's multihash names, then compare it constant-length against
/// the digest the CID carries. An unsupported hash function is refused, never
/// assumed to match.
fn verify_bytes_against_cid(cid: &Cid, bytes: &[u8]) -> Result<(), VerifyError> {
    let mh = cid.hash();
    match mh.code() {
        MULTIHASH_SHA2_256 => {
            let computed = Sha256::digest(bytes);
            if computed.as_slice() == mh.digest() {
                Ok(())
            } else {
                Err(VerifyError::HashMismatch {
                    cid: cid.to_string(),
                })
            }
        }
        code => Err(VerifyError::UnsupportedHash { code }),
    }
}

impl<S: ContentSource> ContentAddressedFetcher for VerifyingContentFetcher<S> {
    fn fetch_verified(&self, cid: &str) -> Result<Vec<u8>, VerifyError> {
        let parsed = Cid::try_from(cid).map_err(|_| VerifyError::InvalidCid(cid.to_string()))?;
        let bytes = self.source.get(&parsed).map_err(VerifyError::Source)?;
        verify_bytes_against_cid(&parsed, &bytes)?;
        Ok(bytes)
    }
}

/// Compute the canonical CIDv1 (raw codec, `sha2-256` multihash) that addresses
/// `bytes`, as its base32 string.
///
/// This is the inverse of the verify: it names content by its hash. It is how a
/// content store (or a test) derives the CID a blob should be stored under, so
/// the round-trip "store bytes under their CID, fetch that CID, get the bytes
/// back verified" holds. Returns [`VerifyError::InvalidCid`] only if the derived
/// multihash could not be assembled (not expected for `sha2-256`).
pub fn cid_v1_raw_sha256(bytes: &[u8]) -> Result<String, VerifyError> {
    use cid::multihash::Multihash;
    /// The `raw` IPLD multicodec code (block bytes ARE the content).
    const RAW_CODEC: u64 = 0x55;
    let digest = Sha256::digest(bytes);
    let mh = Multihash::<64>::wrap(MULTIHASH_SHA2_256, digest.as_slice())
        .map_err(|e| VerifyError::InvalidCid(e.to_string()))?;
    Ok(Cid::new_v1(RAW_CODEC, mh).to_string())
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

    // -----------------------------------------------------------------------
    // Timeout budgets: the connect bound stays tight (a dead host fails fast);
    // the global read budget is raised and overridable; both stay BOUNDED
    // (`fetch-timeout-raise-and-split-for-ipns-and-content`). Network-isolated:
    // every case drives a loopback endpoint (or a routeable-but-dead port), no
    // live network.
    // -----------------------------------------------------------------------

    /// A throwaway loopback server that DELAYS `delay` before writing its
    /// response, to model a slow-but-PROGRESSING host. Isolated from the live
    /// network (binds `127.0.0.1:0`), torn down on [`Drop`].
    struct SlowHttpServer {
        addr: std::net::SocketAddr,
        shutdown: Arc<AtomicBool>,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl SlowHttpServer {
        fn start(delay: Duration, body: &[u8]) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            listener
                .set_nonblocking(true)
                .expect("non-blocking listener");
            let addr = listener.local_addr().expect("local addr");
            let body = body.to_vec();
            let shutdown = Arc::new(AtomicBool::new(false));
            let stop = shutdown.clone();
            let handle = thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let _ = stream.set_nonblocking(false);
                            let mut buf = [0u8; 1024];
                            let _ = stream.read(&mut buf);
                            // The delay models a slow-but-progressing server: the
                            // connect succeeded, the response is merely late.
                            thread::sleep(delay);
                            let head = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
                                len = body.len(),
                            );
                            let _ = stream.write_all(head.as_bytes());
                            let _ = stream.write_all(&body);
                            let _ = stream.flush();
                        }
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

        fn http_url(&self) -> String {
            format!("http://{}/", self.addr)
        }
    }

    impl Drop for SlowHttpServer {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    #[test]
    fn the_default_content_budget_is_raised_above_the_old_thirty_seconds() {
        // The `ronan.eth` regression guard: a cold content fetch must have a
        // realistic wall-clock, not the old 30s that killed a slow-but-
        // progressing first load. The content budget is now larger, the connect
        // bound stays tight, and the record budget sits between them. Asserting
        // the constants (not a 120s live sleep) keeps the test fast + isolated.
        assert!(
            DEFAULT_GLOBAL_TIMEOUT > Duration::from_secs(30),
            "the content budget must be raised above the old 30s that spuriously timed out"
        );
        // Connect stays tight so a dead host still fails fast.
        assert!(
            DEFAULT_CONNECT_TIMEOUT <= Duration::from_secs(10),
            "the connect bound must stay tight so a dead host fails fast"
        );
        assert!(
            DEFAULT_CONNECT_TIMEOUT < DEFAULT_GLOBAL_TIMEOUT,
            "connect must be tighter than the whole-request budget"
        );
        // The record budget is its OWN, appropriately-bounded value: above the
        // tight connect bound, at or below the content budget.
        assert!(
            DEFAULT_IPNS_RECORD_TIMEOUT > DEFAULT_CONNECT_TIMEOUT
                && DEFAULT_IPNS_RECORD_TIMEOUT <= DEFAULT_GLOBAL_TIMEOUT,
            "the ipns record budget must sit between the connect bound and the content budget"
        );
    }

    #[test]
    fn a_slow_but_within_budget_fetch_succeeds() {
        // A host that connects fine but answers SLOWLY (a cold gateway) must not
        // be killed as long as it progresses within the budget. Built with a
        // short explicit budget so the test is fast, exercising the SAME
        // `with_timeouts` lever the production split uses.
        let server = SlowHttpServer::start(Duration::from_millis(300), b"slow but progressing");
        // A budget comfortably above the server's 300ms delay.
        let fetcher = HttpFetcher::with_timeouts(DEFAULT_CONNECT_TIMEOUT, Duration::from_secs(5));

        let response = fetcher
            .fetch(&server.http_url())
            .expect("a slow-but-within-budget fetch succeeds");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"slow but progressing");
    }

    #[test]
    fn a_fetch_that_exceeds_the_global_budget_fails_bounded_not_hangs() {
        // The budget stays BOUNDED: a host that answers SLOWER than the global
        // wall-clock is abandoned as a seam error rather than hanging forever.
        // A tiny explicit budget vs a longer server delay proves the bound bites
        // (and keeps the test fast), via the same `with_timeouts` lever.
        let server = SlowHttpServer::start(Duration::from_secs(2), b"too slow");
        let fetcher =
            HttpFetcher::with_timeouts(DEFAULT_CONNECT_TIMEOUT, Duration::from_millis(200));

        let start = std::time::Instant::now();
        let err = fetcher
            .fetch(&server.http_url())
            .expect_err("a fetch past the global budget must fail, not hang");
        // It failed WELL before the server's 2s delay: the budget bounded it.
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "the global budget must bound the wall-clock, elapsed {:?}",
            start.elapsed()
        );
        assert!(
            matches!(err, FetchError::Transport(_) | FetchError::Io(_)),
            "expected a surfaced seam timeout error, got: {err:?}"
        );
    }

    #[test]
    fn a_dead_host_fails_fast_on_the_tight_connect_bound_not_the_raised_budget() {
        // The connect bound stays tight even though the global budget is raised:
        // an unreachable host must fail on connect (fast), NOT hang until the
        // large global budget. A routeable-but-non-listening address on the
        // TEST-NET-1 documentation block (RFC 5737, guaranteed not to route to a
        // live host) models a dead host with NO live-network dependency. A very
        // short connect bound vs a large global bound proves the split: it fails
        // on the connect bound, far under the global one.
        let fetcher =
            HttpFetcher::with_timeouts(Duration::from_millis(300), Duration::from_secs(120));

        let start = std::time::Instant::now();
        let err = fetcher
            .fetch("http://192.0.2.1:9/")
            .expect_err("a dead host must fail, not hang");
        // It failed near the tight CONNECT bound, nowhere near the 120s global.
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "a dead host must fail fast on the connect bound, elapsed {:?}",
            start.elapsed()
        );
        assert!(
            matches!(err, FetchError::Transport(_) | FetchError::Io(_)),
            "expected a surfaced connect/transport error, got: {err:?}"
        );
    }

    // -----------------------------------------------------------------------
    // The hash-verified content-addressed fetch path.
    // -----------------------------------------------------------------------

    /// A throwaway content store backed by a TEMP DIRECTORY, isolated from the
    /// live network and from every other test.
    ///
    /// It plays the role of an untrusted origin (a gateway / blockstore / peer):
    /// it just hands back whatever bytes it holds for a CID's canonical string.
    /// The verify happens ABOVE it in [`VerifyingContentFetcher`], so this store
    /// can be pointed at either honest content (stored under its real CID) or
    /// TAMPERED content (bytes that do not match the CID) to exercise both the
    /// matching and mismatching cases. The temp dir is removed on [`Drop`].
    struct TempDirContentStore {
        dir: tempfile::TempDir,
    }

    impl TempDirContentStore {
        fn new() -> Self {
            Self {
                dir: tempfile::tempdir().expect("create isolated temp content store"),
            }
        }

        /// Store honest content: derive its real CID, save the bytes under it,
        /// and return that CID. Fetching this CID must verify and return the
        /// bytes.
        fn put(&self, bytes: &[u8]) -> String {
            let cid = cid_v1_raw_sha256(bytes).expect("derive cid for content");
            std::fs::write(self.dir.path().join(&cid), bytes).expect("write content blob");
            cid
        }

        /// Store TAMPERED content under a CID: the file saved for `cid` holds
        /// `tampered`, which does NOT hash to `cid`. Fetching this CID must fail
        /// loudly with a hash mismatch, never return the bytes.
        fn put_tampered_under(&self, cid: &str, tampered: &[u8]) {
            std::fs::write(self.dir.path().join(cid), tampered).expect("write tampered blob");
        }
    }

    impl ContentSource for TempDirContentStore {
        fn get(&self, cid: &Cid) -> Result<Vec<u8>, FetchError> {
            let path = self.dir.path().join(cid.to_string());
            std::fs::read(&path)
                .map_err(|e| FetchError::Transport(format!("content store miss: {e}")))
        }
    }

    #[test]
    fn fetches_and_returns_content_that_verifies_against_its_cid() {
        // The matching case: content stored under its real CID fetches back,
        // verified, byte-for-byte. This drives ONLY through the seam trait: a
        // caller holds `dyn ContentAddressedFetcher`, never the store.
        let store = TempDirContentStore::new();
        let content = b"<html><body>verifiable, content-addressed</body></html>";
        let cid = store.put(content);

        let fetcher: &dyn ContentAddressedFetcher = &VerifyingContentFetcher::new(store);
        let got = fetcher
            .fetch_verified(&cid)
            .expect("content that matches its cid is returned");

        assert_eq!(got, content);
    }

    #[test]
    fn a_hash_mismatch_fails_loudly_and_never_returns_the_bytes() {
        // The mismatching case, the whole point of the path. The store is asked
        // for a real CID but has been made to hold TAMPERED bytes under it. The
        // fetch MUST reject: a HashMismatch, never `Ok`, never the tampered
        // bytes returned as if valid (we don't trust the origin, we verify the
        // content).
        let store = TempDirContentStore::new();
        let honest = b"the content this cid actually names";
        let cid = cid_v1_raw_sha256(honest).expect("derive cid");
        store.put_tampered_under(&cid, b"tampered bytes that do not match the cid");

        let fetcher = VerifyingContentFetcher::new(store);
        let result = fetcher.fetch_verified(&cid);

        assert_eq!(
            result,
            Err(VerifyError::HashMismatch { cid: cid.clone() }),
            "tampered content must be rejected as a loud mismatch, got: {result:?}"
        );
    }

    #[test]
    fn a_malformed_cid_is_rejected_before_touching_the_source() {
        // A CID that does not parse is refused up front as InvalidCid: the
        // source is never consulted (an unparseable identifier cannot name
        // anything to verify against).
        let store = TempDirContentStore::new();
        let fetcher = VerifyingContentFetcher::new(store);

        let err = fetcher
            .fetch_verified("not-a-valid-cid")
            .expect_err("a malformed cid is rejected");
        assert_eq!(err, VerifyError::InvalidCid("not-a-valid-cid".to_string()));
    }

    #[test]
    fn a_source_miss_surfaces_as_a_source_error_not_a_silent_empty_pass() {
        // A well-formed CID the store does not hold must surface as a Source
        // error, NOT as an empty success, and NOT verified against nothing.
        let store = TempDirContentStore::new();
        // A real CID (derived, not stored) so parsing succeeds but the get misses.
        let cid = cid_v1_raw_sha256(b"never stored").expect("derive cid");
        let fetcher = VerifyingContentFetcher::new(store);

        let err = fetcher
            .fetch_verified(&cid)
            .expect_err("a missing blob must fail, not silently pass");
        assert!(
            matches!(err, VerifyError::Source(FetchError::Transport(_))),
            "expected a surfaced source error, got: {err:?}"
        );
    }

    #[test]
    fn a_cid_naming_an_unsupported_hash_is_refused_not_trusted() {
        // A CID whose multihash is NOT sha2-256 cannot be verified by this path
        // yet. It must be REFUSED (UnsupportedHash), never assumed to match:
        // rejecting-when-unsure is the trust stance. Here we hand-build a CIDv1
        // with an identity multihash (code 0x00) over some bytes and store those
        // same bytes; even though the bytes "match" the identity digest, the
        // path refuses because it does not implement that hash function.
        use cid::multihash::Multihash;
        const IDENTITY_CODE: u64 = 0x00;
        const RAW_CODEC: u64 = 0x55;
        let bytes = b"content behind an unsupported hash";
        let mh = Multihash::<64>::wrap(IDENTITY_CODE, bytes).expect("identity multihash");
        let cid = Cid::new_v1(RAW_CODEC, mh).to_string();

        let store = TempDirContentStore::new();
        std::fs::write(store.dir.path().join(&cid), bytes).expect("store the blob");
        let fetcher = VerifyingContentFetcher::new(store);

        let err = fetcher
            .fetch_verified(&cid)
            .expect_err("an unsupported hash function must be refused");
        assert_eq!(
            err,
            VerifyError::UnsupportedHash {
                code: IDENTITY_CODE
            }
        );
    }

    #[test]
    fn derived_cid_round_trips_through_verification() {
        // The CID derivation used to store content is the exact inverse of the
        // verify: content stored under its derived CID verifies against that
        // same CID. Guards the store/verify pair from drifting apart.
        let bytes = b"round-trip me";
        let cid = cid_v1_raw_sha256(bytes).expect("derive cid");
        let parsed = Cid::try_from(cid.as_str()).expect("derived cid parses");
        assert!(verify_bytes_against_cid(&parsed, bytes).is_ok());
        assert_eq!(
            verify_bytes_against_cid(&parsed, b"different bytes"),
            Err(VerifyError::HashMismatch { cid })
        );
    }
}
