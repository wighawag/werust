//! The `_redirects` + custom-404 fallback (IPIP-0002) driven END TO END over a
//! REAL content-addressed fixture site, network-isolated (task
//! `ipfs-web-redirects-and-404-fallback-support`, spec
//! `ens-to-ipfs-resolution-phase1-rpc-skeleton`).
//!
//! WHY AN INTEGRATION TEST WITH A REAL DAG: the load-bearing claim of this
//! feature is that a fallback page is served through the SAME hash-verified
//! `ipfs://` retrieval as any other resource — NO verification bypass. A
//! retriever double could not prove that, so this test synthesizes a real
//! multi-block UnixFS/dag-pb site (a `_redirects` file, an `/app/index.html`
//! SPA entry, a `404.html/index.html` custom error page — the jolly-roger.eth
//! shape) into a CARv1 stream and drives the PRODUCTION
//! [`TrustlessGatewayCarRetriever`] over a canned-CAR [`Fetcher`] double. Every
//! block the fallback serves is verified against its own CID by the same
//! `rs-car-sync` per-block check the normal path uses; there is no live network.
//!
//! The pure rule grammar/matching is unit-tested in `werust_core::redirects`;
//! the seam glue in `werust_core::ipfs`. This file is the end-to-end proof.

use std::collections::{BTreeMap, HashMap};

use cid::multihash::Multihash;
use fetcher::{Cid, FetchError, Fetcher, Response, TrustlessGatewayCarRetriever};
use renderer::SchemeRequest;
use werust_core::ipfs::{resolve_ipfs_request, RedirectSink, MAX_REDIRECT_HOPS};

const DAG_PB_CODEC: u64 = 0x70;
const SHA2_256: u64 = 0x12;

// ---------------------------------------------------------------------------
// Fixture builders: synthesize a REAL dag-pb/UnixFS DAG and CAR offline, binding
// the SAME vetted crates the production path decodes with (mirrors the builders
// in `fetcher`'s own retriever tests).
// ---------------------------------------------------------------------------

/// The CIDv1 (given codec, sha2-256) that addresses `bytes`.
fn cid_for(codec: u64, bytes: &[u8]) -> Cid {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mh = Multihash::<64>::wrap(SHA2_256, &digest).expect("sha2-256 multihash");
    Cid::new_v1(codec, mh)
}

/// Encode a UnixFS `Data` message (the inner payload of a dag-pb node).
fn unixfs_data(node_type: i32, data: Option<&[u8]>, filesize: u64) -> Vec<u8> {
    use quick_protobuf::Writer;
    let mut out = Vec::new();
    {
        let mut w = Writer::new(&mut out);
        w.write_with_tag(8, |w| w.write_enum(node_type)).unwrap();
        if let Some(d) = data {
            w.write_with_tag(18, |w| w.write_bytes(d)).unwrap();
        }
        if filesize > 0 {
            w.write_with_tag(24, |w| w.write_uint64(filesize)).unwrap();
        }
    }
    out
}

/// Encode a dag-pb node with the given inner `Data` and named links.
fn dagpb_node(data: Option<Vec<u8>>, links: &[(String, Cid)]) -> Vec<u8> {
    use ipld_core::ipld::Ipld;
    let mut node = BTreeMap::<String, Ipld>::new();
    if let Some(d) = data {
        node.insert("Data".into(), Ipld::Bytes(d));
    }
    let link_ipld: Vec<Ipld> = links
        .iter()
        .map(|(name, cid)| {
            let mut l = BTreeMap::<String, Ipld>::new();
            l.insert("Hash".into(), Ipld::Link(*cid));
            l.insert("Name".into(), Ipld::String(name.clone()));
            l.insert("Tsize".into(), Ipld::Integer(0));
            Ipld::Map(l)
        })
        .collect();
    node.insert("Links".into(), Ipld::List(link_ipld));
    ipld_dagpb::from_ipld(&Ipld::Map(node)).expect("encode dag-pb node")
}

/// A dag-pb UnixFS `File` leaf block holding `content` inline.
fn file_leaf(content: &[u8]) -> (Cid, Vec<u8>) {
    let data = unixfs_data(2 /* File */, Some(content), content.len() as u64);
    let block = dagpb_node(Some(data), &[]);
    (cid_for(DAG_PB_CODEC, &block), block)
}

/// A dag-pb UnixFS `Directory` node with named entries (links kept in the
/// name-sorted order dag-pb expects).
fn directory(entries: &[(String, Cid)]) -> (Cid, Vec<u8>) {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let data = unixfs_data(1 /* Directory */, None, 0);
    let block = dagpb_node(Some(data), &sorted);
    (cid_for(DAG_PB_CODEC, &block), block)
}

