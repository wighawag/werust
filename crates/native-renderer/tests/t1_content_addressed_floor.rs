//! T1 **content-addressed floor**: a real `ipfs://` static site (a Jekyll/Hugo-
//! class docs/landing page pinned to a CID) rendered by the native T1 path at
//! parity with the server path.
//!
//! This is the objective guard for the conformance ladder's **T1 content-addressed
//! floor** (`docs/conformance-tiers.md` T1; user story 16 of the ship spec, task
//! `t1-content-addressed-floor-ipfs-static-site`). This is where the thesis lands
//! FIRST: a verifiable, content-addressed static document opened as a first-class
//! page, not a novelty. T1 is only "reached" once BOTH floors land — the
//! [server floor](t1_server_floor_goldens) (real hand-authored pages served
//! straight to the native path) AND this content-addressed floor (a real static
//! site fetched over the hash-verified `ipfs://` path and rendered at parity).
//! This test proves the second half at T1.
//!
//! It reuses, rather than reinvents, the two seams the blocking tasks landed
//! (`t1-core-css-stylo-and-latin-shaping-parley` +
//! `ipfs-scheme-resolution-through-renderer-seam`):
//!
//! * the **verifiable content-retrieval seam**: the pinned site's bytes derive
//!   their fixture CID ([`fetcher::cid_v1_raw_sha256`], a single-block raw
//!   `sha2-256` CID), and are resolved through
//!   [`werust_core::ipfs::resolve_ipfs_request`] over a
//!   [`ContentRetriever`](fetcher::ContentRetriever), which returns bytes ONLY
//!   after each block hashes to its CID (the full multi-block CAR/DAG verify is
//!   covered in the `fetcher::retriever` tests);
//! * the **native T1 render path**: the resolved, verified bytes are rendered by
//!   the SAME [`NativeRenderer`] backend, driven THROUGH the [`Renderer`] seam at
//!   the SAME pinned viewport, that the server floor uses.
//!
//! Parity is asserted TWO ways so "the content-addressed path is not a
//! second-class renderer" is objective, not asserted by hand:
//!
//! 1. **Against the SERVER path of the same bytes.** The exact fixture bytes are
//!    rendered directly (as a served `data:text/html` document) AND through the
//!    verified `ipfs://` path; the two painted transcripts must be byte-for-byte
//!    IDENTICAL. The ONLY variable between them is WHERE the bytes came from, which
//!    is the whole meaning of "parity".
//! 2. **Against a committed golden.** The transcript is also asserted equal to the
//!    committed `site.golden.txt`, so a regression anywhere in parse / cascade /
//!    shaping / layout / paint turns the golden red under the `verify` gate.
//!
//! Everything is off the live network: the content source is an in-memory,
//! per-test map, and the CID is DERIVED from the fixture bytes so it verifies
//! deterministically. Shaping is reproducible because it is pinned to the crate's
//! one bundled font (`assets/DejaVuSans.ttf`); the golden is stable ONLY against
//! that font.
//!
//! When an INTENDED render change shifts the golden, regenerate with the ignored
//! helper [`regenerate_goldens`] and review the diff — a golden change is a
//! rendering change.

use std::path::{Path, PathBuf};

use fetcher::{cid_v1_raw_sha256, ContentRetriever, RetrieveError, RetrievedContent};
use native_renderer::css::Color;
use native_renderer::{NativeRenderer, RenderOutput};
use renderer::{LoadState, Renderer, RendererError, SchemeRequest};
use werust_core::ipfs::{resolve_ipfs_request, RedirectSink};

/// The `raw` IPLD multicodec code (a leaf block's bytes ARE the content).
const RAW_CODEC: u64 = 0x55;

/// The viewport width the golden is pinned at, in px. Identical to the server
/// floor's ([`t1_server_floor_goldens`]) so the transcript (and therefore the
/// parity comparison) is against the exact same class of reference.
const FIXTURE_VIEWPORT_WIDTH: f32 = 800.0;

/// The committed fixture name: a real Jekyll/Hugo-class static site (a single
/// self-contained docs/landing page), pinned as a local snapshot and — in the
/// content-addressed floor — pinned to a CID derived from its bytes.
const FIXTURE: &str = "site";

/// Absolute path to the fixtures directory (committed beside this test).
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/t1-content-addressed-floor")
}

/// Read the pinned static-site snapshot `<name>.html`.
fn read_fixture_html(name: &str) -> String {
    let path = fixtures_dir().join(format!("{name}.html"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()))
}

/// The committed golden path for `name`.
fn golden_path(name: &str) -> PathBuf {
    fixtures_dir().join(format!("{name}.golden.txt"))
}

