---
title: review-gate non-blocking nits for 'mobile-chrome-presentation-from-one-derivation' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: mobile-chrome-presentation-from-one-derivation
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'mobile-chrome-presentation-from-one-derivation' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the scope expansion in D2: this task was scoped to 'extend the chrome JSON', but it also DELETED crates/werust-android/rust/src/ffi_json.rs and crates/werust-ios/rust/src/ffi_json.rs and moved the whole wire form to a new public werust_core::chrome_json, swapping a hand-rolled format!+escape encoder for serde_json and changing JSON key order to sorted. The reasoning (adding ten fields twice would commit the same duplication one level down) is sound and the order change is safe because both edges decode with a real parser, but it is a new public core API plus a wire-order change, so it deserves an explicit human yes.
  (docs/spikes/mobile-chrome-presentation-from-one-derivation/DECISIONS.md D2; crates/werust-core/src/lib.rs chrome_json; both rust/src/lib.rs now call werust_core::chrome_json)
- The measurement baseline is not actually the pre-change encoder, though MEASUREMENT.md and the example module doc both call it that. baseline_facts_only_json re-encodes the eleven facts with serde_json, whereas the deleted ffi_json twins used hand-rolled format! + escape. So the 'encode was' column and the +2.5 us delta measure the cost of the ten EXTRA FIELDS, not the real before/after of this commit, which also includes the encoder swap from D2. The conclusion (microseconds, event-driven cadence, not observable) is unaffected; should the wording be corrected to say 'facts-only, same encoder' rather than 'the pre-change encoder'?
  (crates/werust-core/examples/chrome_json_cost.rs baseline_facts_only_json; MEASUREMENT.md: 'it carries the pre-change facts-only encoder as a frozen baseline fixture')
- Ratify a user-visible accessibility default that D5 records only in passing: on both edges the badge's accessibility TEXT is now the ~240-character explanation (Android contentDescription, iOS accessibilityLabel), which REPLACES the short badge label. A screen-reader user therefore no longer hears the badge itself (e.g. the verified label) at all, only the sentence, on every focus. The idiomatic platform split (label = badge text, accessibilityHint / stateDescription = explanation) was not among the alternatives weighed. Sighted users are unaffected (the tap dialog is titled with the badge).
  (BrowserActivity.kt refreshChrome: trust.contentDescription = chrome.trustIndicatorDetail; WKWebViewShellController.swift: trustLabel.accessibilityLabel = chrome.trustIndicatorDetail; DECISIONS.md D5)
- The acceptance criterion 'the mobile edges still build' is not evidenced by the green gate, and this diff carries more Kotlin/Swift than usual (a 21-field data class, a new AlertDialog/UIAlertController path, an added core.chrome() call during initial layout). verify.yml is pure Rust; mobile-ios.yml triggers only on push to main or workflow_dispatch; the Android APK build lives in release.yml (tags). I hand-read both edges for compile-correctness and found nothing broken, but a manual workflow_dispatch of mobile-ios plus one Android assemble before merge would close the gap the source-shape guard cannot.
  (.github/workflows/verify.yml, mobile-ios.yml lines 21-28, release.yml android-apk job; MEASUREMENT.md claims 'the two mobile CI legs build and launch the edges')