fn varint(n: u64) -> Vec<u8> {
    let mut buf = unsigned_varint::encode::u64_buffer();
    unsigned_varint::encode::u64(n, &mut buf).to_vec()
}

/// Frame `blocks` into a CARv1 byte stream rooted at `root`.
fn build_car(root: &Cid, blocks: &[(Cid, Vec<u8>)]) -> Vec<u8> {
    #[derive(serde::Serialize)]
    struct Header {
        roots: Vec<Cid>,
        version: u64,
    }
    let header = Header {
        roots: vec![*root],
        version: 1,
    };
    let header_bytes = serde_ipld_dagcbor::to_vec(&header).expect("encode car header");
    let mut out = Vec::new();
    out.extend(varint(header_bytes.len() as u64));
    out.extend(header_bytes);
    for (cid, block) in blocks {
        let cid_bytes = cid.to_bytes();
        let len = cid_bytes.len() + block.len();
        out.extend(varint(len as u64));
        out.extend(cid_bytes);
        out.extend(block);
    }
    out
}

/// A [`Fetcher`] double that answers EVERY gateway request with the same canned
/// whole-DAG CAR, isolated from the live network. The retriever still parses and
/// hash-verifies every block and resolves the path itself, so a path absent from
/// the DAG is a genuine `PathNotFound` — exactly the case the `_redirects`
/// fallback keys off.
struct CannedCarFetcher {
    car: Vec<u8>,
}

impl Fetcher for CannedCarFetcher {
    fn fetch(&self, url: &str) -> Result<Response, FetchError> {
        Ok(Response {
            status: 200,
            content_type: Some("application/vnd.ipld.car".into()),
            body: self.car.clone(),
            final_url: url.to_string(),
        })
    }
}

/// A content-addressed fixture site: its root CID plus a retriever serving its
/// (whole-DAG) CAR.
struct FixtureSite {
    root: Cid,
    retriever: TrustlessGatewayCarRetriever<CannedCarFetcher>,
    /// The `_redirects` 3xx hand-off the resolver pushes a navigation target
    /// into, exactly as a platform edge's scheme handler owns one. Shared across
    /// every [`get`](FixtureSite::get) on this site, so a redirect CHAIN is
    /// bounded the same way it is in production.
    redirects: RedirectSink,
}

impl FixtureSite {
    /// Resolve `path` under this site's root CID through the FULL production
    /// path (`resolve_ipfs_request` -> verified retrieval -> `_redirects`/404
    /// fallback).
    fn get(&self, path: &str) -> Result<renderer::SchemeResponse, renderer::RendererError> {
        self.get_url(&format!("ipfs://{root}{path}", root = self.root))
    }

    /// Resolve an ABSOLUTE `ipfs://` url through the same path, for following a
    /// redirect target the way the shell's navigation would (a fresh request,
    /// re-entering the handler, so the target is hash-verified by the SAME
    /// retrieval).
    ///
    /// This is a TOP-LEVEL document load, so it first reports `uri` to the sink
    /// exactly as `BrowserShell` does before every navigation it starts — that is
    /// what marks the intercepted request as the MAIN FRAME (only the main frame
    /// may redirect). Use [`get_sub_resource`](FixtureSite::get_sub_resource) for
    /// a request made BY the loaded page.
    fn get_url(&self, uri: &str) -> Result<renderer::SchemeResponse, renderer::RendererError> {
        self.redirects.note_navigation(uri);
        self.request(uri)
    }

    /// Resolve a SUB-RESOURCE of the page currently loaded (an image, a
    /// stylesheet, a script): the same intercepted-request path, but WITHOUT
    /// reporting a top-level navigation, so the resolver sees it for what it is.
    fn get_sub_resource(
        &self,
        path: &str,
    ) -> Result<renderer::SchemeResponse, renderer::RendererError> {
        self.request(&format!("ipfs://{root}{path}", root = self.root))
    }

    /// The raw intercepted request, with no navigation reported.
    fn request(&self, uri: &str) -> Result<renderer::SchemeResponse, renderer::RendererError> {
        resolve_ipfs_request(
            &self.retriever,
            &SchemeRequest {
                uri: uri.to_string(),
            },
            &self.redirects,
        )
    }

    /// The navigation the last [`get`](FixtureSite::get) queued, if it matched a
    /// 3xx rule (drained once, as the shell drains it).
    fn pending_redirect(&self) -> Option<String> {
        self.redirects.take_pending()
    }
}