/// A pinned, in-memory [`ContentRetriever`], isolated from the live network,
/// that verifies a single raw/leaf block against its CID.
///
/// It plays the untrusted-origin role: it holds bytes for a CID and RE-VERIFIES
/// them against that CID before returning, so it can be pointed at honest
/// content (stored under its real CID) or TAMPERED content (bytes that do not
/// match the CID) to exercise both the render-verified and
/// mismatch-fails-the-load cases, with NO network access. The full multi-block
/// CAR/DAG verify is covered in the `fetcher::retriever` tests; this floor
/// pins a single raw block.
#[derive(Default)]
struct PinnedRawRetriever {
    blobs: std::collections::HashMap<String, Vec<u8>>,
}

impl PinnedRawRetriever {
    /// Store honest content under its real (derived) raw CID and return that CID:
    /// retrieving this CID must verify and return these exact bytes.
    fn put(&mut self, bytes: &[u8]) -> String {
        let cid = cid_v1_raw_sha256(bytes).expect("derive pinned fixture cid");
        self.blobs.insert(cid.clone(), bytes.to_vec());
        cid
    }

    /// Store TAMPERED bytes under `cid`: the bytes do NOT hash to `cid`, so a
    /// retrieve of `cid` must fail the load with a hash mismatch, never render.
    fn put_tampered_under(&mut self, cid: &str, tampered: &[u8]) {
        self.blobs.insert(cid.to_string(), tampered.to_vec());
    }
}

impl ContentRetriever for PinnedRawRetriever {
    fn retrieve(&self, cid: &str, _path: &str) -> Result<RetrievedContent, RetrieveError> {
        let bytes = self
            .blobs
            .get(cid)
            .cloned()
            .ok_or_else(|| RetrieveError::MissingBlock {
                cid: cid.to_string(),
            })?;
        // Re-verify against the CID: a raw block's bytes ARE the content, so a
        // mismatch is a hard tamper failure that is never returned.
        let expected = cid_v1_raw_sha256(&bytes).expect("derive cid for held bytes");
        if expected != cid {
            return Err(RetrieveError::BlockHashMismatch {
                cid: cid.to_string(),
            });
        }
        Ok(RetrievedContent {
            bytes,
            codec: RAW_CODEC,
        })
    }
}

/// Build a `data:text/html,…` URL for `html`, percent-encoding exactly the bytes
/// the native backend's decoder treats specially (`%`, `+`) plus spaces, so the
/// bytes reach the native path through the seam byte-for-byte intact and NO
/// network fetch is involved. Identical to the server floor's encoding.
fn data_url(html: &str) -> String {
    let mut payload = String::new();
    for b in html.bytes() {
        match b {
            b'%' => payload.push_str("%25"),
            b'+' => payload.push_str("%2B"),
            b' ' => payload.push_str("%20"),
            _ => payload.push(b as char),
        }
    }
    format!("data:text/html,{payload}")
}

/// Render `html` bytes through the native T1 path, driven THROUGH the [`Renderer`]
/// seam at the pinned viewport, and return the whole [`RenderOutput`].
///
/// This is the SAME render step the server floor uses; the ONLY difference the
/// content-addressed floor introduces is WHERE the bytes came from (the
/// hash-verified `ipfs://` path), not how they are rendered — which is the whole
/// point of "parity".
fn render_bytes(html: &str) -> RenderOutput {
    let mut backend = NativeRenderer::with_viewport_width(FIXTURE_VIEWPORT_WIDTH);
    {
        let seam: &mut dyn Renderer = &mut backend;
        seam.navigate(&data_url(html))
            .expect("the static site is navigable via the native T1 path");
        assert_eq!(
            seam.load_state(),
            LoadState::Finished,
            "the content-addressed site finished loading"
        );
    }
    backend.last_render().expect("a render happened").clone()
}

/// The painted software-text transcript for `html` (the golden reference form).
fn render_bytes_transcript(html: &str) -> String {
    render_bytes(html).surface.transcript()
}

/// Resolve `cid` through the hash-verified content-addressed `ipfs://` path,
/// returning the VERIFIED bytes (as UTF-8), or panicking if the load would fail.
///
/// This drives the exact seam the `ipfs://` scheme handler drives in production:
/// [`resolve_ipfs_request`] over a [`ContentRetriever`], which returns bytes
/// ONLY after each block hashes to its CID. So the site is provably hash-verified
/// on the way in before it is ever rendered.
fn resolve_verified_html(retriever: &PinnedRawRetriever, cid: &str) -> String {
    let response = resolve_ipfs_request(
        retriever,
        &SchemeRequest {
            uri: format!("ipfs://{cid}/index.html"),
        },
        &RedirectSink::new(),
    )
    .expect("verified content-addressed bytes resolve to render");
    // The content-addressed floor renders a page: the resolver infers text/html
    // for served-page parity.
    assert_eq!(response.mime_type, "text/html");
    String::from_utf8(response.body).expect("the fixture site is valid utf-8")
}

