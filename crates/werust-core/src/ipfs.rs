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

use std::sync::{Arc, Mutex};

use fetcher::{ContentRetriever, RetrieveError};
use renderer::{RendererError, SchemeRequest, SchemeResponse};

use crate::redirects::{
    match_fallback, parse_redirects, FallbackAction, RedirectsError, DEFAULT_404_PATH,
    REDIRECTS_PATH,
};

/// The custom scheme this module resolves: `ipfs`.
///
/// Kept as one constant so the backend that registers the scheme handler
/// (`install_ipfs`) and this resolver agree on the single scheme name. A backend
/// registers a handler for `<IPFS_SCHEME>://…` requests and routes each through
/// [`resolve_ipfs_request`].
pub const IPFS_SCHEME: &str = "ipfs";

/// The marker every "this request is being redirected" failure reason carries, so
/// the shell can tell its OWN redirect hand-off apart from a real load failure.
///
/// A matched 3xx answers the intercepted request with a fail-closed error (nothing
/// may render under the redirected-FROM url), and the backend surfaces that as a
/// failed load whose reason is this string. That failure is BOOKKEEPING, not
/// something to show the user: the shell is about to navigate to the target, so it
/// suppresses the banner for a reason carrying this marker
/// (`BrowserShell::pump`). A refusal (an off-root target, or a chain the sink
/// bounded) deliberately does NOT carry it — those are real failures the user must
/// see.
pub const REDIRECT_NAVIGATING_MARKER: &str = "_redirects redirect pending navigation";

/// The maximum number of consecutive `_redirects` 3xx hops werust follows before
/// it refuses (IPIP-0002 names no bound; browsers cap around 20, werust is
/// deliberately tighter because each hop is a full content-addressed retrieval).
///
/// A site whose rules genuinely need more than this is indistinguishable from a
/// site whose rules loop, so the chain fails closed with a legible reason rather
/// than spinning. Reset on every user-initiated navigation
/// ([`RedirectSink::reset`]), so a bounded chain never poisons later browsing.
pub const MAX_REDIRECT_HOPS: usize = 5;

/// The default MIME type for an `ipfs://` response whose path gives no better
/// hint (the CID root, or a path with no recognized extension).
///
/// A content-addressed page is a page: the default is `text/html` so an
/// `ipfs://<cid>` (or `ipfs://<cid>/`) load renders as a document at parity with
/// a served page, rather than being offered for download.
const DEFAULT_MIME_TYPE: &str = "text/html";

/// The hand-off for a `_redirects` 3xx: the `ipfs://` URL the SHELL must navigate
/// to, plus the chain bound that keeps a redirecting site from looping.
///
/// # Why a shared sink rather than a return value
///
/// A 3xx is a NAVIGATION, not an answer to the intercepted request: it moves the
/// URL bar and the session history, which the scheme-resolution seam cannot
/// express ([`SchemeResponse::status`] is explicitly NOT a redirect channel). But
/// the scheme handler is a `Send` closure owned by the backend, while the shell
/// that can actually navigate is `!Send` and lives on the UI thread — and on
/// desktop the resolution runs on a WORKER thread entirely
/// (`docs/adr/0008`). So the resolver PUSHES the target here (an
/// `Arc<Mutex<_>>` clone the handler owns, the same idiom as the Android
/// backend's `pending_eval` queue) and the shell DRAINS it on its existing pump
/// cadence, navigating through the normal path. That navigation re-enters the
/// `ipfs://` handler, so the redirect target is hash-verified by the SAME
/// retrieval as any other page — werust never vouches for a target it did not
/// fetch.
///
/// (Distinct from `webview-renderer`'s `RequestSink`, which is where a COMPLETED
/// request's bytes are delivered. This sink carries a NAVIGATION the shell must
/// perform; nothing is ever served through it.)
///
/// # The chain bound (the loop guard)
///
/// Because each hop is a fresh navigation, the bound cannot live in a recursion
/// depth: it lives here. The sink counts hops and remembers the targets already
/// visited in the CURRENT chain, so a cycle (`/a -> /b -> /a`) or a chain longer
/// than [`MAX_REDIRECT_HOPS`] is refused with a legible reason and queues
/// NOTHING. [`reset`](RedirectSink::reset) starts a fresh chain and is called by
/// every user-initiated navigation.
///
/// Cloning shares one chain (it is an `Arc` handle), which is the point: the
/// handler's clone and the shell's clone are the same sink.
#[derive(Debug, Clone, Default)]
pub struct RedirectSink {
    chain: Arc<Mutex<RedirectChain>>,
}

/// The interior of a [`RedirectSink`]: one in-flight redirect chain.
#[derive(Debug, Default)]
struct RedirectChain {
    /// The absolute `ipfs://` URL the shell has not navigated to yet.
    pending: Option<String>,
    /// The targets already redirected TO in this chain, so a repeat is a cycle.
    visited: Vec<String>,
}