/// Build a fixture site from `(path, contents)` files, wiring the whole DAG into
/// one canned CAR. Only one directory level of nesting is used (enough for the
/// `app/` + `404.html/` shapes this fixture needs).
fn site(files: &[(&str, &[u8])]) -> FixtureSite {
    let mut blocks: Vec<(Cid, Vec<u8>)> = Vec::new();
    // dir path ("" = root) -> entries
    let mut dirs: HashMap<String, Vec<(String, Cid)>> = HashMap::new();
    dirs.insert(String::new(), Vec::new());

    for (path, content) in files {
        let (leaf_cid, leaf_block) = file_leaf(content);
        blocks.push((leaf_cid, leaf_block));
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        match segments.as_slice() {
            [name] => dirs
                .get_mut("")
                .expect("root dir")
                .push(((*name).to_string(), leaf_cid)),
            [dir, name] => dirs
                .entry((*dir).to_string())
                .or_default()
                .push(((*name).to_string(), leaf_cid)),
            other => panic!("fixture supports at most one nested directory, got {other:?}"),
        }
    }

    // Build each sub-directory, then link it into the root.
    let sub_dirs: Vec<String> = dirs.keys().filter(|k| !k.is_empty()).cloned().collect();
    for dir in sub_dirs {
        let entries = dirs.remove(&dir).expect("sub dir entries");
        let (dir_cid, dir_block) = directory(&entries);
        blocks.push((dir_cid, dir_block));
        dirs.get_mut("").expect("root dir").push((dir, dir_cid));
    }

    let root_entries = dirs.remove("").expect("root dir");
    let (root_cid, root_block) = directory(&root_entries);
    blocks.push((root_cid, root_block));

    let car = build_car(&root_cid, &blocks);
    FixtureSite {
        root: root_cid,
        retriever: TrustlessGatewayCarRetriever::with_gateway(
            CannedCarFetcher { car },
            "http://gateway.test",
        ),
        redirects: RedirectSink::new(),
    }
}

const INDEX_HTML: &[u8] = b"<!doctype html><title>fixture</title><h1>home</h1>";
const APP_HTML: &[u8] = b"<!doctype html><title>app</title><div id=app></div>";
const NOT_FOUND_HTML: &[u8] = b"<!doctype html><title>404</title><h1>arr, nothing here</h1>";

/// The jolly-roger.eth shape: a root `_redirects` whose catch-all serves the
/// custom 404 page at `404.html/index.html`, plus an SPA 200-rewrite rule.
fn jolly_roger_shaped_site() -> FixtureSite {
    site(&[
        ("index.html", INDEX_HTML),
        ("app/index.html", APP_HTML),
        ("404.html/index.html", NOT_FOUND_HTML),
        (
            "_redirects",
            b"/app/* /app/index.html 200\n/* /404.html/index.html 404\n",
        ),
    ])
}

#[test]
fn a_not_found_path_serves_the_sites_custom_404_page_with_a_not_found_status() {
    // ACCEPTANCE (the field finding): `jolly-roger.eth/unknown` must serve the
    // site's own 404 page — the content of `404.html/index.html` named by its
    // root `_redirects` (`/* /404.html/index.html 404`) — with a NOT-FOUND
    // status, instead of werust's hard error. Every byte still hash-verified
    // through the same content-addressed retrieval.
    let site = jolly_roger_shaped_site();

    let response = site
        .get("/unknown")
        .expect("a not-found path resolves through the site's _redirects rules");
    assert_eq!(
        response.body, NOT_FOUND_HTML,
        "the site's custom 404 page is served"
    );
    assert_eq!(
        response.status, 404,
        "and it is served with a not-found status, not a 200"
    );
    assert_eq!(
        response.mime_type, "text/html",
        "the mime comes from the served TARGET, so the 404 page renders as a page"
    );
}

#[test]
fn an_existing_path_is_never_intercepted_by_the_catch_all_rule() {
    // IPIP-0002 §3.3 (no forced redirects): the rules are evaluated ONLY when the
    // requested path is absent from the DAG. A page that exists is served as is,
    // with a 200, even though a `/*` catch-all rule matches its path.
    let site = jolly_roger_shaped_site();

    let home = site.get("/").expect("the root page still resolves");
    assert_eq!(home.body, INDEX_HTML);
    assert_eq!(home.status, 200);

    let app = site
        .get("/app/index.html")
        .expect("an existing nested page still resolves");
    assert_eq!(app.body, APP_HTML);
    assert_eq!(app.status, 200);
}

