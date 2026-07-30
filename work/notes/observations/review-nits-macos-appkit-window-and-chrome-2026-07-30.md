---
title: review-gate non-blocking nits for 'macos-appkit-window-and-chrome' (Gate 2 approve)
date: 2026-07-30
status: open
reviewOf: macos-appkit-window-and-chrome
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'macos-appkit-window-and-chrome' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify or file a follow-up: the URL-bar progress TOOLTIP composition is now duplicated verbatim in two edges instead of being extracted to core. crates/werust-macos/src/paint.rs builds format!({hint}... - press Stop (X) to cancel) with the same comment and logic as crates/werust/src/main.rs. It is the same duplication class this task extracted the debug-row helpers for (a user-visible string that will silently drift if one edge is reworded), and it is not listed in DECISIONS.md. Should a load_progress_tooltip rule move into werust-core?
  (crates/werust-macos/src/paint.rs ChromePaint::of (progress_tooltip) vs crates/werust/src/main.rs around line 877)
- Ratify decision 3 plus its residue: DEBUG_CONSOLE_CSS_CLASSES is exported as a sibling family and deliberately NOT folded into CHROME_CSS_CLASS_SETS (sound reasoning: chrome families are toggled on one widget, console classes colour a row). The cost is that no aggregate covers every exported family any more, so each edge guard hand-chains them (GTK chains CHROME_CSS_CLass_SETS + DEBUG_CONSOLE; macOS lists three families literally). A future family added in core can land unstyled on an edge that forgets to chain it, with a green gate. Worth an ALL_CSS_CLASS_SETS aggregate?
  (crates/werust-core/src/debug.rs DEBUG_CONSOLE_CSS_CLASSES doc; crates/werust/src/main.rs every_chrome_css_class_the_core_exports_has_a_rule_in_the_app_css; crates/werust-macos/src/paint.rs every_exported_class_has_a_colour)
- Evidence precision: the spike README attributes a specific environment and duration to run 30572253620 (macOS 14.8.7 build 23J520, Xcode 15.4 build 15F31d, AppleWebKit/605.1.15, all steps succeeded in 1m01s). The same OS/WebKit values are already recorded in the repo from the ENGINE run 30563185521, and the conductor handoff that supplied this run's evidence quoted only the window_smoke transcript. Can the human confirm these were read from THIS run's own Record what this run measured on step, rather than carried over? Everything load-bearing (the leg is green, the smoke passed) is independently confirmed by the handoff.
  (docs/spikes/macos-appkit-window-and-chrome/README.md, What CI proved; the same values appear in docs/spikes/macos-wkwebview-renderer-backend/README.md and expected.json for run 30563185521)
- Small accuracy nit in the evidence sentence: the README says the CI-d commit d9aeca8 differs from the landed tree only in work/ bookkeeping files. It also differs in this README itself and in DECISIONS.md items 6 and 11 (the honesty corrections). The material half of the claim is verified true: no source line differs.
  (git diff d9aeca8..HEAD touches docs/spikes/.../README.md, DECISIONS.md and two work/ files only)
- Cross-task interaction to ratify: the macos-renderer workflow now also triggers on PULL REQUESTS touching crates/werust-core/**, which was previously only a push filter. Every future PR that touches core (most chrome/task work) now spends macos-14 runner minutes and can be gated by a red macOS leg. Defensible (core changes reach the window) but it changes other tasks CI, and DECISIONS 10 records only that the workflow was extended rather than forked.
  (.github/workflows/macos-renderer.yml, pull_request paths gains crates/werust-core/**)
