//! The Android **internal-`https` origin map**: the translation between the
//! core's real `ipfs://<cid>[/path]` URLs and the internal
//! `https://<cid>.ipfs.werust.invalid[/path]` origin the platform `WebView`
//! actually loads.
//!
//! # Why this exists (the opaque-origin root cause)
//!
//! Diagnosed on-device in
//! `docs/spikes/mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md` (task
//! `mobile-ronan-eth-buttons-no-navigation`, field finding D's mobile half): an
//! `ipfs://` document served through `WebViewClient.shouldInterceptRequest`
//! gets an OPAQUE origin in the Android System WebView — the page's origin is
//! the bare string `ipfs://`, with NO host. Two Blink-level consequences kill
//! every SvelteKit client-side navigation on that page:
//!
//! * `fetch()` to an `ipfs://` URL is rejected inside Blink ("URL scheme
//!   \"ipfs\" is not supported") BEFORE the request reaches the network stack,
//!   so `shouldInterceptRequest` never fires for the client router's
//!   `__data.json` fetch — no request, no Network entry, no signal werust can
//!   see.
//! * `history.pushState` to another `ipfs://` path throws `SecurityError`
//!   (the target is not same-origin with the opaque origin), so even the
//!   router's URL update dies.
//!
//! Desktop does NOT have this problem: WebKitGTK's
//! `webkit_web_context_register_uri_scheme` registers `ipfs` as a FIRST-CLASS
//! scheme, so the document gets a real `ipfs://<cid>` origin and both `fetch`
//! and `pushState` work. Android has no scheme-registration API —
//! `shouldInterceptRequest` is interception-only — which is why the mobile
//! symptom (NO navigation at all) is distinct from the desktop one (navigation
//! with a data error).
//!
//! The fix is the internal-`https` fallback recorded for exactly this
//! contingency in
//! `work/notes/observations/mobile-ipfs-interception-mechanism-2026-07-23.md`:
//! the `WebView` loads an internal `https://` origin (which Blink treats as a
//! normal fetchable, `pushState`-able, secure-context origin), and the edge
//! maps every URL between that origin and the core's real `ipfs://` URLs:
//!
//! * OUT (core -> WebView): [`to_webview_url`] maps the pending load
//!   `ipfs://<cid>[/path]` -> `https://<cid>.ipfs.werust.invalid[/path]`, so
//!   `WebView.loadUrl` and every same-origin subresource/`fetch`/`pushState`
//!   stay on the internal origin.
//! * BACK (WebView -> core): [`from_webview_url`] maps every reported or
//!   intercepted URL on the internal origin back to `ipfs://<cid>[/path]`, so
//!   the core's history, the URL bar, the trust machinery, the `_redirects`
//!   main-frame inference, and the debug Network tab all keep speaking
//!   `ipfs://` — the internal origin never leaks into the core.
//!
//! # Decisions baked in
//!
//! * **The CID is the HOST, not a path segment** — one origin PER SITE, so two
//!   content-addressed sites are NOT same-origin with each other (no shared
//!   `localStorage`/IndexedDB, no cross-site `pushState`). A single shared
//!   `https://ipfs.werust.invalid/<cid>/...` origin was rejected on the
//!   thesis's privacy stance (`docs/adr/0001`).
//! * **The host CID is the canonical lowercase base32 CIDv1 form.** Chromium
//!   LOWERCASES hostnames, so a mixed-case CIDv0 (`Qm...`) in the host would
//!   not round-trip. [`to_webview_url`] therefore normalizes any CID
//!   (including a hand-typed CIDv0) to its base32 CIDv1 form — the SAME
//!   canonical form the ENS contenthash decoder already produces
//!   (`werust_core::contenthash`, `Cid::to_string`), so the primary ENS path
//!   sees no change. Consequence: a hand-typed `ipfs://Qm.../` settles onto
//!   its `ipfs://bafy.../` form in the bar — the SAME content under its
//!   canonical name, recorded as an accepted display normalization.
//! * **`.invalid` (RFC 2606)**, never a resolvable TLD: the internal origin
//!   can never collide with a real site, and a leak of the internal URL can
//!   never resolve anywhere.
//! * **Fail soft on the unparseable**: a URL that is not `ipfs://`, or whose
//!   CID does not parse, passes through UNCHANGED both ways (the pre-existing
//!   behaviour keeps handling it); only the well-formed `ipfs://<cid>` case
//!   is remapped.
//! * **The mapping lives in the Rust edge, not Kotlin**: it is pure,
//!   network-isolated, and unit-testable here, and Kotlin stays confined to
//!   the OS edge (every browsing decision is the core's). The ONE Kotlin call
//!   site that loads a URL the core did not surface — the `_blank`/`window.open`
//!   transport — maps through the JNI [`toWebViewUrl`] accessor.