#[test]
fn a_200_rule_rewrites_the_spa_entry_point_without_changing_the_bar() {
    // ACCEPTANCE: a `200` rule is a REWRITE — the target's content is served AT
    // THE REQUESTED URL. Nothing navigates (the resolver answers the intercepted
    // request in place and emits no navigation), so the URL bar is untouched:
    // exactly the SPA/PWA deep-link case (`/app/* /app/index.html 200`).
    let site = jolly_roger_shaped_site();

    let response = site
        .get("/app/some/client-route")
        .expect("an unknown route under /app/ rewrites to the app entry point");
    assert_eq!(response.body, APP_HTML, "the SPA entry point is served");
    assert_eq!(
        response.status, 200,
        "a rewrite is an OK response for the requested url, not an error"
    );
}

#[test]
fn the_first_matching_rule_wins() {
    // IPIP-0002 §3.2: rules are evaluated top to bottom, first match wins. The
    // `/app/*` rewrite precedes the `/*` catch-all, so an unknown `/app/…` path
    // gets the 200 rewrite, NOT the 404 page.
    let site = jolly_roger_shaped_site();
    let response = site.get("/app/deep/link").expect("first match wins");
    assert_eq!(response.body, APP_HTML);
    assert_eq!(response.status, 200);
}

#[test]
fn a_site_with_no_redirects_and_no_404_page_still_hard_not_founds() {
    // ACCEPTANCE (opt-in per site): a site that ships neither a `_redirects` nor
    // a root `404.html` is COMPLETELY unchanged — a missing path is still
    // werust's honest fail-closed not-found, never a guessed page.
    let site = site(&[("index.html", INDEX_HTML)]);

    let err = site
        .get("/unknown")
        .expect_err("a site without the opt-in files keeps the hard not-found");
    let renderer::RendererError::Backend(reason) = err else {
        panic!("expected a fail-closed backend error");
    };
    assert!(
        reason.contains("path not found"),
        "the honest not-found reason is preserved, got: {reason}"
    );
}

#[test]
fn a_root_404_html_is_honoured_even_without_a_redirects_file() {
    // The DEFAULT custom-404 convention: a site with a root `404.html` but NO
    // `_redirects` still serves that page (with a not-found status) for an
    // unknown path, exactly as a gateway does.
    let site = site(&[("index.html", INDEX_HTML), ("404.html", NOT_FOUND_HTML)]);

    let response = site.get("/nope").expect("the default 404.html is honoured");
    assert_eq!(response.body, NOT_FOUND_HTML);
    assert_eq!(response.status, 404);
}

#[test]
fn a_redirects_target_that_does_not_exist_fails_closed() {
    // ACCEPTANCE (fail-closed, no bypass): a `_redirects` whose `to` names a
    // resource that is NOT in the DAG is itself a not-found. The fallback never
    // invents content and never recurses back into the rules.
    let site = site(&[
        ("index.html", INDEX_HTML),
        ("_redirects", b"/* /missing-404.html 404\n"),
    ]);

    let err = site
        .get("/unknown")
        .expect_err("a missing fallback target must fail closed");
    let renderer::RendererError::Backend(reason) = err else {
        panic!("expected a fail-closed backend error");
    };
    assert!(
        reason.contains("_redirects") && reason.contains("missing-404.html"),
        "the failure names the unresolvable target, got: {reason}"
    );
}

#[test]
fn an_off_root_target_is_rejected_so_one_site_cannot_impersonate_another() {
    // ACCEPTANCE (unique-origin security): `_redirects` is a PER-SITE capability.
    // A `to` that leaves the root CID (an absolute URL, a protocol-relative
    // authority, or a `..` escape) is REJECTED — a site's rules can only ever
    // name content under its OWN root CID, which is what keeps a `_redirects`
    // from making one content root impersonate another.
    for off_root in [
        "https://evil.example/404.html",
        "ipfs://bafyotherroot/404.html",
        "//evil.example/404.html",
        "/../bafyotherroot/404.html",
    ] {
        let rules = format!("/* {off_root} 404\n");
        let site = site(&[("index.html", INDEX_HTML), ("_redirects", rules.as_bytes())]);
        let err = site
            .get("/unknown")
            .expect_err("an off-root target must be rejected");
        let renderer::RendererError::Backend(reason) = err else {
            panic!("expected a fail-closed backend error");
        };
        assert!(
            reason.contains("root cid"),
            "the refusal names the unique-origin rule for `{off_root}`, got: {reason}"
        );
    }
}

