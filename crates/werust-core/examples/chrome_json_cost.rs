//! MEASURE what carrying the derived strings on the chrome JSON costs, per
//! chrome refresh (task `mobile-chrome-presentation-from-one-derivation`).
//!
//! The task added ten DERIVED fields (the status line, the trust badge + its
//! explanation, the banner text, the invalid-entry badge text, the progress
//! fraction + hint) to the document both mobile edges already decode on every
//! chrome refresh, so that each edge reads a field instead of re-deriving the
//! rule in Kotlin/Swift. That is a bigger payload on a hot-ish path, and the task
//! asked for a NUMBER rather than an assurance ("the honest expectation is 'no
//! measurable change'. Say so with a number").
//!
//! WHAT IT COMPARES: the BASELINE below is the FACTS-ONLY document (exactly the
//! eleven fields the two `ffi_json` twins used to emit), encoded with the SAME
//! `serde_json` encoder today's [`werust_core::chrome_json`] uses and frozen here
//! as a measurement fixture. Both are timed over the same chrome states, plus a
//! decode of each document (the edge's half of a refresh), so the comparison
//! covers the whole encode + parse round trip a refresh really pays.
//!
//! So the delta isolates ONE variable, the ten EXTRA FIELDS. It is deliberately
//! NOT this commit's literal before/after: the deleted twins hand-rolled their
//! JSON with `format!` plus their own escaping, so a true before/after would fold
//! the ENCODER SWAP into the same number and measure two changes at once. Read
//! the columns as "facts only" vs "facts + derivation", never as "before" vs
//! "after"; the conclusion rests on the absolute magnitude (microseconds, on an
//! event-driven cadence) rather than on which half of the delta is which.
//!
//! WHAT IT DOES NOT MEASURE, stated so the number is not over-read: the JNI /
//! C-ABI string hop itself, and the platform parsers on the other side
//! (`org.json.JSONObject` on Android, `JSONSerialization` on iOS). Neither can be
//! driven from this gate; the Rust-side parse is a proxy for their SHAPE (twice
//! the fields, roughly four times the bytes), not for their absolute speed.
//!
//! Run it (release, or the numbers are debug-build noise):
//!
//! ```sh
//! cargo run --release -p werust-core --example chrome_json_cost
//! ```
//!
//! The recorded run is
//! `docs/spikes/mobile-chrome-presentation-from-one-derivation/MEASUREMENT.md`.
//! Deliberately an EXAMPLE, not a test: it is a measurement harness, and a
//! wall-clock threshold would be a flaky gate on shared CI.

use std::time::{Duration, Instant};

use renderer::{LoadState, TrustPosture};
use werust_core::{chrome_json, ChromeState, LoadStep};

/// The FACTS-ONLY chrome document: the eleven `ChromeState` facts and nothing
/// else — the field SET the two `ffi_json` twins emitted before this task, but
/// encoded here with the same `serde_json` the shipping encoder uses rather than
/// with their hand-rolled `format!`, so the comparison varies the FIELDS and not
/// the encoder (see the module docs).
///
/// Frozen here as the measurement BASELINE only. It is not used by anything that
/// ships, and it must not grow: its whole purpose is to be the field set the wire
/// carried on 2026-07-31, before the derived strings joined it.
fn baseline_facts_only_json(state: &ChromeState) -> String {
    serde_json::json!({
        "url": state.url_text,
        "loadState": match state.load_state {
            LoadState::Idle => "idle",
            LoadState::Started => "started",
            LoadState::Committed => "committed",
            LoadState::Finished => "finished",
            LoadState::Failed => "failed",
        },
        "loading": state.is_loading(),
        "loadStep": state.load_step().wire_name(),
        "canGoBack": state.can_go_back,
        "canGoForward": state.can_go_forward,
        "trustPosture": werust_core::debug::trust_posture_wire_name(state.trust_posture),
        "error": state.last_error,
        "failureKind": state.failure_kind().map(|kind| kind.wire_name()),
        "retryable": state.failure_is_retryable(),
        "invalidEntry": state.invalid_entry,
    })
    .to_string()
}