#[test]
fn content_addressed_site_renders_at_parity_with_the_server_path() {
    // The heart of the T1 content-addressed floor: a real Jekyll/Hugo-class static
    // site, fetched over the hash-verified `ipfs://` path against a PINNED fixture
    // CID (off the live network), renders through the native T1 path IDENTICALLY
    // to the SERVER path of the exact same bytes. Parity is the whole claim — the
    // content-addressed path is NOT a second-class renderer.
    let site = read_fixture_html(FIXTURE);

    // Pin the site under its real CID and resolve it through the verifiable
    // content-retrieval seam (the bytes come back ONLY after they verify).
    let mut retriever = PinnedRawRetriever::default();
    let cid = retriever.put(site.as_bytes());
    let verified = resolve_verified_html(&retriever, &cid);

    // The verified bytes are byte-for-byte the pinned site: nothing was altered on
    // the content-addressed way in.
    assert_eq!(
        verified, site,
        "the verified content-addressed bytes are the pinned site, unaltered"
    );

    // Render BOTH the served path (the exact bytes, straight to the native path)
    // and the content-addressed path (the verified bytes), and assert the two
    // painted transcripts are byte-for-byte identical: same content renders the
    // same, so the content-addressed path is at parity with the server path.
    let served_transcript = render_bytes_transcript(&site);
    let content_addressed_transcript = render_bytes_transcript(&verified);
    assert_eq!(
        content_addressed_transcript, served_transcript,
        "the `ipfs://` render must match the served render of the same bytes \
         byte-for-byte: the content-addressed path is at parity, not second-class"
    );

    // And assert that shared render against the committed golden, so a regression
    // anywhere in the native T1 path (parse / cascade / shaping / layout / paint)
    // turns the golden red under the `verify` gate.
    let golden = golden_path(FIXTURE);
    let expected = std::fs::read_to_string(&golden).unwrap_or_else(|e| {
        panic!(
            "missing golden {} ({e}). Regenerate with: cargo test -p native-renderer \
             --test t1_content_addressed_floor -- --ignored regenerate_goldens",
            golden.display()
        )
    });
    assert_eq!(
        content_addressed_transcript,
        expected.trim_end_matches('\n'),
        "the content-addressed site drifted from its committed golden ({}). If this \
         render change is intended, regenerate the golden and review the diff.",
        golden.display()
    );
}

#[test]
fn the_site_renders_via_the_native_t1_path_with_shaped_text() {
    // Beyond parity: prove the content-addressed site actually went through the T1
    // NATIVE path (not a stub) — real shaped runs with positive proportional
    // advances, real per-font-size line heights, and cascaded colour reaching the
    // surface, exactly as the server floor demands of its pages.
    let site = read_fixture_html(FIXTURE);
    let mut retriever = PinnedRawRetriever::default();
    let cid = retriever.put(site.as_bytes());
    let verified = resolve_verified_html(&retriever, &cid);

    let out = render_bytes(&verified);
    assert!(!out.layout.runs.is_empty(), "the site produced runs");
    // Real Latin/LTR shaping: every run has a positive proportional advance and a
    // real (positive) line height from the bundled font's metrics.
    assert!(
        out.layout.runs.iter().all(|r| r.advance > 0.0),
        "every run has a positive shaped advance"
    );
    assert!(
        out.layout.runs.iter().all(|r| r.line_height > 0.0),
        "every run has a real font line height"
    );
    // The <h1> line is larger than a body line — real per-font-size metrics.
    let max_line = out
        .layout
        .runs
        .iter()
        .map(|r| r.line_height)
        .fold(0.0_f32, f32::max);
    let min_line = out
        .layout
        .runs
        .iter()
        .map(|r| r.line_height)
        .fold(f32::MAX, f32::min);
    assert!(
        max_line > min_line,
        "heading/body lines differ in height (real shaping metrics)"
    );
    // Real pixels were painted at a positive size.
    assert!(
        out.surface.width > 0 && out.surface.height > 0,
        "painted a sized surface"
    );

    // The cascaded colour reaches the surface (not just the transcript): the
    // `.note` line is green (#0a7d33), an author rule over the core-CSS colour set.
    // The transcript splits text into per-word runs, so match a single word of
    // the `.note` line ("Verification").
    let note = out
        .layout
        .runs
        .iter()
        .find(|r| r.text.contains("Verification"))
        .expect("the .note run");
    assert_eq!(
        note.style.color,
        Color {
            r: 0x0a,
            g: 0x7d,
            b: 0x33
        },
        "the .note author colour cascaded onto its run"
    );
    let note_green = (0..out.surface.height).any(|y| {
        (0..out.surface.width).any(|x| out.surface.pixel(x, y) == Some([0x0a, 0x7d, 0x33, 255]))
    });
    assert!(note_green, "the .note painted in its cascaded green");
}