// ---------------------------------------------------------------------------
// The 3xx NAVIGATION (task `ipfs-redirects-3xx-navigation-support`), driven over
// the SAME real DAG so the redirected target's hash verification is proven, not
// asserted.
// ---------------------------------------------------------------------------

#[test]
fn a_3xx_rule_navigates_to_the_target_and_the_target_is_hash_verified_by_the_same_retrieval() {
    // ACCEPTANCE: a matching 3xx NAVIGATES. Nothing is served for the
    // redirected-FROM url (the shell must move the bar, not silently render the
    // new page under the old address); the absolute `ipfs://<rootcid>/<to>` target
    // is handed to the shell, and FOLLOWING it (a fresh request, exactly as the
    // shell's navigation does) resolves the target's bytes through the SAME
    // per-block-verified retrieval as any other resource.
    let site = site(&[
        ("index.html", INDEX_HTML),
        ("new.html", APP_HTML),
        ("_redirects", b"/old /new.html 301\n"),
    ]);

    let err = site
        .get("/old")
        .expect_err("a redirect renders nothing under the OLD url");
    let renderer::RendererError::Backend(reason) = err else {
        panic!("expected a fail-closed backend error");
    };
    assert!(
        reason.contains("301") && reason.contains("/new.html"),
        "the redirect is legible in the reason, got: {reason}"
    );

    let target = site
        .pending_redirect()
        .expect("a matching 3xx queues a navigation");
    assert_eq!(
        target,
        format!("ipfs://{root}/new.html", root = site.root),
        "the navigation target is absolute, under the site's OWN root cid"
    );

    // The shell's navigation: a fresh request for the target, re-entering the
    // resolver, so the redirected page is verified by the SAME retrieval.
    let response = site
        .get_url(&target)
        .expect("the redirect target resolves through the verified path");
    assert_eq!(response.body, APP_HTML);
    assert_eq!(
        response.status, 200,
        "the redirected page is an ordinary OK page at its own url"
    );
}

#[test]
fn a_3xx_target_injects_the_splat_and_may_not_leave_the_root_cid() {
    // ACCEPTANCE: placeholder/`:splat` injection works for a 3xx exactly as it
    // does for 200/404, and the same-root confinement still holds for a
    // navigation (the load-bearing case: an off-root target would navigate the
    // browser to another content root on a site's own say-so).
    let splat_site = site(&[
        ("index.html", INDEX_HTML),
        ("new/deep.html", APP_HTML),
        ("_redirects", b"/old/* /new/:splat 302\n"),
    ]);
    let _ = splat_site.get("/old/deep.html");
    assert_eq!(
        splat_site.pending_redirect(),
        Some(format!(
            "ipfs://{root}/new/deep.html",
            root = splat_site.root
        )),
        "the captured splat is injected into the navigation target"
    );

    for off_root in [
        "https://evil.example/landing",
        "ipfs://bafyotherroot/landing",
        "//evil.example/landing",
        "/../bafyotherroot/landing",
    ] {
        let rules = format!("/* {off_root} 301\n");
        let site = site(&[("index.html", INDEX_HTML), ("_redirects", rules.as_bytes())]);
        let err = site
            .get("/unknown")
            .expect_err("an off-root redirect must be rejected");
        let renderer::RendererError::Backend(reason) = err else {
            panic!("expected a fail-closed backend error");
        };
        assert!(
            reason.contains("root cid"),
            "the refusal names the unique-origin rule for `{off_root}`, got: {reason}"
        );
        assert_eq!(
            site.pending_redirect(),
            None,
            "an off-root target is NEVER queued as a navigation"
        );
    }
}

#[test]
fn a_3xx_matched_by_a_sub_resource_of_the_page_never_navigates_the_page_away() {
    // ACCEPTANCE (the main-frame rule, over a real DAG): the scheme handler
    // answers the main document AND every sub-resource of it. A page with a stale
    // `<img src="/old/logo.png">` whose path matches a 3xx rule must keep
    // rendering: the sub-resource fails closed (or gets the site's 404 page) and
    // queues NOTHING, so the browser stays on the page the user is reading.
    let site = site(&[
        ("index.html", INDEX_HTML),
        ("404.html", NOT_FOUND_HTML),
        ("new/deep.html", APP_HTML),
        ("_redirects", b"/old/* /new/:splat 301\n"),
    ]);

    // The page the user is reading (the top-level document).
    let home = site
        .get("/index.html")
        .expect("the page itself resolves normally");
    assert_eq!(home.body, INDEX_HTML);

    // Its stale sub-resource, whose path matches the 3xx rule.
    let logo = site
        .get_sub_resource("/old/logo.png")
        .expect("a matched sub-resource falls through to the site's 404 page");
    assert_eq!(
        logo.status, 404,
        "a sub-resource is answered, not navigated"
    );
    assert_eq!(
        site.pending_redirect(),
        None,
        "a sub-resource must NEVER yank the top-level page onto the rewritten path"
    );

    // The MAIN-FRAME request for the same shape still redirects: excluding
    // sub-resources must not break the feature.
    let _ = site.get("/old/deep.html");
    assert_eq!(
        site.pending_redirect(),
        Some(format!("ipfs://{root}/new/deep.html", root = site.root)),
        "the top-level document still redirects"
    );
}

