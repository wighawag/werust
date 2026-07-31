# Measurement: what the derived strings cost a chrome refresh

Task: `mobile-chrome-presentation-from-one-derivation`. The task asked for the chrome-refresh cost "with an actual measurement, not an assurance", because the collapse moves ten DERIVED strings onto a document both mobile edges decode on every refresh.

Harness: `crates/werust-core/examples/chrome_json_cost.rs` (re-runnable; it carries the pre-change facts-only encoder as a frozen baseline fixture). Run it with:

```sh
cargo run --release -p werust-core --example chrome_json_cost
```

## The run

2026-07-31, AMD Ryzen 7 PRO 6850U, Linux 6.12.96, rustc 1.91.1, `--release`, 100,000 iterations per measurement, per-call averages.

| chrome state | was (B) | now (B) | encode was | encode now |
| --- | ---: | ---: | ---: | ---: |
| idle | 207 | 581 | 1.333 µs | 2.895 µs |
| resolving a name | 226 | 614 | 1.337 µs | 2.917 µs |
| fetching content | 298 | 750 | 1.417 µs | 3.015 µs |
| settled, content-verified | 287 | 673 | 1.301 µs | 2.935 µs |
| settled, mutable name | 275 | 771 | 1.397 µs | 3.127 µs |
| transient failure | 254 | 740 | 1.477 µs | 3.296 µs |
| hard failure | 259 | 748 | 1.487 µs | 3.234 µs |

Worst-case decode (the largest document, Rust-side `serde_json` parse): **723 ns -> 1.581 µs**.

Total payload across the seven states: **1,806 B -> 4,877 B (+170%)**, i.e. an average chrome document grows from ~258 B to ~697 B.

## What it means

A chrome refresh is ONE encode plus ONE decode, so the round trip goes from about **2.1 µs to about 4.6 µs: +2.5 µs per refresh**, on a document ~440 bytes larger. Both halves roughly double, which is what you expect from a document with twice the fields.

That is not a hot path on either mobile edge. The refresh cadence is EVENT-DRIVEN (after each core action and each page-lifecycle signal, `afterCoreAction` on both edges, never a timer and never a per-frame poll, which is the Android ANR guard's standing constraint), so a navigation pays this a handful of times, not hundreds of times per second. Against that, the microseconds here are dominated by everything else in the same refresh: the JNI / C-ABI string hop, the platform JSON parser, and the widget assignments that follow.

The honest expectation in the task ("no measurable change") holds at the level that matters: **+2.5 µs a few times per navigation is not observable in the UI**, while the same refresh already crosses an FFI boundary and repaints widgets. It is NOT literally "no change" at the microsecond level, which is why the number is recorded rather than asserted.

## What is not measured

- The JNI (`nativeChromeJson`) and C-ABI (`werust_ios_chrome_json`) string hop itself.
- The PLATFORM parsers on the other side: `org.json.JSONObject` (Android) and `JSONSerialization` (iOS). Neither can be driven from this repo's pure-Rust `verify` gate, which has no Android SDK and no Xcode. The Rust-side decode above is a proxy for their SHAPE (twice the fields, ~2.7x the bytes), not for their absolute speed.
- Anything on-device. The two mobile CI legs build and launch the edges; they do not time a refresh.

Against those unmeasured parts, this change also REMOVES work from each edge: the Kotlin/Swift `when`/`switch` chains that used to derive the same ten values per refresh are gone, so the edge-side cost of a refresh drops slightly even as the document grows.