impl RedirectSink {
    /// A fresh sink with an empty chain.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue `target` (an absolute `ipfs://<rootcid><path>` URL) as the next
    /// navigation, or refuse it with the reason the chain may not continue.
    ///
    /// Refuses — queueing nothing — when the target has already been redirected
    /// to in this chain (a cycle) or the chain has reached
    /// [`MAX_REDIRECT_HOPS`]. A poisoned lock is treated as a refusal for the
    /// same fail-closed reason: werust would rather not redirect than redirect
    /// blind.
    pub(crate) fn queue(&self, target: &str) -> Result<(), String> {
        let Ok(mut chain) = self.chain.lock() else {
            return Err("the redirect chain state is unusable".to_string());
        };
        if chain.visited.iter().any(|seen| seen == target) {
            return Err(format!(
                "it revisits `{target}`, so the site's rules form a redirect cycle"
            ));
        }
        if chain.visited.len() >= MAX_REDIRECT_HOPS {
            return Err(format!(
                "the redirect chain is longer than the {MAX_REDIRECT_HOPS} hop limit"
            ));
        }
        chain.visited.push(target.to_string());
        chain.pending = Some(target.to_string());
        Ok(())
    }

    /// Take the `ipfs://` URL the shell must navigate to, if a matched 3xx rule
    /// queued one. Drained ONCE (a second call yields [`None`] until another
    /// redirect is queued), so a shell that pumps on a timer cannot re-navigate
    /// the same target in a loop of its own.
    #[must_use]
    pub fn take_pending(&self) -> Option<String> {
        self.chain.lock().ok()?.pending.take()
    }