#[test]
fn a_redirect_cycle_over_a_real_dag_is_bounded_and_fails_closed() {
    // ACCEPTANCE (the loop guard): a `_redirects` whose targets redirect again
    // must not loop unboundedly. Each hop is a fresh navigation, so the bound is
    // the shared sink's: a cycle (or an over-long chain) is refused, the chain
    // stops, and the last thing the user sees is a legible fail-closed error.
    let site = site(&[
        ("index.html", INDEX_HTML),
        ("_redirects", b"/a /b 301\n/b /a 301\n"),
    ]);

    let mut next = format!("ipfs://{root}/a", root = site.root);
    let mut hops = 0usize;
    let reason = loop {
        let err = site
            .get_url(&next)
            .expect_err("a redirect renders nothing in place");
        let renderer::RendererError::Backend(reason) = err else {
            panic!("expected a fail-closed backend error");
        };
        match site.pending_redirect() {
            Some(target) => {
                hops += 1;
                assert!(
                    hops <= MAX_REDIRECT_HOPS,
                    "the chain must stop at {MAX_REDIRECT_HOPS} hops, walked {hops}"
                );
                next = target;
            }
            None => break reason,
        }
    };
    assert!(
        reason.contains("cycle") || reason.contains("hop limit"),
        "the bounded chain says WHY it stopped, got: {reason}"
    );
}

#[test]
fn the_fallback_content_is_hash_verified_through_the_same_retrieval() {
    // ACCEPTANCE (NO verification bypass): the fallback target is fetched through
    // the SAME content-addressed retrieval as any other resource, so a TAMPERED
    // 404 page (bytes that do not hash to the CID the DAG links) fails the load
    // instead of rendering. This is the whole reason the fallback is resolved by
    // path under the root CID rather than by handing bytes around.
    let honest_404 = NOT_FOUND_HTML;
    let (real_404_cid, _honest_block) = file_leaf(honest_404);
    // A block that does NOT hash to `real_404_cid`, framed under it.
    let tampered_block = dagpb_node(
        Some(unixfs_data(2, Some(b"<h1>tampered 404</h1>"), 21)),
        &[],
    );

    let (index_cid, index_block) = file_leaf(INDEX_HTML);
    let redirects = b"/* /404.html 404\n";
    let (redirects_cid, redirects_block) = file_leaf(redirects);
    let (root_cid, root_block) = directory(&[
        ("404.html".into(), real_404_cid),
        ("_redirects".into(), redirects_cid),
        ("index.html".into(), index_cid),
    ]);
    let car = build_car(
        &root_cid,
        &[
            (root_cid, root_block),
            (index_cid, index_block),
            (redirects_cid, redirects_block),
            // The tampered bytes under the honest 404 page's CID.
            (real_404_cid, tampered_block),
        ],
    );
    let site = FixtureSite {
        root: root_cid,
        retriever: TrustlessGatewayCarRetriever::with_gateway(
            CannedCarFetcher { car },
            "http://gateway.test",
        ),
        redirects: RedirectSink::new(),
    };

    let err = site
        .get("/unknown")
        .expect_err("a tampered fallback page must fail the load, never render");
    let renderer::RendererError::Backend(reason) = err else {
        panic!("expected a fail-closed backend error");
    };
    assert!(
        reason.contains("mismatch"),
        "the fallback is hash-verified like any resource, got: {reason}"
    );
}

#[test]
fn a_broken_redirects_file_fails_the_load_rather_than_serving_the_wrong_page() {
    // IPIP-0002 §3.4: an unparseable `_redirects` is an ERROR, not something to
    // ignore — ignoring it would serve a different page than the site's author
    // wrote (or a hard not-found where they wrote a fallback).
    let site = site(&[
        ("index.html", INDEX_HTML),
        ("404.html", NOT_FOUND_HTML),
        ("_redirects", b"/* /404.html 999\n"),
    ]);

    let err = site
        .get("/unknown")
        .expect_err("a malformed _redirects must fail closed");
    let renderer::RendererError::Backend(reason) = err else {
        panic!("expected a fail-closed backend error");
    };
    assert!(
        reason.contains("_redirects"),
        "the failure names the broken redirects file, got: {reason}"
    );
}

