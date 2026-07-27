//! The concurrency BOUNDARY for `ipfs://` scheme resolution: run the blocking
//! CAR fetch + per-block verify + DAG reassembly OFF the GTK main thread, then
//! marshal the completion (finish the request + mark the shared trust posture)
//! BACK onto the main thread.
//!
//! # Why this exists
//!
//! The desktop `ipfs://` scheme handler used to call
//! [`resolve_ipfs_request`](werust_core::ipfs::resolve_ipfs_request)
//! SYNCHRONOUSLY inside the WebKitGTK scheme-handler closure, which runs on the
//! single GTK main thread — once per request (the main document AND every
//! sub-resource). A trustless-gateway CAR fetch is network I/O measured in
//! seconds, so blocking the UI thread on it froze the whole window (GNOME's
//! "application not responding" dialog on a real load). This module moves the
//! blocking work off that thread so the event loop keeps turning and
//! sub-resource fetches proceed concurrently instead of serializing the loop.
//!
//! # The boundary and the marshalling rule (`docs/adr/0008`)
//!
//! The retriever stays SYNCHRONOUS (no async runtime, `docs/adr/0004`); the
//! concurrency boundary is HERE, at the scheme-handler edge, split into two
//! halves so the whole thing is testable without a GTK loop:
//!
//! * [`RetrievalOutcome`] is the ONLY thing that crosses the thread boundary. It
//!   is a plain `Send` value ([`SchemeResponse`] on success, a message +
//!   `verified` flag on failure) computed by [`retrieve_off_thread`], which runs
//!   the whole blocking [`resolve_ipfs_request`] on a worker. Because it is a
//!   value, NOTHING GTK and NOTHING `!Send` (not the `WebKitURISchemeRequest`,
//!   not the `Rc<RefCell<LoadLifecycle>>`) is ever touched off the main thread.
//! * [`complete_ipfs_request`] is the completion half. It runs on the
//!   marshalling thread (the GTK main thread in production), takes the `Send`
//!   [`RetrievalOutcome`] plus the `!Send` request sink and shared lifecycle, and
//!   applies it: on a VERIFIED success it marks the shared posture
//!   content-verified and finishes the request with the bytes; on ANY failure it
//!   fails the load WITHOUT marking the posture (fail-closed, verification
//!   unchanged). The shared load lifecycle is thus mutated ONLY on the main
//!   thread — the desktop analogue of the Android Mutex fix — so the worker can
//!   never race the UI thread's posture updates.
//!
//! The production wiring (`backend.rs::install_ipfs`) glues these with
//! [`gio::spawn_blocking`](gio::spawn_blocking) (run [`retrieve_off_thread`] on
//! gio's I/O thread pool) and
//! [`glib::MainContext::spawn_local`](glib::MainContext::spawn_local) (await the
//! outcome and call [`complete_ipfs_request`] back on the GTK loop). Those two
//! calls need a real GTK main context, so they live in `backend.rs`; the pure
//! boundary logic — and its no-off-thread-lifecycle-access guarantee — lives here
//! and is exercised headlessly by the tests below.

use fetcher::ContentRetriever;
use renderer::{RendererError, SchemeRequest, SchemeResponse};
use werust_core::ipfs::{resolve_ipfs_request, RedirectSink};

use crate::SharedLifecycle;

/// The `Send` result of an off-thread `ipfs://` resolution: the ONLY value that
/// crosses the worker/main-thread boundary.
///
/// It carries NO GTK type and NO `!Send` handle, so a worker thread can produce
/// it and hand it back to the main thread for [`complete_ipfs_request`] to apply.
/// On [`Ok`] the verified [`SchemeResponse`] is delivered and the load is marked
/// content-verified; on [`Err`] the load fails closed with the legible reason and
/// the posture is left untouched.
pub type RetrievalOutcome = Result<SchemeResponse, RendererError>;