    /// Start a FRESH chain: forget the hops walked so far (and any undrained
    /// target).
    ///
    /// Called by every USER-initiated navigation, so the hop budget bounds one
    /// site's redirect chain rather than a whole session: a user who types a URL,
    /// clicks a link, or goes back gets the full budget again.
    pub fn reset(&self) {
        if let Ok(mut chain) = self.chain.lock() {
            chain.pending = None;
            chain.visited.clear();
        }
    }
}

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
///
/// A trailing query string (`?…`) and fragment (`#…`) are STRIPPED before the
/// path is taken: they are request/anchor modifiers, NOT part of the
/// content-addressed DAG path. This is load-bearing for SvelteKit
/// `adapter-static` sites, whose client router fetches a route's data as
/// `<page>/__data.json?x-sveltekit-invalidated=…` on every client-side
/// navigation — the invalidation query is always present. Without stripping it,
/// the last path segment becomes a literal `__data.json?x-sveltekit-invalidated=01`
/// that matches no directory entry, the retrieval fails `PathNotFound`, and
/// SvelteKit renders its client error boundary (the ronan.eth blog "500").
/// (`docs/spikes/diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture/DIAGNOSIS.md`.)
pub fn parse_ipfs_uri(uri: &str) -> Result<IpfsRef, RendererError> {
    let after_scheme = uri
        .strip_prefix("ipfs://")
        .ok_or_else(|| RendererError::InvalidUrl(uri.to_string()))?;
    // Drop a fragment first (it may itself contain a `?`), then a query string;
    // the remainder is the content-addressed `<cid>[/path]` the DAG resolves.
    // A `?`/`#` cannot appear in a CID or a static file path, so cutting at the
    // first of either is unambiguous.
    let rest = after_scheme
        .split_once('#')
        .map_or(after_scheme, |(head, _)| head);
    let rest = rest.split_once('?').map_or(rest, |(head, _)| head);
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

/// Map a `_redirects` problem onto the seam's [`RendererError`], so a broken,
/// off-root, or unsupported rule FAILS the load with its distinct reason.
///
/// IPIP-0002 §3.4 requires an unreadable/unparseable redirects file to be
/// surfaced rather than ignored: ignoring it would serve a DIFFERENT page than
/// the site's author wrote (or a hard not-found where they wrote a fallback), so
/// every [`RedirectsError`] is a legible failed load.
fn redirects_error_to_renderer_error(err: RedirectsError) -> RendererError {
    RendererError::Backend(format!("ipfs:// _redirects fallback failed: {err}"))
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
///
/// # The one exception: a NOT-FOUND path consults the site's own rules
///
/// A [`PathNotFound`](RetrieveError::PathNotFound) — and ONLY that — is handed to
/// the site's IPFS web-pathing rules before it fails: a root `_redirects`
/// (IPIP-0002, [`crate::redirects`]) or the default root `404.html`, exactly as
/// an HTTP gateway resolves it, which is what makes `jolly-roger.eth/unknown`
/// serve that site's own 404 page instead of a hard error. The fallback target is
/// fetched through the SAME verifying retrieval, by path under the SAME root CID,
/// so this adds NO verification bypass and NO cross-site reach; a site that ships
/// neither file is completely unchanged (the feature is opt-in per site).
///
/// # A matched 3xx rule NAVIGATES (it is not answered here)
///
/// A rule asking for `301`/`302`/`303`/`307`/`308` is a REDIRECT: nothing is
/// served for the intercepted (old) URL. The absolute `ipfs://<rootcid><to>`
/// target is pushed into `redirects` for the shell to navigate to, and this
/// request is answered with a fail-closed error naming the redirect, so no page
/// is ever rendered under the old URL. The shell's navigation re-enters this
/// resolver for the target, so the redirected page is hash-verified by the SAME
/// retrieval; the [`RedirectSink`] bounds the hop count so a redirecting site
/// cannot loop.
pub fn resolve_ipfs_request(
    retriever: &dyn ContentRetriever,
    request: &SchemeRequest,
    redirects: &RedirectSink,
) -> Result<SchemeResponse, RendererError> {
    let reference = parse_ipfs_uri(&request.uri)?;
    // Route THROUGH the verifying retriever: bytes come back only after every
    // block in the resolved resource's DAG hashed to its own CID. Any failure is
    // a hard failure that fails the load, never a silent render of unverified
    // bytes.
    match retriever.retrieve(&reference.cid, &reference.path) {
        Ok(content) => Ok(SchemeResponse::ok(
            mime_type_for_path(&reference.path),
            content.bytes,
        )),
        // The ONLY branch the site's web-pathing rules are consulted on: a path
        // that is not in the DAG (IPIP-0002 §3.3, "no forced redirects"). An
        // existing resource is served above, untouched, so a catch-all rule can
        // never shadow a real page and a site without the opt-in files pays no
        // cost at all.
        Err(not_found @ RetrieveError::PathNotFound { .. }) => {
            resolve_not_found_fallback(retriever, &reference, not_found, redirects)
        }
        Err(other) => Err(retrieve_error_to_renderer_error(other)),
    }
}

/// Resolve a NOT-FOUND path per the SITE's own web-pathing rules (IPIP-0002),
/// or surface the original honest not-found.
///
/// The order mirrors an HTTP gateway: the site's root `_redirects` first (its
/// first matching rule decides), then the DEFAULT root `404.html` convention,
/// then — for a site that ships neither — the untouched fail-closed
/// [`PathNotFound`](RetrieveError::PathNotFound) werust always gave. So the
/// feature is strictly OPT-IN per site.
///
/// TRUST (the load-bearing part): the `_redirects` file AND the rule's target are
/// fetched through the SAME [`ContentRetriever`] as any other resource, by PATH
/// under the SAME root CID — every block hash-verified, the budget unchanged,
/// nothing bypassed. A target that would leave the root CID is refused by
/// [`match_fallback`] before it is ever fetched (the unique-origin rule recorded
/// in [`crate::redirects`]), so a site's rules can only ever serve that site's
/// own content. A target that does not exist is itself a not-found (fail-closed,
/// no second round of rules).
fn resolve_not_found_fallback(
    retriever: &dyn ContentRetriever,
    reference: &IpfsRef,
    not_found: RetrieveError,
    redirects: &RedirectSink,
) -> Result<SchemeResponse, RendererError> {
    match probe_optional(retriever, &reference.cid, REDIRECTS_PATH)? {
        Some(file) => {
            let rules = parse_redirects(&file).map_err(redirects_error_to_renderer_error)?;
            match match_fallback(&rules, &reference.path) {
                Some(Ok(FallbackAction::Serve { path, status })) => {
                    serve_fallback_target(retriever, reference, &path, status)
                }
                Some(Ok(FallbackAction::Redirect { path, status })) => {
                    Err(queue_redirect(redirects, reference, &path, status))
                }
                Some(Err(e)) => Err(redirects_error_to_renderer_error(e)),
                // The file exists but says nothing about this path: fall through
                // to the default `404.html` convention, then to the honest
                // not-found.
                None => serve_default_404(retriever, reference, not_found),
            }
        }
        // No `_redirects` at all: the site simply did not opt in to rules.
        None => serve_default_404(retriever, reference, not_found),
    }
}

/// Hand a matched 3xx rule's target to the shell as a NAVIGATION, and answer the
/// intercepted (old) request with the fail-closed error that says so.
///
/// The target is made ABSOLUTE against the request's OWN root CID
/// (`ipfs://<cid><path>`) — never against anything the rule could name, because
/// [`match_fallback`] already refused any `to` that leaves the root. So the
/// navigation stays inside the same content root, which is also what keeps the
/// shell's root-CID-prefix `ens_pages` association intact: a redirect inside an
/// ENS site lands on the SAME root CID, so the bar keeps showing the site's
/// `.eth` identity (plus the new in-site path) instead of leaking a raw CID.
///
/// The return value is ALWAYS an error: nothing may render for the old URL. When
/// the chain bound refuses the hop, the error says THAT instead and nothing is
/// queued, so the redirect chain terminates rather than looping.
fn queue_redirect(
    redirects: &RedirectSink,
    reference: &IpfsRef,
    target: &str,
    status: u16,
) -> RendererError {
    let url = format!("{IPFS_SCHEME}://{cid}{target}", cid = reference.cid);
    match redirects.queue(&url) {
        // Carries `REDIRECT_NAVIGATING_MARKER`: the shell is about to navigate, so
        // this failure is bookkeeping and its banner is suppressed.
        Ok(()) => RendererError::Backend(format!(
            "ipfs:// {REDIRECT_NAVIGATING_MARKER}: {status} to `{target}`"
        )),
        // A REFUSAL (off-root, cycle, over-long chain) carries no marker: the
        // chain stops here and the user must see why.
        Err(reason) => RendererError::Backend(format!(
            "ipfs:// _redirects {status} redirect to `{target}` refused: {reason}"
        )),
    }
}

/// Probe for an OPTIONAL fallback file (`_redirects`, `404.html`) under the root
/// CID, distinguishing "the site does not ship it" from "it is there but did not
/// verify".
///
/// [`Ok(None)`] means ABSENT, which is not an error: the caller falls through to
/// the next convention and ultimately to the original honest not-found (so a site
/// that opted into nothing behaves exactly as it did before this feature).
/// Absence arrives in two shapes and BOTH must count as absent: a local
/// [`PathNotFound`](RetrieveError::PathNotFound) when the root's listing is
/// already at hand, and a [`Source`](RetrieveError::Source) transport failure
/// when the gateway answers the scoped request for a non-existent path with an
/// HTTP error (how absence is signalled over that transport, and gateway-
/// dependent). Treating a transport failure on an OPTIONAL probe as absence
/// cannot yield content and cannot weaken verification: the worst case is the
/// pre-existing honest not-found.
///
/// Every VERIFICATION-class failure (tamper, incomplete DAG, budget, malformed,
/// unsupported codec/hash, invalid CID) still fails the load on its real reason:
/// the fallback never degrades a tamper signal into a plain not-found.
fn probe_optional(
    retriever: &dyn ContentRetriever,
    cid: &str,
    path: &str,
) -> Result<Option<Vec<u8>>, RendererError> {
    match retriever.retrieve(cid, path) {
        Ok(content) => Ok(Some(content.bytes)),
        Err(RetrieveError::PathNotFound { .. } | RetrieveError::Source(_)) => Ok(None),
        Err(other) => Err(retrieve_error_to_renderer_error(other)),
    }
}

/// The DEFAULT custom-error-page convention: a site with a root `404.html` (and
/// no rule naming something else) serves it, with a not-found status, for a path
/// that is not in the DAG. A site without one keeps the original honest
/// not-found.
fn serve_default_404(
    retriever: &dyn ContentRetriever,
    reference: &IpfsRef,
    not_found: RetrieveError,
) -> Result<SchemeResponse, RendererError> {
    match probe_optional(retriever, &reference.cid, DEFAULT_404_PATH)? {
        Some(bytes) => Ok(SchemeResponse {
            mime_type: mime_type_for_path(DEFAULT_404_PATH).to_string(),
            body: bytes,
            status: 404,
        }),
        // No default error page either: the site opted into nothing, so werust's
        // honest fail-closed not-found stands, exactly as before this feature.
        None => Err(retrieve_error_to_renderer_error(not_found)),
    }
}

/// Fetch a matched rule's target through the SAME verified retrieval and answer
/// the REQUESTED url with it, at the rule's status.
///
/// Nothing navigates: the bytes are the answer to the intercepted request, so a
/// `200` rewrite (the SPA/PWA case) and a `404`/`410`/`451` error page both leave
/// the URL bar — and therefore the page's identity and trust posture — exactly
/// where they were. A target that does not resolve is itself a not-found, named
/// so the site author can see WHICH target was missing; the rules are NOT
/// re-evaluated for it (no fallback loops).
fn serve_fallback_target(
    retriever: &dyn ContentRetriever,
    reference: &IpfsRef,
    target: &str,
    status: u16,
) -> Result<SchemeResponse, RendererError> {
    match retriever.retrieve(&reference.cid, target) {
        Ok(content) => Ok(SchemeResponse {
            mime_type: mime_type_for_path(target).to_string(),
            body: content.bytes,
            status,
        }),
        Err(RetrieveError::PathNotFound { .. }) => Err(RendererError::Backend(format!(
            "ipfs:// _redirects fallback failed: target `{target}` is not in the site's dag"
        ))),
        Err(other) => Err(retrieve_error_to_renderer_error(other)),
    }
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
        /// Every `(cid, path)` asked for, in call order, so a test can assert
        /// WHICH extra retrievals the fallback did (and did not) make.
        asked: std::cell::RefCell<Vec<String>>,
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

        /// The paths retrieved so far, in call order.
        fn asked_paths(&self) -> Vec<String> {
            self.asked.borrow().clone()
        }
    }

    impl ContentRetriever for PinnedRetriever {
        fn retrieve(&self, cid: &str, path: &str) -> Result<RetrievedContent, RetrieveError> {
            self.asked.borrow_mut().push(path.to_string());
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
    fn strips_a_query_string_from_the_resolved_dag_path() {
        // SvelteKit's client router fetches a route's data as
        // `<page>/__data.json?x-sveltekit-invalidated=…` on every client-side
        // navigation (the invalidation param is ALWAYS appended). A query string
        // is a REQUEST modifier, not part of the content-addressed DAG path: the
        // resource named in the DAG is `/blog/__data.json`, not
        // `/blog/__data.json?x-sveltekit-invalidated=01`. The query (and any
        // fragment) must be stripped before the path is resolved, or the last
        // segment is a literal `__data.json?x-sveltekit-invalidated=01` that
        // matches no directory entry and the load fails.
        let r = parse_ipfs_uri("ipfs://bafydir/blog/__data.json?x-sveltekit-invalidated=01")
            .expect("a query-carrying data uri");
        assert_eq!(r.cid, "bafydir");
        assert_eq!(
            r.path, "/blog/__data.json",
            "the query string must not leak into the resolved dag path"
        );
    }

    #[test]
    fn strips_a_fragment_from_the_resolved_dag_path() {
        // A URL fragment (`#…`) is a client-side anchor, never part of the
        // content-addressed path; it must be stripped like the query.
        let r = parse_ipfs_uri("ipfs://bafydir/blog/#posts").expect("a fragment-carrying uri");
        assert_eq!(r.cid, "bafydir");
        assert_eq!(r.path, "/blog/");
        // A bare query on the root authority leaves an empty path, not `?…`.
        let root = parse_ipfs_uri("ipfs://bafydir?foo=bar").expect("a query on a bare cid");
        assert_eq!(root.cid, "bafydir");
        assert_eq!(root.path, "");
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
            &RedirectSink::new(),
        )
        .expect("directory root resolves index.html");
        assert_eq!(response.body, index);
        assert_eq!(response.mime_type, "text/html");
    }

    #[test]
    fn a_sveltekit_data_fetch_with_the_invalidated_query_resolves_the_nested_data() {
        // The end-to-end SvelteKit-over-ipfs regression at the seam: the client
        // router's `/blog/__data.json?x-sveltekit-invalidated=01` fetch must
        // resolve the SAME verified `/blog/__data.json` bytes the build ships,
        // with an application/json MIME. Before the fix the query leaked into the
        // path segment, so the retriever was asked for a resource named
        // `__data.json?x-sveltekit-invalidated=01` and the load failed (SvelteKit
        // then rendered its client error boundary: the reported "500").
        let cid = "bafysvelteroot";
        let data_json = br#"{"type":"data","nodes":[null,{"type":"data"}]}"#;
        let mut retriever = PinnedRetriever::default();
        // The retriever is keyed on the CLEAN dag path; a leaked query would miss.
        retriever.put(cid, "/blog/__data.json", data_json);

        let response = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/blog/__data.json?x-sveltekit-invalidated=01"),
            },
            &RedirectSink::new(),
        )
        .expect("the nested __data.json resolves despite the client-nav query");
        assert_eq!(response.body, data_json);
        assert_eq!(
            response.mime_type, "application/json",
            "__data.json is served as json for parity"
        );
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
            &RedirectSink::new(),
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
            &RedirectSink::new(),
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
            &RedirectSink::new(),
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
            &RedirectSink::new(),
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
            &RedirectSink::new(),
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
            &RedirectSink::new(),
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
            &RedirectSink::new(),
        )
        .expect_err("a missing sub-resource fails the load");
        assert!(matches!(err, RendererError::Backend(_)));
    }

    // -----------------------------------------------------------------------
    // The `_redirects` / custom-404 fallback AT THE SEAM (IPIP-0002).
    //
    // The rule grammar/matching is unit-tested in `crate::redirects`, and the
    // whole thing is proven over a REAL content-addressed DAG in
    // `tests/ipfs_redirects_fixture.rs`. Here we pin the seam glue: WHICH paths
    // are retrieved (and which are NOT), and how the action becomes a response.
    // -----------------------------------------------------------------------

    #[test]
    fn a_not_found_path_serves_the_sites_custom_404_page_with_a_not_found_status() {
        // The field case (jolly-roger.eth/unknown): the site's root `_redirects`
        // is `/* /404.html/index.html 404`, so a not-found path serves that
        // page's VERIFIED bytes with a not-found status, instead of a hard error.
        let cid = "bafyjollyroger";
        let page = b"<!doctype html><title>404</title><h1>arr, nothing here</h1>";
        let mut retriever = PinnedRetriever::default();
        retriever.put(cid, "/_redirects", b"/* /404.html/index.html 404\n");
        retriever.put(cid, "/404.html/index.html", page);

        let response = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/unknown"),
            },
            &RedirectSink::new(),
        )
        .expect("the site's rules resolve the not-found path");
        assert_eq!(response.body, page, "the site's own 404 page is served");
        assert_eq!(response.status, 404, "with the honest not-found status");
        assert_eq!(
            response.mime_type, "text/html",
            "the mime comes from the TARGET path, so the page renders as a page"
        );
    }

    #[test]
    fn a_200_rule_serves_the_rewrite_target_as_the_requested_resource() {
        // A 200 rule is a REWRITE: the target's bytes answer the REQUESTED url.
        // Nothing navigates (the resolver returns a response, never a
        // navigation), so the URL bar — and with it the page identity the trust
        // indicator describes — is untouched.
        let cid = "bafyspa";
        let app = b"<!doctype html><title>app</title><div id=app></div>";
        let mut retriever = PinnedRetriever::default();
        retriever.put(cid, "/_redirects", b"/app/* /app/index.html 200\n");
        retriever.put(cid, "/app/index.html", app);

        let response = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/app/deep/client-route"),
            },
            &RedirectSink::new(),
        )
        .expect("the spa rewrite resolves");
        assert_eq!(response.body, app);
        assert_eq!(response.status, 200);
    }

    #[test]
    fn a_resolvable_path_never_reads_the_redirects_file_at_all() {
        // IPIP-0002 §3.3 ("no forced redirects") AND the opt-in cost promise: the
        // rules are consulted ONLY for a path that is not in the DAG, so a normal
        // page load does not pay a single extra retrieval — and a catch-all rule
        // can never shadow a real page.
        let cid = "bafysite";
        let index = b"<!doctype html><title>home</title>";
        let mut retriever = PinnedRetriever::default();
        retriever.put(cid, "/", index);
        retriever.put(cid, "/_redirects", b"/* /404.html 404\n");

        let response = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/"),
            },
            &RedirectSink::new(),
        )
        .expect("an existing page resolves normally");
        assert_eq!(response.body, index);
        assert_eq!(response.status, 200);
        assert_eq!(
            retriever.asked_paths(),
            vec!["/".to_string()],
            "a found resource must not trigger any _redirects lookup"
        );
    }

    #[test]
    fn a_site_with_no_redirects_and_no_404_page_keeps_the_honest_not_found() {
        // The feature is OPT-IN per site: a site shipping neither file behaves
        // exactly as before — a fail-closed not-found naming the requested path.
        let cid = "bafyplain";
        let retriever = PinnedRetriever::default();

        let err = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/unknown"),
            },
            &RedirectSink::new(),
        )
        .expect_err("an opt-out site keeps the hard not-found");
        let RendererError::Backend(reason) = err else {
            panic!("expected a fail-closed backend error");
        };
        assert!(
            reason.contains("path not found") && reason.contains("/unknown"),
            "the ORIGINAL not-found reason is preserved, got: {reason}"
        );
    }

    #[test]
    fn a_root_404_html_is_served_when_the_site_ships_no_redirects() {
        // The DEFAULT convention: a root `404.html` with no `_redirects` at all
        // is still honoured, exactly as a gateway honours it.
        let cid = "bafydefault404";
        let page = b"<!doctype html><title>404</title>";
        let mut retriever = PinnedRetriever::default();
        retriever.put(cid, "/404.html", page);

        let response = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/nope"),
            },
            &RedirectSink::new(),
        )
        .expect("the default 404.html is honoured");
        assert_eq!(response.body, page);
        assert_eq!(response.status, 404);
    }

    #[test]
    fn a_redirects_file_that_matches_nothing_falls_through_to_the_default_404() {
        // A `_redirects` that simply says nothing about this path is not a match:
        // the default `404.html` convention still applies (and, without one, the
        // honest not-found stands).
        let cid = "bafypartial";
        let page = b"<!doctype html><title>404</title>";
        let mut retriever = PinnedRetriever::default();
        retriever.put(cid, "/_redirects", b"/only/this /that.html 200\n");
        retriever.put(cid, "/404.html", page);

        let response = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/elsewhere"),
            },
            &RedirectSink::new(),
        )
        .expect("an unmatched path falls through to the default 404 page");
        assert_eq!(response.body, page);
        assert_eq!(response.status, 404);
    }

    #[test]
    fn a_fallback_target_that_is_missing_fails_closed_and_names_the_target() {
        // No invented content, and no second round of rules: a `to` that is not
        // in the DAG is itself a not-found, naming WHICH target was missing.
        let cid = "bafybadtarget";
        let mut retriever = PinnedRetriever::default();
        retriever.put(cid, "/_redirects", b"/* /missing-404.html 404\n");

        let err = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/unknown"),
            },
            &RedirectSink::new(),
        )
        .expect_err("a missing target must fail closed");
        let RendererError::Backend(reason) = err else {
            panic!("expected a fail-closed backend error");
        };
        assert!(
            reason.contains("missing-404.html"),
            "the failure names the unresolvable target, got: {reason}"
        );
    }

    #[test]
    fn an_off_root_target_is_refused_and_never_fetched() {
        // The unique-origin rule: a `to` that leaves the root CID is refused
        // BEFORE any retrieval, so a site's rules can never reach at (or make it
        // look like it is serving) another content root.
        let cid = "bafyimpersonator";
        let mut retriever = PinnedRetriever::default();
        retriever.put(
            cid,
            "/_redirects",
            b"/* https://evil.example/404.html 404\n",
        );

        let err = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/unknown"),
            },
            &RedirectSink::new(),
        )
        .expect_err("an off-root target must be refused");
        let RendererError::Backend(reason) = err else {
            panic!("expected a fail-closed backend error");
        };
        assert!(
            reason.contains("root cid"),
            "the refusal names the unique-origin rule, got: {reason}"
        );
        assert_eq!(
            retriever.asked_paths(),
            vec!["/unknown".to_string(), "/_redirects".to_string()],
            "the off-root target is never fetched"
        );
    }

    #[test]
    fn a_matching_3xx_rule_queues_a_navigation_to_the_target_under_the_same_root_cid() {
        // The 3xx NAVIGATION: a matching redirect rule does NOT serve anything in
        // place; it queues an absolute `ipfs://<rootcid><to>` navigation for the
        // shell (bar + history move), with the `:splat` injected. The intercepted
        // request itself is answered fail-closed (nothing is rendered for the OLD
        // url), so the only page the user ever sees is the redirect target,
        // hash-verified by the fresh retrieval that navigation triggers.
        let cid = "bafyredirect";
        let mut retriever = PinnedRetriever::default();
        retriever.put(cid, "/_redirects", b"/old/* /new/:splat 301\n");
        retriever.put(cid, "/404.html", b"<h1>404</h1>");
        let redirects = RedirectSink::new();

        let err = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/old/thing"),
            },
            &redirects,
        )
        .expect_err("the redirected request itself renders nothing");
        let RendererError::Backend(reason) = err else {
            panic!("expected a fail-closed backend error");
        };
        assert!(
            reason.contains("301") && reason.contains("/new/thing"),
            "the reason names the status and the target, got: {reason}"
        );
        assert_eq!(
            redirects.take_pending(),
            Some(format!("ipfs://{cid}/new/thing")),
            "the navigation target is absolute, under the SAME root cid"
        );
        assert_eq!(
            redirects.take_pending(),
            None,
            "the sink is drained once, so the shell cannot re-navigate"
        );
        assert_eq!(
            retriever.asked_paths(),
            vec!["/old/thing".to_string(), "/_redirects".to_string()],
            "the target is NOT fetched here: the navigation re-enters the handler"
        );
    }

    #[test]
    fn every_3xx_status_navigates_and_a_defaulted_status_redirects_too() {
        // All five codes navigate identically (werust caches nothing, so it has no
        // permanence to honour differently), and a rule with NO status is the
        // spec's default 301, i.e. also a navigation.
        for (rule, status) in [
            ("/old /new.html 301\n", "301"),
            ("/old /new.html 302\n", "302"),
            ("/old /new.html 303\n", "303"),
            ("/old /new.html 307\n", "307"),
            ("/old /new.html 308\n", "308"),
            ("/old /new.html\n", "301"),
        ] {
            let cid = "bafyeach3xx";
            let mut retriever = PinnedRetriever::default();
            retriever.put(cid, "/_redirects", rule.as_bytes());
            let redirects = RedirectSink::new();
            let err = resolve_ipfs_request(
                &retriever,
                &SchemeRequest {
                    uri: format!("ipfs://{cid}/old"),
                },
                &redirects,
            )
            .expect_err("a redirect renders nothing in place");
            assert!(
                matches!(&err, RendererError::Backend(msg) if msg.contains(status)),
                "`{rule}` must redirect with status {status}, got: {err:?}"
            );
            assert_eq!(
                redirects.take_pending(),
                Some(format!("ipfs://{cid}/new.html"))
            );
        }
    }

    #[test]
    fn an_off_root_3xx_target_is_refused_and_never_queued_for_navigation() {
        // The unique-origin rule holds for a NAVIGATION too, and it is the
        // load-bearing one here: queueing an off-root target would navigate the
        // shell to ANOTHER content root (or an `https://` origin) on a site's own
        // say-so. Refused before anything is fetched or queued.
        let cid = "bafyimpersonator";
        let mut retriever = PinnedRetriever::default();
        retriever.put(cid, "/_redirects", b"/* https://evil.example/landing 302\n");
        let redirects = RedirectSink::new();

        let err = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/unknown"),
            },
            &redirects,
        )
        .expect_err("an off-root redirect must be refused");
        let RendererError::Backend(reason) = err else {
            panic!("expected a fail-closed backend error");
        };
        assert!(
            reason.contains("root cid"),
            "the refusal names the unique-origin rule, got: {reason}"
        );
        assert_eq!(
            redirects.take_pending(),
            None,
            "an off-root target is NEVER queued as a navigation"
        );
    }

    #[test]
    fn a_redirect_chain_is_bounded_and_a_cycle_fails_closed() {
        // The loop guard. Each hop is a fresh navigation, so the bound lives in
        // the sink: a chain longer than `MAX_REDIRECT_HOPS`, or a hop back onto an
        // already-visited target, fails closed with a legible reason and queues
        // NOTHING — there is no unbounded loop.
        let cid = "bafyloop";
        let mut retriever = PinnedRetriever::default();
        // A/B ping-pong: each path redirects to the other, forever.
        retriever.put(cid, "/_redirects", b"/a /b 301\n/b /a 301\n");
        let redirects = RedirectSink::new();

        let mut next = format!("ipfs://{cid}/a");
        let mut hops = 0;
        let reason = loop {
            let err = resolve_ipfs_request(&retriever, &SchemeRequest { uri: next }, &redirects)
                .expect_err("a redirect renders nothing in place");
            let RendererError::Backend(reason) = err else {
                panic!("expected a fail-closed backend error");
            };
            match redirects.take_pending() {
                Some(target) => {
                    hops += 1;
                    assert!(
                        hops <= MAX_REDIRECT_HOPS,
                        "the chain must stop at {MAX_REDIRECT_HOPS} hops, did {hops}"
                    );
                    next = target;
                }
                // The chain refused to go further: this is the fail-closed end.
                None => break reason,
            }
        };
        assert!(
            reason.contains("redirect"),
            "the chain stops with a legible redirect reason, got: {reason}"
        );
        // A user-initiated navigation starts a FRESH chain, so a bounded chain
        // never poisons later browsing.
        redirects.reset();
        let err = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/a"),
            },
            &redirects,
        )
        .expect_err("a redirect renders nothing in place");
        assert!(matches!(err, RendererError::Backend(_)));
        assert_eq!(
            redirects.take_pending(),
            Some(format!("ipfs://{cid}/b")),
            "after a reset the chain budget is fresh"
        );
    }

    #[test]
    fn a_redirect_target_that_does_not_resolve_fails_closed_on_the_next_hop() {
        // Verification intact: werust does not pre-fetch or vouch for the target;
        // the NAVIGATION re-enters the handler, and a target that is not in the
        // dag (and that no rule covers) is the honest fail-closed not-found —
        // nothing is invented for it.
        let cid = "bafydeadend";
        let mut retriever = PinnedRetriever::default();
        retriever.put(cid, "/_redirects", b"/old /gone.html 301\n");
        let redirects = RedirectSink::new();

        let _ = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/old"),
            },
            &redirects,
        );
        let target = redirects.take_pending().expect("the redirect is queued");
        let err = resolve_ipfs_request(&retriever, &SchemeRequest { uri: target }, &redirects)
            .expect_err("a missing redirect target fails closed");
        let RendererError::Backend(reason) = err else {
            panic!("expected a fail-closed backend error");
        };
        assert!(
            reason.contains("gone.html"),
            "the failure names the path that did not resolve, got: {reason}"
        );
        assert_eq!(
            redirects.take_pending(),
            None,
            "a dead end queues no further navigation"
        );
    }

    #[test]
    fn a_gateway_404_on_the_optional_probe_counts_as_absent_not_as_a_hard_failure() {
        // A real trustless gateway signals "this path is not in the dag" for an
        // OPTIONAL probe (`/_redirects`, `/404.html`) as an HTTP error on the
        // scoped request, which surfaces as a `Source` transport failure rather
        // than a local `PathNotFound`. Both shapes mean ABSENT, so a site with no
        // `_redirects` must still reach its default `404.html` (and, with neither,
        // its original honest not-found) instead of failing on the probe.
        let cid = "bafyprobe";
        let page = b"<!doctype html><title>404</title>";
        let mut retriever = PinnedRetriever::default();
        retriever.fail(
            cid,
            "/_redirects",
            RetrieveError::Source(fetcher::FetchError::Transport(
                "gateway returned status 404".into(),
            )),
        );
        retriever.put(cid, "/404.html", page);

        let response = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/unknown"),
            },
            &RedirectSink::new(),
        )
        .expect("an absent _redirects must not fail the fallback");
        assert_eq!(response.body, page);
        assert_eq!(response.status, 404);
    }

    #[test]
    fn a_tampered_redirects_file_fails_the_load_and_never_falls_back_to_guessing() {
        // The `_redirects` file is itself content: if IT fails to verify, the load
        // fails on the REAL reason (tamper), rather than pretending the site has
        // no rules and serving something else.
        let cid = "bafytamperedrules";
        let mut retriever = PinnedRetriever::default();
        retriever.fail(
            cid,
            "/_redirects",
            RetrieveError::BlockHashMismatch {
                cid: cid.to_string(),
            },
        );
        retriever.put(cid, "/404.html", b"<h1>404</h1>");

        let err = resolve_ipfs_request(
            &retriever,
            &SchemeRequest {
                uri: format!("ipfs://{cid}/unknown"),
            },
            &RedirectSink::new(),
        )
        .expect_err("a tampered _redirects must fail the load");
        let RendererError::Backend(reason) = err else {
            panic!("expected a fail-closed backend error");
        };
        assert!(
            reason.contains("mismatch"),
            "the real verify failure is surfaced, got: {reason}"
        );
    }
}
