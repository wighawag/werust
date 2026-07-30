---
title: "Export the chrome's CSS-class SET from core so a painter cannot keep a stale class list (and no exported class can go unstyled)"
slug: export-the-chrome-css-class-set-from-core
blockedBy: []
covers: []
---

## What to build

A latent bug found at Gate-2/Gate-3 of `desktop-chrome-presentation-into-core` and ratified for fixing by the human on 2026-07-30. Fix it BEFORE a second painter exists, because a second painter is what makes it bite.

`desktop-chrome-presentation-into-core` moved the class-NAME decisions into the core (`trust_indicator_css_class` returns one of `trust-loading` / `trust-verified` / `trust-name-trusted-rpc` / `trust-mutable-name` / `trust-unverified`; `error_banner_css_class` returns `error-banner` or `error-banner-transient`). But the EXHAUSTIVE toggle lists stayed hard-coded in the GTK painter (`crates/werust/src/main.rs`, the `for class in [...]` loops in `Chrome::refresh`), and nothing ties the two together. The painter's correctness depends on its literal list containing EVERY class the core can return, because it toggles exactly one on and the rest off; a class missing from the list is a class that never gets cleared, so a stale badge colour lingers across a transition.

So today, adding a fifth trust posture in core silently leaves every painter with a stale list, with a green test suite. `macos-wkwebview-backend-and-window` and `windows-webview2-backend-and-window` will each add a painter, multiplying the same latent bug.

**The fix:** export the COMPLETE set from core next to the functions that produce it (a `pub const` slice of the class names, or an enum whose variants map to the names, whichever reads better beside the existing `*_css_class` functions), and have the GTK painter iterate THAT instead of a literal. Then pin it with tests.

**Two teeth are worth having, not one:**

1. **Exhaustiveness:** every class name a `*_css_class` function can return is in the exported set. The cheap way is to drive the function over every `TrustPosture` and every relevant `ChromeState` shape and assert the result is a member. Adding a posture without extending the set then reds the gate.
2. **No unstyled class:** every name in the exported set has a matching rule in the GTK stylesheet (`APP_CSS`). This catches the real user-visible failure mode of the first tooth's cousin: a new class that IS toggled correctly but has no styling, so the state renders invisibly. `APP_CSS` is a plain `&str` in the edge, so a string-containment assertion is enough.

**Keep the layering intact.** The class NAME is a derivation (core); the STYLESHEET is painting (edge). This task does not move `APP_CSS` into core, and it does not give core any notion of colour.

## Acceptance criteria

- [ ] The complete chrome CSS-class set is exported from `werust-core`, beside the `*_css_class` functions, and covers both families (the `trust-*` postures and the `error-banner*` severities).
- [ ] The GTK painter derives its toggle lists from the exported set rather than from hard-coded literals; behaviour is unchanged (exactly one class active at a time, no stale class across a transition).
- [ ] A test asserts EXHAUSTIVENESS: every value a `*_css_class` function can return is a member of the exported set, so adding a posture in core without extending the set fails the gate.
- [ ] A test asserts every exported class name has a rule in the GTK `APP_CSS`, so an exported-but-unstyled class fails the gate too.
- [ ] `APP_CSS` stays in the edge; core gains no styling concept.
- [ ] The debug view's `trust-*` reuse (its Network tab paints per-request posture with the SAME classes, ADR-0006) still works and is covered by the same guarantee.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Prompt

> Goal: close a latent bug before two more painters inherit it. The chrome's CSS class NAMES are now decided in `werust-core` (`trust_indicator_css_class`, `error_banner_css_class`), but the GTK painter still hard-codes the exhaustive `for class in [...]` toggle lists in `Chrome::refresh`, and nothing connects them: a class the core can return but the painter's literal omits is a class that never gets cleared, so a stale badge lingers. Export the complete class set from core beside those functions (a `pub const` slice or an enum, whichever reads better), have the painter iterate it, and pin it with TWO tests: exhaustiveness (every value a `*_css_class` function can return is in the set, so adding a posture without extending the set reds the gate) and no-unstyled-class (every exported name has a rule in the edge's `APP_CSS`, catching a correctly-toggled but invisible state). Keep the layering: the class name is a derivation in core, the stylesheet stays in the edge, and core gains no notion of colour. Behaviour must not change.

## Requeue 2026-07-30

Gate-2 BLOCK to fix (the block is correct, keep the rest of the work): the exhaustiveness tooth does not actually bite. The core test drives postures from a hand-written array literal, so adding a FIFTH TrustPosture variant does NOT force that list to grow: an author adds the arm to trust_indicator_css_class, forgets TRUST_INDICATOR_CSS_CLASSES, and the suite stays green while the painter finds no matching member, removes all five classes and adds none, so the new posture paints UNSTYLED. That is exactly acceptance criterion 3. Fix: make the drive list exhaustive BY CONSTRUCTION, i.e. a match over TrustPosture (in a core-side helper such as TrustPosture::ALL, or inside the test helper) so a new variant is a COMPILE error rather than a silently-green test; do the same for the error-banner severity axis if it has the same shape. Also (a) correct the false doc claim on every_chrome_state_shape that it is 'exhaustive by construction' when it is not, and (b) redo the DECISIONS.md mutation check so it exercises the NEW-VARIANT path, not merely a mutated existing branch. Note that a Phase-2 name-verified posture is already anticipated in the TrustPosture docs, so this is a likely path, not a hypothetical.