/// Run the blocking `ipfs://` resolution (CAR fetch + per-block verify + DAG
/// reassembly) and return its `Send` [`RetrievalOutcome`].
///
/// This is the half that runs OFF the main thread (on gio's I/O thread pool in
/// production, via `gio::spawn_blocking`). It does the full
/// [`resolve_ipfs_request`] — the SAME verifying path the synchronous handler
/// used — but touches nothing `!Send`: it takes only the (`Send + Sync`)
/// retriever and the request URI, and returns a plain value. Verification is
/// UNCHANGED: a tamper / incomplete / budget / path failure comes back as the
/// same `Err` it always did, so the completion half fails the load closed.
///
/// `redirects` is the shell's `_redirects` 3xx [`RedirectSink`]: a matched
/// redirect rule pushes its `ipfs://<rootcid><to>` target there (nothing is ever
/// served for the redirected-FROM url, so this still returns an `Err` for it) and
/// the shell drains it on its pump to perform the navigation. The sink is
/// `Send + Sync` (an `Arc<Mutex<_>>` inside), so it crosses the worker boundary
/// like the retriever does — unlike the `!Send` lifecycle, which still never
/// leaves the main thread.
pub fn retrieve_off_thread<R: ContentRetriever>(
    retriever: &R,
    uri: String,
    redirects: &RedirectSink,
) -> RetrievalOutcome {
    resolve_ipfs_request(retriever, &SchemeRequest { uri }, redirects)
}

/// The sink a completed `ipfs://` request is delivered to on the marshalling
/// thread: finish with verified bytes, or fail the load closed.
///
/// Abstracts WebKitGTK's `WebKitURISchemeRequest` (`request.finish` /
/// `request.finish_error`) so the completion logic — mark-verified-only-on-success
/// and fail-closed-on-error — is testable without a GTK request. The production
/// backend implements this over the real request; the tests implement it over a
/// recording double. Like the real request it is `!Send`: it is only ever touched
/// on the marshalling (main) thread.
pub trait RequestSink {
    /// Finish the request with the verified `response` bytes (a successful,
    /// fully-verified resolution).
    fn finish(&mut self, response: SchemeResponse);
    /// Fail the load with `error` (a fail-closed resolution failure). Nothing is
    /// rendered.
    fn fail(&mut self, error: RendererError);
}

