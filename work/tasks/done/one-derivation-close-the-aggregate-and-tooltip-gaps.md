---
title: "Make the class-coverage gates exhaustive over FAMILIES by construction, and unify the URL-bar progress tooltip into core"
slug: one-derivation-close-the-aggregate-and-tooltip-gaps
blockedBy: []
covers: []
---

## What to build

Two residues of the shared-derivation work, both found at Gate-2/Gate-3 of `macos-appkit-window-and-chrome` and both ratified by the human on 2026-07-30. Same theme: ONE derivation, every painter.

### Part 1 — the coverage gates are exhaustive over CLASSES but not over FAMILIES

`export-the-chrome-css-class-set-from-core` gave each exported family a real tooth: a new `TrustPosture` cannot be added without reddening the gate, because the drive lists are exhaustive BY CONSTRUCTION (`TrustPosture::ALL` and friends, each pinned by a compile-time total `match`).

That guarantee does NOT extend to the set of families itself. Both painters hard-code which families they check:

- `crates/werust/src/main.rs`, `every_chrome_css_class_the_core_exports_has_a_rule_in_the_app_css`, iterates `CHROME_CSS_CLASS_SETS.iter().copied().chain([DEBUG_CONSOLE_CSS_CLASSES])`.
- `crates/werust-macos/src/paint.rs`, `every_exported_class_has_a_colour`, iterates a literal `[TRUST_INDICATOR_CSS_CLASSES, ERROR_BANNER_CSS_CLASSES, DEBUG_CONSOLE_CSS_CLASSES]`.

So a SIXTH exported family (a future posture group, a network-row severity, whatever) joins neither gate, both suites stay green, and the new family renders invisibly on BOTH platforms. That is exactly the failure the first task existed to prevent, one level up.

**Fix:** export from `werust-core` a single aggregate over EVERY exported class family, made exhaustive by construction with the same trick already used in this repo (a total `match` in an anonymous `const` block that refuses to compile until a new family is named in it, as `_TRUST_POSTURE_ALL_IS_EVERY_POSTURE_IN_SLOT_ORDER` does for postures). Both painters then iterate the aggregate instead of their own literal, so a new family automatically joins the GTK stylesheet gate and the macOS palette gate.

**Preserve the distinction that already exists.** `DEBUG_CONSOLE_CSS_CLASSES` is deliberately NOT a member of `CHROME_CSS_CLASS_SETS`, and that reasoning is sound: `CHROME_CSS_CLASS_SETS` is what a chrome painter TOGGLES on one widget (exactly one on, the rest off), while console classes colour a row. The new aggregate is for COVERAGE gates only. Do not merge the two concepts, and do not let a painter start toggling console classes on a widget.

### Part 2 — the progress tooltip is duplicated verbatim in two edges

`crates/werust/src/main.rs` and `crates/werust-macos/src/paint.rs` both build `format!("{hint}… — press Stop (✕) to cancel")`, with the same surrounding logic and near-identical comments. It is a pure function of `ChromeState`, so it belongs beside the other `load_progress_*` rules in core, and both edges should call it. Two copies is how the Kotlin and Swift twins started.

**One nuance to handle rather than ignore:** the sentence names a UI affordance (the Stop control and its glyph). If a painter's stop affordance is labelled differently, the core function should take that label as a parameter rather than let the edge fork the sentence. Check what each edge actually shows before choosing; both currently use `✕`.

While you are there, check whether the same has happened to any other `load_progress_*` consumer, and say what you found either way.

## Acceptance criteria

- [ ] `werust-core` exports ONE aggregate over every exported CSS-class family, kept exhaustive by a compile-time check, so adding a family without listing it fails to compile.
- [ ] Both the GTK stylesheet gate and the macOS palette gate iterate that aggregate; neither keeps a hand-written family list.
- [ ] A new family added in core (try it during development, then revert) reds BOTH gates rather than passing silently.
- [ ] `CHROME_CSS_CLASS_SETS` keeps its narrower toggling meaning; the aggregate does not replace it and no painter toggles console classes on a widget.
- [ ] The URL-bar progress tooltip is composed ONCE in `werust-core` beside the other `load_progress_*` rules, and both edges call it; the stop-affordance label is parameterised if the edges differ.
- [ ] Behaviour unchanged: the same tooltip text appears on desktop and macOS as today.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green. If the macOS half is touched in a way the Ubuntu gate cannot compile, dispatch the `macos-renderer` leg at your work branch (`gh workflow run macos-renderer.yml --ref <branch>` — the workflow is on `main`) and record the run; do NOT ship a prediction.

## Prompt

> Goal: close the last two duplication gaps in the shared chrome derivation. FIRST, the class-coverage gates are exhaustive over CLASSES but not over FAMILIES: `crates/werust/src/main.rs` and `crates/werust-macos/src/paint.rs` each hard-code which exported families they check, so a sixth family would join neither gate and paint invisibly on both platforms while both suites stay green. Export ONE aggregate over every family from `werust-core`, exhaustive BY CONSTRUCTION using the same anonymous-`const` total-`match` trick this repo already uses for `TrustPosture::ALL`, and have both painters iterate it. Keep `CHROME_CSS_CLASS_SETS`'s narrower meaning (what a painter TOGGLES on one widget, exactly one on): the aggregate is for coverage gates only. SECOND, the URL-bar progress tooltip (`"{hint}… — press Stop (✕) to cancel"`) is duplicated verbatim in both edges; move it beside the other `load_progress_*` rules in core and have both call it, parameterising the stop-affordance label if the edges differ. Behaviour must not change. If you touch macOS code the Ubuntu gate cannot compile, dispatch `macos-renderer.yml` at your branch and record the run rather than predicting it.