use fetcher::Cid;

/// The host suffix of the internal origin every content-addressed page is
/// served under on Android: `https://<cid>.ipfs.werust.invalid[/path]`.
/// `.invalid` is reserved (RFC 2606) so the origin can never collide with, or
/// resolve to, a real site.
pub const INTERNAL_IPFS_HOST_SUFFIX: &str = ".ipfs.werust.invalid";

/// The scheme of the internal origin the WebView loads.
const INTERNAL_SCHEME: &str = "https";

/// Map a core URL to the URL the platform `WebView` should load: an
/// `ipfs://<cid>[/path][?query][#fragment]` becomes
/// `https://<cid>.ipfs.werust.invalid[/path][?query][#fragment]` with the CID
/// normalized to its lowercase base32 CIDv1 form (Chromium lowercases
/// hostnames, so the mixed-case CIDv0 form cannot round-trip). Any other URL
/// — `https://`, `werust://settings`, an unparseable CID — is returned
/// UNCHANGED.
#[must_use]
pub fn to_webview_url(url: &str) -> String {
    let Some(rest) = url.strip_prefix("ipfs://") else {
        return url.to_string();
    };
    let (cid_str, tail) = match rest.split_once('/') {
        Some((cid, tail)) => (cid, format!("/{tail}")),
        None => (rest, "/".to_string()),
    };
    let Ok(cid) = Cid::try_from(cid_str) else {
        return url.to_string();
    };
    let Ok(v1) = cid.into_v1() else {
        return url.to_string();
    };
    format!(
        "{INTERNAL_SCHEME}://{}{INTERNAL_IPFS_HOST_SUFFIX}{tail}",
        v1
    )
}

