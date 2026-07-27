//! T0 **content-addressed floor**: the same v0-subset fragment over `ipfs://`
//! rendered at parity with the server floor.
//!
//! This is the objective guard for the conformance ladder's **T0 content-addressed
//! floor** (`docs/conformance-tiers.md` T0; user story 12 of the ship spec, task
//! `t0-content-addressed-floor-parity`). T0 is only "reached" once BOTH floors
//! land: the [server floor](t0_server_floor_goldens) (an authored fragment served
//! straight to the native path) AND this content-addressed floor (the SAME class
//! of fragment fetched over the hash-verified `ipfs://` path and rendered
//! IDENTICALLY). This test proves the second half.
//!
//! It reuses, rather than reinvents, the two seams the blocking tasks landed:
//!
//! * the **verifiable content-retrieval seam**: a pinned fixture CID is derived
//!   from the fragment bytes ([`fetcher::cid_v1_raw_sha256`], a raw/leaf CID), and
//!   the bytes are resolved through [`werust_core::ipfs::resolve_ipfs_request`]
//!   over a [`ContentRetriever`](fetcher::ContentRetriever), which returns bytes
//!   ONLY after each block hashes to its CID (task
//!   `verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend`). The full
//!   multi-block CAR/DAG verify mechanics are covered in the `fetcher::retriever`
//!   tests; this floor pins a single raw block so the render-parity assertion is
//!   the only variable;
//! * the **native T0 render path**: the resolved, verified bytes are rendered by
//!   the SAME [`NativeRenderer`] backend, driven THROUGH the [`Renderer`] seam at
//!   the SAME pinned viewport, that the server floor uses.
//!
//! The parity assertion is byte-for-byte against the committed server-floor
//! goldens (`tests/fixtures/t0-server-floor/<name>.golden.txt`): the SAME
//! reference the server floor asserts. So "content-addressed renders identically
//! to served" is not a fresh, drift-prone golden. It is literally the server
//! floor's golden, reached through the `ipfs://` path.
//!
//! Everything is off the live network: the content source is an in-memory,
//! per-test map, and the CID is derived from the fixture bytes so it verifies
//! deterministically.

use std::path::{Path, PathBuf};

use fetcher::{cid_v1_raw_sha256, ContentRetriever, RetrieveError, RetrievedContent};
use native_renderer::{NativeRenderer, RenderOutput};
use renderer::{LoadState, Renderer, SchemeRequest};
use werust_core::ipfs::{resolve_ipfs_request, RedirectSink};

/// The `raw` IPLD multicodec code (a leaf block's bytes ARE the content).
const RAW_CODEC: u64 = 0x55;

/// The viewport width the goldens are pinned at, in px. Identical to the server
/// floor's ([`t0_server_floor_goldens::FIXTURE_VIEWPORT_WIDTH`]) so the transcript
/// (and therefore the parity comparison) is against the exact same reference.
const FIXTURE_VIEWPORT_WIDTH: f32 = 800.0;

/// The committed fixture names shared with the server floor. The content-addressed
/// floor renders the SAME class of fragment, so it reuses the SAME fixtures and
/// the SAME goldens: parity is asserted against the server floor's own reference,
/// not a second copy that could drift.
const FIXTURES: &[&str] = &["article", "lists", "inline-styles", "headings"];

/// Absolute path to the shared `t0-server-floor` fixtures directory.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/t0-server-floor")
}

/// Read the authored fragment `<name>.html` (the same input the server floor
/// renders).
fn read_fixture_html(name: &str) -> String {
    let path = fixtures_dir().join(format!("{name}.html"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()))
}

/// The committed golden path for `name`: the SERVER floor's golden, reused as
/// the content-addressed floor's parity reference.
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

/// Render `html` bytes through the native T0 path, driven THROUGH the [`Renderer`]
/// seam at the pinned viewport, and return the painted software-text transcript.
///
/// This is the SAME render step the server floor uses; the only difference in the
/// content-addressed floor is WHERE the bytes came from (the hash-verified
/// `ipfs://` path, below), not how they are rendered, which is the whole point of
/// "parity".
fn render_bytes_transcript(html: &str) -> String {
    let mut backend = NativeRenderer::with_viewport_width(FIXTURE_VIEWPORT_WIDTH);
    let seam: &mut dyn Renderer = &mut backend;
    // Verified content-addressed bytes render exactly as a served document does:
    // hand them to the native path as a self-contained `data:text/html` source
    // (the T0 backend's self-contained entry point), so the ONLY variable versus
    // the server floor is the fetch path.
    seam.navigate(&data_url(html))
        .expect("the verified v0-subset fragment is navigable at T0");
    assert_eq!(
        seam.load_state(),
        LoadState::Finished,
        "the content-addressed fragment finished loading"
    );
    let RenderOutput { surface, .. } = backend.last_render().expect("a render happened");
    surface.transcript()
}

