---
title: "Gate-3 conductor review: one-derivation-close-the-aggregate-and-tooltip-gaps (APPROVE)"
date: 2026-07-30
status: open
reviewOf: one-derivation-close-the-aggregate-and-tooltip-gaps
verdict: approve
---

## Verdict: APPROVE

Merged as `fed6725`. Gate-1 and Gate-2 green (5 non-blocking nits), local full gate green, and the `macos-renderer` leg is green on `main` (run 30578142612), which matters here because the change touches both painters.

## Acceptance criteria, ticked

- [x] **One aggregate over every family, exhaustive by construction.** `CssClassFamily` with `ALL` and a `const fn classes()`, both pinned by `_CSS_CLASS_FAMILY_ALL_IS_EVERY_FAMILY_IN_SLOT_ORDER`, the same anonymous-`const` total-`match` trick already used for `TrustPosture::ALL`. A family named but not listed fails to compile, and so does a family listed but not named.
- [x] **Both painters iterate it.** `crates/werust/src/main.rs` and `crates/werust-macos/src/paint.rs` now drive `CssClassFamily::ALL`; neither keeps a hand-written family list.
- [x] **`CHROME_CSS_CLASS_SETS` kept its narrower toggling meaning**, and no painter toggles console classes on a widget.
- [x] **The tooltip is composed once in core**, `load_progress_tooltip(state, stop_label)`, with the stop affordance PARAMETERISED rather than assumed. The core tests exercise both a `✕` and an `Esc` label, so the parameter is real rather than decorative.
- [x] Behaviour unchanged; gates green on Ubuntu and on the macOS leg.

## Nit triage (5 non-blocking findings)

**The one worth acting on eventually: the new tooth binds ENUM VARIANTS, not exported consts.** Adding a `CssClassFamily` variant cannot compile without joining `ALL` and `classes()`, which is the guarantee we wanted. But nothing forces a NEW exported const (say a future `FOO_CSS_CLASSES`) to become a variant at all: an author can export a family, never name it in the enum, and both coverage gates stay green while it paints invisibly. That is the same shape of hole one level up again, and it is now genuinely hard to close in the type system. My read: the honest fix is social rather than mechanical (a note beside the enum saying "a new class family is a variant HERE first"), unless someone wants to make the families a macro. Worth recording rather than silently inheriting.

**For the human, two ratifications.** The aggregate landed as a PUBLIC enum in `werust-core`, which is the surface the queued Windows and mobile painters will bind to, so it is cheaper to object now than later. And `STOP_AFFORDANCE_LABEL` now lives in CORE, which puts a UI affordance glyph slightly across the core-derives / edge-paints line this repo has been careful about, even though the label is parameterised at the call.

**Small residue:** the edge-wiring shape test dropped its assertion that the desktop shell consumes `CHROME_CSS_CLASS_SETS`, and as landed that `pub const` now has no consumer outside core's own tests (the toggle loops use the family enum). Either give it a consumer or retire it, so a public const is not left dangling.

**A process question the build raised, and it is a fair one:** `verify` runs bare `cargo clippy`, so lint debt in `cfg(test)` code never reds the gate (it names nine `field_reassign_with_default` in the macOS paint tests plus a copied pre-existing lint). Whether `verify` should be `cargo clippy --all-targets` is a repo-policy decision with a real cost: it would red the gate today until that debt is cleared.