/// Map a URL the platform `WebView` reports (a load signal, an intercepted
/// request, a same-document history update) back to the core's real URL: an
/// internal-origin `https://<cid>.ipfs.werust.invalid[/path][?query][#fragment]`
/// becomes `ipfs://<cid>[/path][?query][#fragment]`. Any other URL is returned
/// UNCHANGED. Idempotent: an `ipfs://` URL passes through.
#[must_use]
pub fn from_webview_url(url: &str) -> String {
    let Some(rest) = url.strip_prefix("https://") else {
        return url.to_string();
    };
    let (host, tail) = match rest.split_once('/') {
        Some((host, tail)) => (host, format!("/{tail}")),
        None => (rest, String::new()),
    };
    let Some(label) = host.strip_suffix(INTERNAL_IPFS_HOST_SUFFIX) else {
        return url.to_string();
    };
    if label.is_empty() || label.contains('.') {
        return url.to_string();
    }
    // Chromium lowercases the host; the canonical base32 CIDv1 form is already
    // lowercase, so the label parses as-is. A label that does not parse as a
    // CID is not one of ours: pass the URL through unchanged (fail soft).
    let Ok(cid) = Cid::try_from(label) else {
        return url.to_string();
    };
    format!("ipfs://{cid}{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ronan.eth fixture root in BOTH its forms: the CIDv0 a user might
    /// type, and its canonical lowercase base32 CIDv1 (what the ENS
    /// contenthash decoder produces and what the internal origin carries).
    const CID_V0: &str = "QmUsRTSHzVrxGNGc3scGFapuFa3NCELA7T6x356YmDjf79";
    const CID_V1: &str = "bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq";

    #[test]
    fn an_ipfs_url_maps_to_the_internal_https_origin() {
        assert_eq!(
            to_webview_url(&format!("ipfs://{CID_V1}/")),
            format!("https://{CID_V1}{INTERNAL_IPFS_HOST_SUFFIX}/")
        );
        // Path + query (the SvelteKit `__data.json` fetch shape) are preserved.
        assert_eq!(
            to_webview_url(&format!(
                "ipfs://{CID_V1}/blog/__data.json?x-sveltekit-invalidated=01"
            )),
            format!(
                "https://{CID_V1}{INTERNAL_IPFS_HOST_SUFFIX}/blog/__data.json?x-sveltekit-invalidated=01"
            )
        );
        // A fragment is preserved too.
        assert_eq!(
            to_webview_url(&format!("ipfs://{CID_V1}/portfolio/#card")),
            format!("https://{CID_V1}{INTERNAL_IPFS_HOST_SUFFIX}/portfolio/#card")
        );
    }

    #[test]
    fn the_internal_origin_maps_back_to_the_real_ipfs_url() {
        assert_eq!(
            from_webview_url(&format!(
                "https://{CID_V1}{INTERNAL_IPFS_HOST_SUFFIX}/blog/__data.json?x-sveltekit-invalidated=01"
            )),
            format!("ipfs://{CID_V1}/blog/__data.json?x-sveltekit-invalidated=01")
        );
        // The bare origin (no trailing slash, how Chromium reports it) maps back too.
        assert_eq!(
            from_webview_url(&format!("https://{CID_V1}{INTERNAL_IPFS_HOST_SUFFIX}")),
            format!("ipfs://{CID_V1}")
        );
    }

    #[test]
    fn the_mapping_round_trips() {
        for url in [
            format!("ipfs://{CID_V1}/"),
            format!("ipfs://{CID_V1}/blog/"),
            format!("ipfs://{CID_V1}/blog/__data.json?x=1&y=2"),
        ] {
            assert_eq!(from_webview_url(&to_webview_url(&url)), url);
        }
    }

    #[test]
    fn a_cidv0_url_normalizes_to_the_canonical_base32_form() {
        // A hand-typed CIDv0 (`Qm...`, mixed case) cannot round-trip through a
        // hostname (Chromium lowercases it), so the map normalizes it to the
        // SAME content's canonical base32 CIDv1 form — the form the ENS
        // contenthash decoder already produces — and the round trip settles on
        // that canonical form.
        assert_eq!(
            to_webview_url(&format!("ipfs://{CID_V0}/blog/")),
            format!("https://{CID_V1}{INTERNAL_IPFS_HOST_SUFFIX}/blog/")
        );
        assert_eq!(
            from_webview_url(&to_webview_url(&format!("ipfs://{CID_V0}/blog/"))),
            format!("ipfs://{CID_V1}/blog/")
        );
    }

    #[test]
    fn a_chromium_lowercased_host_still_maps_back() {
        // Chromium reports hostnames LOWERCASED: an intercepted/reported URL's
        // host is the lowercase form. The canonical base32 CIDv1 is already
        // lowercase, so the reported URL maps back to the same CID.
        let reported = format!("https://{CID_V1}{INTERNAL_IPFS_HOST_SUFFIX}/blog/__data.json?x=1")
            .to_lowercase();
        assert_eq!(
            from_webview_url(&reported),
            format!("ipfs://{CID_V1}/blog/__data.json?x=1")
        );
    }

    #[test]
    fn urls_off_the_internal_origin_are_untouched_both_ways() {
        for url in [
            "https://example.com/",
            "https://example.com/blog/__data.json",
            "werust://settings",
            "ipns://k51qzi5uqu5di8vqwh6vd3md9td0s8ek6n5y6p7z5s7v8c5y5v5x5y5z5/",
        ] {
            assert_eq!(to_webview_url(url), url, "to_webview_url({url})");
            assert_eq!(from_webview_url(url), url, "from_webview_url({url})");
        }
        // An `ipfs://` URL is already the core form: from_webview_url is a no-op.
        let ipfs = format!("ipfs://{CID_V1}/blog/");
        assert_eq!(from_webview_url(&ipfs), ipfs);
        // A host that merely CONTAINS the suffix-looking text is not ours.
        let lookalike = format!("https://evil{INTERNAL_IPFS_HOST_SUFFIX}.example.com/x");
        assert_eq!(from_webview_url(&lookalike), lookalike);
    }

    #[test]
    fn an_unparseable_cid_is_left_alone_fail_soft() {
        // Not a CID at all: unchanged, the pre-existing handling keeps it.
        assert_eq!(to_webview_url("ipfs://not-a-cid/"), "ipfs://not-a-cid/");
        // A syntactically CID-looking but invalid label on the internal origin
        // is not one of ours: unchanged.
        let bogus = format!("https://zzz{INTERNAL_IPFS_HOST_SUFFIX}/x");
        assert_eq!(from_webview_url(&bogus), bogus);
    }
}
