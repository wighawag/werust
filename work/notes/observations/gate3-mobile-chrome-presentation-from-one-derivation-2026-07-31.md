---
title: "Gate-3 verdict: mobile-chrome-presentation-from-one-derivation (APPROVE) — three copies became one, and the missing explanation finally reached mobile"
date: 2026-07-31
status: open
reviewOf: mobile-chrome-presentation-from-one-derivation
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main`. 1,626 insertions against 858 deletions: the Kotlin and Swift presentation twins collapsed onto the shared derivation, both `ffi_json.rs` twins deleted, a new `werust_core::chrome_json`, a 477-line shape guard, a measurement, and a decisions block.

The headline is the deletion count. This is the task that finally makes the repo's own thesis true on the two platforms most users are on: the trust EXPLANATION (`trust_indicator_detail`, the text saying what a posture MEANS) had shipped desktop-only for months precisely because the rules were hand-copied three times.

## What I measured, because the Rust gate cannot compile a line of this

The Gate-2 reviewer flagged, correctly, that "the mobile edges still build" is NOT evidenced by a green Ubuntu gate: `verify.yml` is pure Rust, and this diff carries a 21-field Kotlin data class, 237 changed lines of Swift, a new dialog path on both edges and an added `core.chrome()` call during initial layout. Hand-reading was all the reviewer could do. So I measured both:

- **iOS: [run 30603014298](https://github.com/wighawag/werust/actions/runs/30603014298)**, `mobile-ios.yml`, push to `main` after the merge — **SUCCESS**. The Swift edge compiles and the leg builds it.
- **Android: [run 30603036662](https://github.com/wighawag/werust/actions/runs/30603036662)**, `release.yml` dry run, `android-apk` job — **SUCCESS** (as were all five jobs). The Kotlin edge compiles and the APK builds.

So the acceptance criterion is met by evidence, not by inspection. This is the fourth time this drive that a CI-shaped criterion needed the conductor to fire the run.

## Criteria, ticked

1. **Both mobile edges consume the shared derivation instead of re-deriving it.** MET, and the diff proves it by SUBTRACTION: `WerustCore.swift` loses 237 lines, `ffi_json.rs` disappears on both edges (250 lines each), and the `statusLine()`/`trustIndicator()`/`errorBanner()`/`invalidEntryBadge()`/`loadProgress*()` twins are gone. A "consumes the shared derivation" claim that does not delete the old rules is the one to distrust; this one deletes them.
2. **The trust EXPLANATION now exists on mobile.** MET. That was the gap the task said was "not cosmetic", and closing it was in scope rather than deferred.
3. **Fork (a) taken — derived strings on the chrome JSON, not new FFI entry points — with the decision recorded.** MET, and the payload cost was MEASURED rather than assumed, which the task explicitly asked for: microseconds, on an event-driven cadence, not observable. The honest expectation was "no measurable change" and the number backs it.
4. **The wire vocabulary stays shared.** MET.
5. **No behaviour change except the new explanation; divergences recorded rather than silently normalised.** MET, and the recorded divergences are the evidence for why the duplication had to go (the load-progress fraction really was `0.25` in Rust and Swift but `25` in Kotlin).

## The one finding I am NOT ratifying

**The accessibility wiring drops the state name.** Both edges now set the badge's accessibility text to the ~240-character explanation (`trust.contentDescription` on Android, `trustLabel.accessibilityLabel` on iOS), REPLACING the short badge label. So a screen-reader user hears the essay on every focus and never hears WHICH trust state the badge is in — the single fact a sighted user gets instantly and repeatedly. Sighted users are unaffected, which is exactly why it would go unnoticed. On a browser whose thesis is a legible trust posture, that is the wrong half to drop, and the idiomatic platform split (label = state, `stateDescription`/`accessibilityHint` = explanation) was not among the alternatives weighed.

Cut as `mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay`, with the fix prescribed and both strings still required to come from the one derivation. Not a block: the explanation reaching mobile at all is a large net gain over the status quo, and the fix is a few lines on each edge.

## Review-nit triage (4 raised, all non-blocking)

- **D2's scope expansion.** RAISED TO THE HUMAN rather than ratified by me. The task said "extend the chrome JSON"; the change also deleted both `ffi_json.rs` twins, moved the wire form into a new PUBLIC `werust_core::chrome_json`, swapped a hand-rolled `format!`+escape encoder for `serde_json`, and changed JSON key order to sorted. The reasoning is genuinely good — adding ten fields to two hand-rolled encoders would have committed the same duplication one level down, which is the exact sin this task exists to undo — and the order change is safe because both edges decode with a real parser. But it is a new public core API plus a wire-order change, so it wants an explicit yes.
- **The measurement baseline is mislabelled.** Real, small, and folded into the new task: `baseline_facts_only_json` re-encodes the eleven original facts with `serde_json`, so the delta measures the ten EXTRA FIELDS, not the commit's real before/after (which also swapped the encoder). The conclusion is unaffected; only the words "the pre-change encoder" are wrong, and a number described as something it never measured is how a future decision gets made on bad evidence.
- **The accessibility default.** Acted on (above).
- **"The mobile edges still build" was unevidenced.** Closed by measurement (above).

## An off-path finding the build filed correctly

`mobile-debug-view-row-rules-still-re-derived-2026-07-31.md`: the same duplication persists ONE SURFACE OVER, in the mobile DEBUG view (`networkTrustLabel()`, `consoleLevelColor()`, `trustColor()` twins of `werust_core::debug`), plus hard-coded hex colours transcribed from `desktop_paint::CLASS_COLORS` with no test that they agree — while the GTK edge HAS exactly such a test. Correctly left out of scope and correctly written down; it is the obvious next collapse, and it now has a named precedent to follow.