/// The chrome states a refresh actually encodes, across a whole load: idle, the
/// pre-content name resolution, content in flight, a settled verified page, a
/// mutable-name page, a transient failure and a hard failure.
fn representative_states() -> Vec<(&'static str, ChromeState)> {
    vec![
        ("idle", ChromeState::default()),
        (
            "resolving a name",
            ChromeState {
                url_text: "ronan.eth".into(),
                load_step: LoadStep::ResolvingName,
                ..ChromeState::default()
            },
        ),
        (
            "fetching content",
            ChromeState {
                url_text:
                    "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi/index.html"
                        .into(),
                load_state: LoadState::Started,
                load_step: LoadStep::FetchingContent,
                ..ChromeState::default()
            },
        ),
        (
            "settled, content-verified",
            ChromeState {
                url_text:
                    "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi/index.html"
                        .into(),
                load_state: LoadState::Finished,
                trust_posture: TrustPosture::ContentVerified,
                ..ChromeState::default()
            },
        ),
        (
            "settled, mutable name",
            ChromeState {
                url_text: "ipns://k51qzi5uqu5dh9giahc358e235iqoncw9lpyc6vq2ttzwlmzfzu2mmojfhtsg6"
                    .into(),
                load_state: LoadState::Finished,
                trust_posture: TrustPosture::MutableName,
                ..ChromeState::default()
            },
        ),
        (
            "transient failure",
            ChromeState {
                url_text: "ronan.eth".into(),
                load_state: LoadState::Failed,
                last_error: Some("transport error: timeout: global".into()),
                ..ChromeState::default()
            },
        ),
        (
            "hard failure",
            ChromeState {
                url_text: "ronan.eth".into(),
                load_state: LoadState::Failed,
                last_error: Some("IPNS record did not verify: bad signature".into()),
                ..ChromeState::default()
            },
        ),
    ]
}

/// Time `iterations` calls of `f`, returning the per-call average. The result is
/// consumed through a black box so the optimiser cannot delete the work.
fn time_per_call(iterations: u32, mut f: impl FnMut() -> usize) -> Duration {
    // Warm up: first-call effects (allocator growth, branch predictors) are not
    // what a steady-state refresh pays.
    for _ in 0..iterations / 10 {
        std::hint::black_box(f());
    }
    let start = Instant::now();
    for _ in 0..iterations {
        std::hint::black_box(f());
    }
    start.elapsed() / iterations
}

fn main() {
    const ITERATIONS: u32 = 100_000;

    println!("werust chrome-refresh cost: facts only (same encoder) vs facts + derivation");
    println!("{ITERATIONS} iterations per measurement, per-call averages\n");

    let mut baseline_bytes = 0usize;
    let mut carrier_bytes = 0usize;
    let states = representative_states();

    println!(
        "{:<28} {:>12} {:>12} {:>13} {:>13}",
        "chrome state", "facts (B)", "+derived (B)", "encode facts", "encode both"
    );
    for (name, state) in &states {
        let facts_only = baseline_facts_only_json(state);
        let with_derivation = chrome_json(state);
        baseline_bytes += facts_only.len();
        carrier_bytes += with_derivation.len();

        let encode_facts_only = time_per_call(ITERATIONS, || baseline_facts_only_json(state).len());
        let encode_with_derivation = time_per_call(ITERATIONS, || chrome_json(state).len());
        println!(
            "{:<28} {:>12} {:>12} {:>13?} {:>13?}",
            name,
            facts_only.len(),
            with_derivation.len(),
            encode_facts_only,
            encode_with_derivation
        );
    }

    // The EDGE's half of a refresh: parsing the document it was handed. A proxy
    // for the platform parsers (see the module docs), driven on the largest
    // document so it is the worst case rather than the average.
    let (_, worst) = states
        .iter()
        .max_by_key(|(_, state)| chrome_json(state).len())
        .expect("there is at least one state");
    let facts_only = baseline_facts_only_json(worst);
    let with_derivation = chrome_json(worst);
    let parse_facts_only = time_per_call(ITERATIONS, || {
        serde_json::from_str::<serde_json::Value>(&facts_only)
            .expect("valid JSON")
            .as_object()
            .map_or(0, serde_json::Map::len)
    });
    let parse_with_derivation = time_per_call(ITERATIONS, || {
        serde_json::from_str::<serde_json::Value>(&with_derivation)
            .expect("valid JSON")
            .as_object()
            .map_or(0, serde_json::Map::len)
    });

    println!(
        "\ntotal payload over {} states: {baseline_bytes} B -> {carrier_bytes} B ({:+.0}%)",
        states.len(),
        (carrier_bytes as f64 / baseline_bytes as f64 - 1.0) * 100.0
    );
    println!(
        "worst-case decode (largest document): {parse_facts_only:?} -> {parse_with_derivation:?}"
    );
    println!(
        "\nA chrome refresh is ONE encode + ONE decode. The mobile refresh cadence is \
         event-driven\n(after each core action / page-lifecycle signal), not a timer or a \
         per-frame poll, so this\ncost is paid a handful of times per navigation."
    );
}