#[test]
fn a_hash_mismatch_fails_the_content_addressed_load_and_never_renders() {
    // The trust gate, at the T1 floor: the content is hash-verified on the way in,
    // so TAMPERED bytes (that do not hash to the CID) must FAIL the load and NEVER
    // reach the renderer. A content-addressed floor that rendered unverified bytes
    // would not be a floor at all.
    let site = read_fixture_html(FIXTURE);
    let honest_cid = cid_v1_raw_sha256(site.as_bytes()).expect("derive fixture cid");

    let mut retriever = PinnedRawRetriever::default();
    // The origin holds bytes that do NOT match the CID.
    retriever.put_tampered_under(
        &honest_cid,
        b"<!doctype html><h1>tampered</h1> not the pinned site",
    );

    let result = resolve_ipfs_request(
        &retriever,
        &SchemeRequest {
            uri: format!("ipfs://{honest_cid}/index.html"),
        },
        &RedirectSink::new(),
    );

    let err = result.expect_err("a hash mismatch must fail the load, never render");
    // It is a backend/load failure carrying the verify reason, not a response the
    // renderer would ever be handed.
    assert!(
        matches!(&err, RendererError::Backend(msg) if msg.contains("mismatch")),
        "the mismatch fails the load with a verify reason, got: {err:?}"
    );
}

#[test]
fn the_pinned_cid_is_derived_from_the_site_and_verifies_deterministically() {
    // The fixture CID is PINNED (derived from the site bytes) and the whole path is
    // off the live network: the same site always derives the same CID, and
    // resolving it returns exactly those bytes. This guards the test's own
    // isolation + determinism claim (a CID that drifted from the bytes would make
    // the parity test flaky or network-dependent).
    let site = read_fixture_html(FIXTURE);
    let cid_a = cid_v1_raw_sha256(site.as_bytes()).expect("derive cid");
    let cid_b = cid_v1_raw_sha256(site.as_bytes()).expect("derive cid again");
    assert_eq!(cid_a, cid_b, "the pinned CID is deterministic for the site");

    let mut retriever = PinnedRawRetriever::default();
    let stored_cid = retriever.put(site.as_bytes());
    assert_eq!(
        stored_cid, cid_a,
        "the site is stored under its derived CID"
    );

    let verified = resolve_verified_html(&retriever, &stored_cid);
    assert_eq!(
        verified, site,
        "the pinned CID resolves to exactly the site bytes, off the network"
    );

    // A CID naming DIFFERENT bytes is a different identifier (so it could never
    // verify against this site).
    let other_cid = cid_v1_raw_sha256(b"different bytes").expect("derive other cid");
    assert_ne!(other_cid, stored_cid);
}

#[test]
fn the_site_stays_within_the_t1_static_scope() {
    // T1 is real static documents: NO floats/flex/grid/tables (T2) and NO
    // JavaScript (T3). Guard that the pinned site never quietly drifts into a
    // higher tier's constructs — a fixture that did would make this floor claim
    // more than T1 defines.
    let html = read_fixture_html(FIXTURE).to_ascii_lowercase();
    for banned in [
        "<table",
        "<script",
        "float:",
        "display:flex",
        "display: flex",
        "display:grid",
        "display: grid",
        "display:table",
        "display: table",
    ] {
        assert!(
            !html.contains(banned),
            "the site uses out-of-T1-scope construct `{banned}`"
        );
    }
}

/// Regenerate the committed golden from the current render output.
///
/// This is NOT part of the gate (it is `#[ignore]`d): it is the maintainer helper
/// that rewrites `site.golden.txt` after an INTENDED render change. Run it, then
/// review the diff before committing — a golden change is a rendering change.
///
/// ```sh
/// cargo test -p native-renderer --test t1_content_addressed_floor -- \
///     --ignored regenerate_goldens
/// ```
#[test]
#[ignore = "maintainer helper: rewrites the committed golden; run explicitly"]
fn regenerate_goldens() {
    let site = read_fixture_html(FIXTURE);
    let transcript = render_bytes_transcript(&site);
    let path = golden_path(FIXTURE);
    std::fs::write(&path, format!("{transcript}\n"))
        .unwrap_or_else(|e| panic!("write golden {}: {e}", path.display()));
    eprintln!("wrote {}", path.display());
}
