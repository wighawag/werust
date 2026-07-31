---
title: "Retire `CHROME_CSS_CLASS_SETS`: no edge consumes it, and an exported constant nobody binds to is a liability the next painter will copy"
slug: retire-the-unconsumed-chrome-css-class-sets-aggregate
blockedBy: []
covers: []
---

## What to build

Ratified by the human on 2026-07-31, from the conductor's drive report: `CHROME_CSS_CLASS_SETS` has no consumer outside core's own tests and doc comments, so it is either given one or retired. The decision is RETIRE.

**The evidence.** A repo-wide search for the symbol finds it only in `crates/werust-core/src/lib.rs` (its definition, two doc references, and its own unit tests), `crates/werust-core/src/debug.rs` (a doc reference) and `crates/werust-core/tests/chrome_css_class_set_edge_wiring_shape.rs` (a shape test naming the string). No Rust edge, no Kotlin, no Swift binds to it. Meanwhile the three shells that landed during the same period — GTK, AppKit and Win32 — all bind to `CssClassFamily`, the aggregate that came later and is exhaustive over families by construction.

**Do NOT delete it blind: it has a documented NARROWER meaning that must be dealt with first.** `lib.rs` says explicitly that `CssClassFamily` is "NOT a replacement for `CHROME_CSS_CLASS_SETS`, which keeps its NARROWER" meaning (the classes that TOGGLE, as opposed to the complete set of every class in every family), and `lib.rs`'s own tests lean on that distinction. So the real work is deciding what happens to that distinction:

- If the toggling-versus-complete distinction is still load-bearing for a test or a rule, MOVE it onto `CssClassFamily` (a method or an associated set) so there is one aggregate with two views, and point the existing tests at it.
- If it turns out the distinction is no longer used by anything that matters, say so plainly and let it go with the constant.

Either way, END with one aggregate that every painter binds to, not two where the second exists only to explain the first. Update the doc comments in `lib.rs` and `debug.rs` that reference the retired name, and update or retire `chrome_css_class_set_edge_wiring_shape.rs` — that shape test asserts an edge-wiring contract for a constant no edge wires to, which is the clearest single symptom of the problem.

**Why this is worth doing rather than leaving a harmless unused constant.** It is not harmless. It is PUBLIC, it is documented as the thing painters should iterate, and the next platform edge (or the next contributor writing a fourth painter) will reasonably bind to it, at which point the repo has two competing aggregates with different coverage and a real drift risk in exactly the surface — trust and chrome CSS classes — where drift has already cost this project the desktop-only trust explanation.

**Check the release-note angle:** this removes a `pub` item from `werust-core`. There is no external consumer of that crate today, but say so in the commit rather than assuming it silently.

## Acceptance criteria

- [ ] `CHROME_CSS_CLASS_SETS` no longer exists as a public constant, and no doc comment refers a reader to it.
- [ ] The toggling-versus-complete distinction it carried is either preserved on `CssClassFamily` (with its tests repointed) or explicitly recorded as no longer needed, with the reason.
- [ ] `chrome_css_class_set_edge_wiring_shape.rs` is updated or retired so that no shape test asserts an edge-wiring contract for a symbol no edge wires to.
- [ ] Every existing class-coverage guarantee still holds: nothing that was exhaustive-by-construction becomes a hand-maintained list.
- [ ] The commit notes that a `pub` item was removed from `werust-core` and that no consumer outside the repo is known.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Prompt

> Goal: retire `CHROME_CSS_CLASS_SETS` (ratified by the human, 2026-07-31). It is `pub`, it is documented as the thing painters should iterate, and NOTHING outside core's own tests and doc comments consumes it — the three shells that landed recently (GTK, AppKit, Win32) all bind to `CssClassFamily` instead. Do not delete it blind: `lib.rs` documents that `CssClassFamily` is "NOT a replacement", because this constant carries the NARROWER toggling-classes meaning, and core's tests lean on that. So first decide the distinction's fate — move it onto `CssClassFamily` as a second view (repointing the tests) if it is still load-bearing, or record plainly that nothing needs it any more — then remove the constant, fix the doc comments in `lib.rs` and `debug.rs` that point at it, and update or retire `crates/werust-core/tests/chrome_css_class_set_edge_wiring_shape.rs`, which currently asserts an edge-wiring contract for a symbol no edge wires to. End with ONE aggregate every painter binds to. Keep every coverage guarantee exhaustive-by-construction: nothing may regress into a hand-maintained list. Note in the commit that this removes a `pub` item from `werust-core` and that no external consumer is known.
