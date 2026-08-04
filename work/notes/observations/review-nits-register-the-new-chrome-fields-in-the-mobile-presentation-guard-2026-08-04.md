---
title: review-gate non-blocking nits for 'register-the-new-chrome-fields-in-the-mobile-presentation-guard' (Gate 2 approve)
date: 2026-08-04
status: open
reviewOf: register-the-new-chrome-fields-in-the-mobile-presentation-guard
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'register-the-new-chrome-fields-in-the-mobile-presentation-guard' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the fourth chrome-JSON key named by the task forward-pointer, reloadStopControl (the mode wire name), was deliberately NOT registered in either field list, so only the two per-edge guards forbid an edge branching on it; a future third mobile edge would have no central protection. Is that the intended contract?
  (docs/spikes/register-the-new-chrome-fields-in-the-mobile-presentation-guard/DECISIONS.md section 1; DERIVED_FIELDS requires decode AND paint, while the per-edge guards assert the painter never touches chrome.reloadStopControl, so registering it would demand exactly what the siblings forbid. Verified: neither WerustCore.kt nor WerustCore.swift decodes it.)
- Ratify: the sequencing assertion in two ALREADY-LANDED sibling tasks (android + ios collapsed_control_and_dropped_history_buttons_shape.rs) was inverted from the_mobile_presentation_guard_field_lists_are_not_registered_here into the_mobile_presentation_guard_registers_the_fields_this_edge_consumes, rather than deleted. Cross-task edit of files this task did not own.
  (DECISIONS.md section 2. The old assertion fires by design once registration lands, so a green gate was impossible without touching it; the inverted form now demands an exact DERIVED_FIELDS literal. Reasonable, but it is a coupling reversal in another task's file.)
- The newly INVERTED per-edge assertion is argued to have teeth but was not mutation-checked: the spike README mutates only edge sources, and the red it reports (both sequencing tests failing on registration) is the PRE-inversion direction. Was removing a DERIVED_FIELDS entry actually observed to red both edge suites?
  (README section on the teeth check lists three mutations, all on Kotlin/Swift edge sources. This repo caught a vacuous assertion of exactly this class in the Android sibling review (android DECISIONS section 7), so the class is a known local failure mode. By construction it looks sound: the field names appear in the guard only as DERIVED_FIELDS literals, and scan strips comments.)
- Stale claim left in the Android spike README: its Mutation-checked paragraph still says registering loadSpinnerVisible in DERIVED_FIELDS early turns the suite red. After this change that mutation is exactly what makes it GREEN, and unlike the DECISIONS sections it carries no superseded banner.
  (docs/spikes/android-chrome-collapse-reload-stop-and-drop-history-buttons/README.md, Mutation-checked paragraph. Both sibling READMEs also keep the phrasing that the field lists are NOT registered yet with a trailing correction clause, which reads self-contradictory.)
- The stated rationale for demanding an exact literal match instead of contains is inaccurate: it says the field names occur in the guard comments and a comment is not a registration, but scan() strips comments entirely and never collects their text as literals, so a comment mention could never satisfy a contains check either. Harmless (exact is stricter) but the wrong reason invites a future revert to contains.
  (crates/werust-android/rust/tests/collapsed_control_and_dropped_history_buttons_shape.rs around line 526 and the iOS twin; scan() skips from // to end of line before any literal collection.)
- STOP_AFFORDANCE_LABEL is the single glyph U+2715, now a forbidden SUBSTRING in every literal of the four scanned mobile sources, yet the same glyph is the app generic close affordance elsewhere. If that close button ever moves into a scanned file, the guard reds with a misleading message about restating the core derivation.
  (crates/werust-core/src/lib.rs:851; DebugView.kt:122 sets text to that glyph today and is not in every_mobile_source(). Current occurrences in BrowserActivity.kt / WKWebViewShellController.swift are comment-only, so the gate is unaffected now.)