#[test]
fn a_missing_sub_resource_under_a_404_site_still_serves_the_sites_404_page() {
    // The fallback is per-PATH, not per-page: any not-found path under the site
    // (an asset, a data file) is answered by the site's own rules, exactly as a
    // gateway answers it. The status stays 404 so the page/loader can tell.
    let site = jolly_roger_shaped_site();
    let response = site
        .get("/assets/missing.js")
        .expect("a missing sub-resource is answered by the site's rules");
    assert_eq!(response.body, NOT_FOUND_HTML);
    assert_eq!(response.status, 404);
}

// ---------------------------------------------------------------------------
// The PER-RESOURCE-SCOPED gateway shape (`dag-scope=entity`), which is what the
// production backend actually talks to.
// ---------------------------------------------------------------------------

/// A [`Fetcher`] double modelling a REAL trustless gateway under
/// `dag-scope=entity`: a distinct CAR per requested path, and for a path that is
/// NOT in the DAG the blocks it managed to traverse (the root listing) with a
/// 200 — checked against dweb.link / trustless-gateway.link / ipfs.io, which all
/// answer `/ipfs/<cid>/does-not-exist?format=car&dag-scope=entity` that way. So
/// the not-found is decided LOCALLY by werust's own verified walk
/// (`PathNotFound`), never taken on the gateway's word. Network-isolated.
///
/// A gateway that instead answers an absent path with an HTTP error is covered
/// by [`Http404GatewayFetcher`] below.
struct ScopedGatewayFetcher {
    /// Keyed by URL path (`/ipfs/<cid>[/<sub>]`), the CAR that path returns.
    cars: HashMap<String, Vec<u8>>,
    /// The traversal-blocks CAR returned for any other (absent) path.
    traversal_only: Vec<u8>,
}

impl Fetcher for ScopedGatewayFetcher {
    fn fetch(&self, url: &str) -> Result<Response, FetchError> {
        let no_query = url.split('?').next().unwrap_or(url);
        let key = match no_query.find("/ipfs/") {
            Some(i) => no_query[i..].to_string(),
            None => no_query.to_string(),
        };
        Ok(Response {
            status: 200,
            content_type: Some("application/vnd.ipld.car".into()),
            body: self
                .cars
                .get(&key)
                .cloned()
                .unwrap_or_else(|| self.traversal_only.clone()),
            final_url: url.to_string(),
        })
    }
}

/// A [`Fetcher`] double for the OTHER gateway behaviour on an absent path:
/// answering the scoped request with an HTTP error instead of the traversal
/// blocks. The requested resource path still gets the traversal blocks (so
/// werust's own walk decides the not-found), but the OPTIONAL fallback probes
/// (`/_redirects`, `/404.html`) are HTTP-404'd.
struct Http404ProbeGatewayFetcher {
    cars: HashMap<String, Vec<u8>>,
    traversal_only: Vec<u8>,
}

impl Fetcher for Http404ProbeGatewayFetcher {
    fn fetch(&self, url: &str) -> Result<Response, FetchError> {
        let no_query = url.split('?').next().unwrap_or(url);
        let key = match no_query.find("/ipfs/") {
            Some(i) => no_query[i..].to_string(),
            None => no_query.to_string(),
        };
        if key.ends_with("/_redirects") || key.ends_with("/404.html") {
            return Ok(Response {
                status: 404,
                content_type: Some("text/plain".into()),
                body: Vec::new(),
                final_url: url.to_string(),
            });
        }
        Ok(Response {
            status: 200,
            content_type: Some("application/vnd.ipld.car".into()),
            body: self
                .cars
                .get(&key)
                .cloned()
                .unwrap_or_else(|| self.traversal_only.clone()),
            final_url: url.to_string(),
        })
    }
}