/// Apply an off-thread [`RetrievalOutcome`] on the marshalling (main) thread:
/// finish the request with the verified bytes and mark the shared load
/// content-verified, or fail the load closed WITHOUT marking it.
///
/// This is the completion half of the concurrency boundary, and it runs ONLY on
/// the thread that owns `life` (the GTK main thread in production, driven by
/// `MainContext::spawn_local`). Marshalling the mark here — rather than letting
/// the worker touch the `Rc<RefCell<LoadLifecycle>>` — is what keeps the shared
/// posture free of the worker/UI-thread race (`docs/adr/0008`), the desktop
/// analogue of the Android Mutex-guarded session.
///
/// Trust is UNCHANGED from the synchronous path: the posture is upgraded to
/// content-verified (the shared two-axis rule then surfaces the honest
/// ENS/mutable variant) ONLY on a verified `Ok`; any `Err` fails the load and
/// leaves the posture untouched, so a page whose bytes did not verify is never
/// reported verified.
pub fn complete_ipfs_request<S: RequestSink>(
    outcome: RetrievalOutcome,
    sink: &mut S,
    life: &SharedLifecycle,
) {
    match outcome {
        Ok(response) => {
            // The bytes verified against their CID on the worker: mark the current
            // load content-verified HERE, on the main thread, so the shared
            // lifecycle is only ever mutated on its owning thread.
            life.borrow_mut().mark_content_verified();
            sink.finish(response);
        }
        Err(error) => {
            // Verification failed (a hash mismatch, an unverifiable CID, an
            // incomplete DAG, a budget overflow, a source error): fail the load
            // WITHOUT marking it, so unverified bytes never render AND the posture
            // stays untrusted. Fail-closed, exactly as the synchronous path was.
            sink.fail(error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LoadLifecycle;
    use fetcher::{cid_v1_raw_sha256, RetrieveError, RetrievedContent};
    use renderer::TrustPosture;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    /// A pinned, in-memory `ContentRetriever` double (`Send + Sync`, off the live
    /// network) that verifies a single raw/leaf block against its CID and records
    /// which THREAD it ran the (blocking) retrieval on. Holding honest bytes under
    /// their real CID exercises the verified path; holding tampered bytes exercises
    /// the fail-closed path. It is `Send + Sync` because the whole point is that it
    /// runs on a worker thread.
    struct ThreadRecordingRetriever {
        cid: String,
        bytes: Vec<u8>,
        ran_on_thread: AtomicU64,
    }

    impl ThreadRecordingRetriever {
        fn new(cid: &str, bytes: &[u8]) -> Self {
            Self {
                cid: cid.to_string(),
                bytes: bytes.to_vec(),
                ran_on_thread: AtomicU64::new(0),
            }
        }
    }

    fn thread_id_u64() -> u64 {
        // A stable-enough per-thread token: hash the ThreadId. Only equality
        // across the two observations matters to the test.
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::thread::current().id().hash(&mut h);
        h.finish()
    }

    impl ContentRetriever for ThreadRecordingRetriever {
        fn retrieve(&self, cid: &str, _path: &str) -> Result<RetrievedContent, RetrieveError> {
            self.ran_on_thread.store(thread_id_u64(), Ordering::SeqCst);
            if cid != self.cid {
                return Err(RetrieveError::MissingBlock {
                    cid: cid.to_string(),
                });
            }
            // Re-verify the held bytes against the CID, exactly as the real path
            // would: honest bytes pass, tampered bytes are a hard mismatch.
            let expected = cid_v1_raw_sha256(&self.bytes).expect("derive cid for held bytes");
            if expected != self.cid {
                return Err(RetrieveError::BlockHashMismatch {
                    cid: self.cid.clone(),
                });
            }
            Ok(RetrievedContent {
                bytes: self.bytes.clone(),
                codec: 0x55,
            })
        }
    }

    /// A recording [`RequestSink`] double standing in for the WebKitGTK request:
    /// it captures whether the load was finished (with which bytes) or failed
    /// (with which reason), so the completion behaviour is asserted without a GTK
    /// request. It is `!Send` (holds a `Rc`) exactly like the real request, so the
    /// type system helps guarantee it is only touched on the marshalling thread.
    #[derive(Default)]
    struct RecordingSink {
        finished: Option<SchemeResponse>,
        failed: Option<String>,
        _not_send: std::marker::PhantomData<Rc<()>>,
    }

    impl RequestSink for RecordingSink {
        fn finish(&mut self, response: SchemeResponse) {
            self.finished = Some(response);
        }
        fn fail(&mut self, error: RendererError) {
            self.failed = Some(error.to_string());
        }
    }

    fn shared_life(url: &str) -> SharedLifecycle {
        let life: SharedLifecycle = Rc::new(RefCell::new(LoadLifecycle::default()));
        life.borrow_mut().begin(url);
        life
    }

    #[test]
    fn retrieval_runs_off_the_marshalling_thread_and_the_completion_runs_on_it() {
        // Acceptance (the off-thread wiring at the seam, headless): the blocking
        // retrieval runs on a DIFFERENT thread than the completion, and the
        // shared lifecycle is only ever touched on the completion (marshalling)
        // thread. This is the whole point: the UI thread never blocks on the CAR
        // fetch, and the posture is never mutated off-thread.
        let page = b"<!doctype html><title>ipfs</title><h1>verified off-thread</h1>";
        let cid = cid_v1_raw_sha256(page).expect("derive pinned fixture cid");
        let retriever = Arc::new(ThreadRecordingRetriever::new(&cid, page));

        let main_thread = thread_id_u64();
        let uri = format!("ipfs://{cid}/index.html");

        // Marshal exactly as production does: run `retrieve_off_thread` on a
        // worker, hand the `Send` outcome back, and apply the completion on THIS
        // (the marshalling) thread. A std::thread + join stands in for
        // gio::spawn_blocking + spawn_local without a GTK loop.
        let retriever_for_worker = retriever.clone();
        let outcome = std::thread::spawn(move || {
            retrieve_off_thread(retriever_for_worker.as_ref(), uri, &RedirectSink::new())
        })
        .join()
        .expect("worker thread completes");

        // The retrieval ran on the worker thread, NOT the marshalling thread.
        let ran_on = retriever.ran_on_thread.load(Ordering::SeqCst);
        assert_ne!(
            ran_on, main_thread,
            "the blocking retrieval must run OFF the marshalling (UI) thread"
        );

        // The completion runs on the marshalling thread and touches the shared
        // lifecycle there.
        let life = shared_life(&format!("ipfs://{cid}/index.html"));
        assert_eq!(
            life.borrow().posture(),
            TrustPosture::UnverifiedOrigin,
            "a fresh load is untrusted until the verified completion runs"
        );
        let mut sink = RecordingSink::default();
        complete_ipfs_request(outcome, &mut sink, &life);

        // The verified bytes were delivered and the posture flipped, ON the
        // marshalling thread.
        assert_eq!(
            sink.finished.as_ref().map(|r| r.body.as_slice()),
            Some(page.as_slice()),
            "the verified bytes are delivered to render"
        );
        assert!(sink.failed.is_none(), "a verified load does not fail");
        assert_eq!(
            life.borrow().posture(),
            TrustPosture::ContentVerified,
            "the verified completion marks the shared posture content-verified"
        );
    }

    #[test]
    fn a_verification_failure_off_thread_still_fails_the_load_and_never_marks_verified() {
        // Trust unchanged, fail-closed: when the off-thread retrieval fails to
        // verify (tampered bytes under a real CID), the completion fails the load
        // with the distinct reason and NEVER marks the posture verified — exactly
        // the guarantee the synchronous handler gave.
        let honest = b"the page this cid actually names";
        let cid = cid_v1_raw_sha256(honest).expect("derive pinned fixture cid");
        // Hold TAMPERED bytes under the honest CID: the worker's retrieve fails.
        let retriever = ThreadRecordingRetriever::new(&cid, b"tampered bytes not matching the cid");

        let uri = format!("ipfs://{cid}/index.html");
        let outcome = std::thread::scope(|s| {
            s.spawn(|| retrieve_off_thread(&retriever, uri, &RedirectSink::new()))
                .join()
                .unwrap()
        });

        let life = shared_life(&format!("ipfs://{cid}/index.html"));
        let mut sink = RecordingSink::default();
        complete_ipfs_request(outcome, &mut sink, &life);

        assert!(
            sink.finished.is_none(),
            "unverified bytes must never be delivered to render"
        );
        let reason = sink.failed.expect("a hash mismatch must fail the load");
        assert!(
            reason.contains("mismatch"),
            "the failure carries the distinct verify reason, got: {reason}"
        );
        assert_eq!(
            life.borrow().posture(),
            TrustPosture::UnverifiedOrigin,
            "a load that did not verify is NEVER reported content-verified"
        );
    }

    #[test]
    fn concurrent_off_thread_retrievals_do_not_serialize_and_each_completes_correctly() {
        // Sub-resources do not serialize the event loop: several requests run
        // their blocking retrieval concurrently on worker threads, and each
        // completion delivers its own verified bytes independently. A shared,
        // `Send + Sync` retriever is fetched from concurrently, exactly as the
        // production `Arc`-shared retriever is.
        let pages: Vec<Vec<u8>> = (0..4u8)
            .map(|i| format!("<!doctype html><title>r{i}</title>").into_bytes())
            .collect();
        let handles: Vec<_> = pages
            .iter()
            .map(|page| {
                let cid = cid_v1_raw_sha256(page).expect("cid");
                let retriever = Arc::new(ThreadRecordingRetriever::new(&cid, page));
                let uri = format!("ipfs://{cid}/index.html");
                let r = retriever.clone();
                (
                    page.clone(),
                    std::thread::spawn(move || {
                        retrieve_off_thread(r.as_ref(), uri, &RedirectSink::new())
                    }),
                )
            })
            .collect();

        for (page, handle) in handles {
            let outcome = handle.join().expect("worker completes");
            let life = shared_life("ipfs://x/");
            let mut sink = RecordingSink::default();
            complete_ipfs_request(outcome, &mut sink, &life);
            assert_eq!(
                sink.finished.as_ref().map(|r| r.body.clone()),
                Some(page),
                "each concurrent retrieval delivers its own verified bytes"
            );
            assert_eq!(life.borrow().posture(), TrustPosture::ContentVerified);
        }
    }
}