/// Build a `data:text/html,…` URL for `html`, percent-encoding exactly the bytes
/// the T0 backend's decoder treats specially (`%`, `+`) plus spaces, identical to
/// the server floor's encoding, so the fragment reaches the native path
/// byte-for-byte intact.
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

/// Resolve `cid` through the hash-verified content-addressed `ipfs://` path,
/// returning the VERIFIED bytes (as UTF-8), or panicking if the load would fail.
///
/// This drives the exact seam the `ipfs://` scheme handler drives in production:
/// [`resolve_ipfs_request`] over a [`ContentRetriever`], which returns bytes
/// ONLY after each block hashes to its CID. So the fragment is provably
/// hash-verified on the way in before it is ever rendered.
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
    String::from_utf8(response.body).expect("the fixture fragment is valid utf-8")
}

#[test]
fn content_addressed_fragment_renders_at_parity_with_the_server_floor_golden() {
    // The heart of the T0 content-addressed floor: the SAME v0-subset fragment,
    // fetched over the hash-verified `ipfs://` path against a PINNED fixture CID
    // (off the live network), renders through the native T0 path IDENTICALLY to
    // the server floor, asserted byte-for-byte against the SERVER floor's own
    // committed golden. This is what "T0 is not reached until both floors land"
    // means, made objective.
    for name in FIXTURES {
        let fragment = read_fixture_html(name);

        // Pin the fragment under its real CID and resolve it through the
        // verifiable content-retrieval seam (the bytes come back ONLY after they
        // verify against the CID).
        let mut retriever = PinnedRawRetriever::default();
        let cid = retriever.put(fragment.as_bytes());
        let verified = resolve_verified_html(&retriever, &cid);

        // The verified bytes are byte-for-byte the authored fragment: nothing was
        // altered on the content-addressed way in.
        assert_eq!(
            verified, fragment,
            "fixture {name}: the verified content-addressed bytes are the authored fragment"
        );

        // Render the VERIFIED bytes through the native T0 path and assert parity
        // against the SERVER floor's committed golden (the same reference the
        // server floor asserts).
        let actual = render_bytes_transcript(&verified);
        let golden_path = golden_path(name);
        let expected = std::fs::read_to_string(&golden_path).unwrap_or_else(|e| {
            panic!(
                "missing server-floor golden {} ({e}); regenerate it via the \
                 t0_server_floor_goldens helper",
                golden_path.display()
            )
        });
        assert_eq!(
            actual,
            expected.trim_end_matches('\n'),
            "fixture {name}: the content-addressed (`ipfs://`) render must match the \
             server-floor golden ({}) byte-for-byte: the two floors are at parity.",
            golden_path.display()
        );
    }
}

#[test]
fn a_hash_mismatch_fails_the_content_addressed_load_and_never_renders() {
    // The trust gate, at the floor: the content is hash-verified on the way in, so
    // TAMPERED bytes (that do not hash to the CID) must FAIL the load and NEVER
    // reach the renderer. A content-addressed floor that rendered unverified bytes
    // would not be a floor at all.
    let fragment = read_fixture_html("article");
    let honest_cid = cid_v1_raw_sha256(fragment.as_bytes()).expect("derive fixture cid");

    let mut retriever = PinnedRawRetriever::default();
    // The origin holds bytes that do NOT match the CID.
    retriever.put_tampered_under(&honest_cid, b"<h1>tampered</h1> not the pinned fragment");

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
        matches!(&err, renderer::RendererError::Backend(msg) if msg.contains("mismatch")),
        "the mismatch fails the load with a verify reason, got: {err:?}"
    );
}

#[test]
fn the_pinned_cid_is_derived_from_the_fragment_and_verifies_deterministically() {
    // The fixture CID is PINNED (derived from the fragment bytes) and the whole
    // path is off the live network: the same fragment always derives the same CID,
    // and resolving it returns exactly those bytes. This guards the test's own
    // isolation + determinism claim (a CID that drifted from the bytes would make
    // the parity test flaky or network-dependent).
    let fragment = read_fixture_html("headings");
    let cid_a = cid_v1_raw_sha256(fragment.as_bytes()).expect("derive cid");
    let cid_b = cid_v1_raw_sha256(fragment.as_bytes()).expect("derive cid again");
    assert_eq!(
        cid_a, cid_b,
        "the pinned CID is deterministic for the fragment"
    );

    let mut retriever = PinnedRawRetriever::default();
    let stored_cid = retriever.put(fragment.as_bytes());
    assert_eq!(stored_cid, cid_a, "content is stored under its derived CID");

    let verified = resolve_verified_html(&retriever, &stored_cid);
    assert_eq!(
        verified, fragment,
        "the pinned CID resolves to exactly the fragment bytes, off the network"
    );

    // Sanity: a CID naming DIFFERENT bytes is a different identifier (so it could
    // never verify against this fragment).
    let other_cid = cid_v1_raw_sha256(b"different bytes").expect("derive other cid");
    assert_ne!(other_cid, stored_cid);
}