/// The scoped CARs for a one-level site: the root listing, plus one entity CAR
/// per root file. Returns `(root cid, cars, traversal-only car)`.
fn scoped_cars(files: &[(&str, &[u8])]) -> (Cid, HashMap<String, Vec<u8>>, Vec<u8>) {
    let leaves: Vec<(String, Cid, Vec<u8>)> = files
        .iter()
        .map(|(name, content)| {
            let (cid, block) = file_leaf(content);
            ((*name).to_string(), cid, block)
        })
        .collect();
    let entries: Vec<(String, Cid)> = leaves
        .iter()
        .map(|(name, cid, _)| (name.clone(), *cid))
        .collect();
    let (root_cid, root_block) = directory(&entries);

    let traversal_only = build_car(&root_cid, &[(root_cid, root_block.clone())]);
    let mut cars = HashMap::new();
    cars.insert(format!("/ipfs/{root_cid}"), traversal_only.clone());
    for (name, cid, block) in leaves {
        cars.insert(
            format!("/ipfs/{root_cid}/{name}"),
            build_car(&root_cid, &[(root_cid, root_block.clone()), (cid, block)]),
        );
    }
    (root_cid, cars, traversal_only)
}

#[test]
fn a_scoped_gateway_site_serves_its_custom_404_page() {
    // The jolly-roger case over the shape the production backend really sees:
    // each fallback file (`_redirects`, then the target) is its OWN
    // `dag-scope=entity` fetch, and every block is hash-verified by the
    // production retriever before anything is served.
    let (root_cid, cars, traversal_only) = scoped_cars(&[
        ("404.html", NOT_FOUND_HTML),
        ("_redirects", b"/* /404.html 404\n"),
        ("index.html", INDEX_HTML),
    ]);
    let retriever = TrustlessGatewayCarRetriever::with_gateway(
        ScopedGatewayFetcher {
            cars,
            traversal_only,
        },
        "http://gw.test",
    );

    let response = resolve_ipfs_request(
        &retriever,
        &SchemeRequest {
            uri: format!("ipfs://{root_cid}/unknown"),
        },
        &RedirectSink::new(),
    )
    .expect("the site's custom 404 page is served over the scoped gateway shape");
    assert_eq!(response.body, NOT_FOUND_HTML);
    assert_eq!(response.status, 404);
}

#[test]
fn a_scoped_gateway_site_with_no_redirects_keeps_its_honest_not_found() {
    // The opt-in promise on that same shape: a site shipping neither file still
    // gets werust's ORIGINAL fail-closed not-found, naming the path the user
    // actually asked for (not the `_redirects` probe).
    let (root_cid, cars, traversal_only) = scoped_cars(&[("index.html", INDEX_HTML)]);
    let retriever = TrustlessGatewayCarRetriever::with_gateway(
        ScopedGatewayFetcher {
            cars,
            traversal_only,
        },
        "http://gw.test",
    );

    let err = resolve_ipfs_request(
        &retriever,
        &SchemeRequest {
            uri: format!("ipfs://{root_cid}/unknown"),
        },
        &RedirectSink::new(),
    )
    .expect_err("a site that opted into nothing keeps its hard not-found");
    let renderer::RendererError::Backend(reason) = err else {
        panic!("expected a fail-closed backend error");
    };
    assert!(
        reason.contains("path not found") && reason.contains("/unknown"),
        "the ORIGINAL not-found reason survives the scoped probes, got: {reason}"
    );
}

#[test]
fn a_gateway_that_http_404s_the_optional_probes_is_tolerated_as_absence() {
    // Gateways differ on how they answer a scoped request for a path that is not
    // in the DAG: some return the traversal blocks (above), others an HTTP error.
    // On the OPTIONAL probes (`_redirects` / `404.html`) both must read as "the
    // site does not ship it", so a site that opted into nothing keeps its ORIGINAL
    // honest not-found instead of surfacing a confusing gateway-transport failure.
    // (Tolerating this on a probe can never yield content: the worst case is the
    // pre-existing not-found. A target a rule actually NAMED is not affected —
    // that path fails the load.)
    let (root_cid, cars, traversal_only) = scoped_cars(&[("index.html", INDEX_HTML)]);
    let retriever = TrustlessGatewayCarRetriever::with_gateway(
        Http404ProbeGatewayFetcher {
            cars,
            traversal_only,
        },
        "http://gw.test",
    );

    let err = resolve_ipfs_request(
        &retriever,
        &SchemeRequest {
            uri: format!("ipfs://{root_cid}/unknown"),
        },
        &RedirectSink::new(),
    )
    .expect_err("a site that opted into nothing keeps its hard not-found");
    let renderer::RendererError::Backend(reason) = err else {
        panic!("expected a fail-closed backend error");
    };
    assert!(
        reason.contains("path not found") && reason.contains("/unknown"),
        "an http-404ing gateway must not turn the probes into a hard failure, got: {reason}"
    );
}
