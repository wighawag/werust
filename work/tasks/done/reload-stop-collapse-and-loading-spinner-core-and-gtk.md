---
title: "Reload and Stop become ONE control, and a spinner joins the URL-bar progress: the derivation in core (through BOTH carriers) plus the GTK painter"
slug: reload-stop-collapse-and-loading-spinner-core-and-gtk
spec: chrome-conventional-controls
blockedBy: [shortcut-resolution-in-core-and-the-gtk-edge]
covers: [8, 9, 10, 14]
---

## What to build

The second tracer bullet: two new derived chrome facts in the toolkit-free core, exported through BOTH carriers so every edge can read them, plus the GTK painter proving them end to end.

**The control mode.** Every edge currently carries a separate Reload button and Stop button, with Stop enabled only while a load is in flight and Reload only when it is not. Browsers collapse those into ONE control that reloads when idle and stops while loading. Derive that mode from the same `ChromeState::is_loading` the progress bar already reads, so no edge decides it.

**The spinner.** There is no spinner anywhere in werust today. The URL-bar progress bar stays exactly as it is (it is liked, and it is the fine-grained signal); the spinner is a SECOND presentation of the same `is_loading` fact, not a second source of truth, and it covers the case the progress bar cannot: a load that stalls before any progress is reported is currently indistinguishable from an idle browser.

**Both carriers, or the mobile edges cannot follow.** This repo has exactly two carriers from the one derivation, chosen by what can cross the boundary: the plain-Rust snapshot in `crates/desktop-paint` (consumed by the AppKit and Win32 painters) and the chrome JSON (decoded by the Kotlin and Swift edges). Both must carry the new facts, because the sibling tasks for the other four edges consume one or the other, and a mobile edge that has to run its own conditional is exactly the twin this repo has already deleted once.

**Do not lose the cancel path.** The separate Stop button is currently the documented cancel affordance. After the collapse, cancel must still be reachable, including from the keyboard (Escape with the page focused, from the shortcut task).

**This task is the EXPAND step of an expand -> migrate -> contract sequence, and it must NOT take the contract step.** The mobile presentation guard (`crates/werust-core/tests/mobile_chrome_presentation_shape.rs`) drives off two HARDCODED lists, `FACT_FIELDS` and `DERIVED_FIELDS`, and asserts that BOTH mobile bindings decode every derived field and that both painters paint from them. So registering the new fields in `DERIVED_FIELDS` here would red the gate immediately and keep it red until the Android and iOS tasks land three tasks later, i.e. this task could not pass its own gate. Add the fields to the core derivation and to both carriers, and leave the guard lists ALONE. Registering them is owned by exactly one later task, `register-the-new-chrome-fields-in-the-mobile-presentation-guard`, which is blocked on both mobile edges consuming them. Do not weaken or special-case the guard to work around this.

## Acceptance criteria

- [ ] The reload/stop control mode is a derived value in the toolkit-free core, computed from the existing loading fact; no edge computes it.
- [ ] Spinner visibility is a derived value from the same fact; the URL-bar progress bar's existing behaviour is unchanged.
- [ ] Both carriers export the new facts: the `desktop-paint` snapshot and the chrome JSON.
- [ ] The mobile presentation guard's `FACT_FIELDS` / `DERIVED_FIELDS` lists are NOT touched (that is the contract step, owned by `register-the-new-chrome-fields-in-the-mobile-presentation-guard`), and the guard is not weakened or special-cased to accommodate this task.
- [ ] The gate is green with THIS task alone, without the mobile edges having been updated.
- [ ] The GTK painter shows ONE control that reloads when idle and stops while loading, plus the spinner, reading only the derived values.
- [ ] The GTK back and forward buttons are untouched (desktop keeps them, per the spec).
- [ ] Cancelling an in-flight load is still possible, and still works from the keyboard.
- [ ] The existing guard style holds: a painter reads exported values rather than re-deriving them, and the carriers agree with the core (assert it, as the repo already does for the palette and the chrome JSON).
- [ ] Tests cover the derived values across loading and idle states, with no display required.
- [ ] Tests network-isolated; mirror the repo's existing test style.

## Blocked by

- `shortcut-resolution-in-core-and-the-gtk-edge` — no logical dependency, but both tasks edit the GTK desktop binary's window construction, so they are serialised to avoid a merge conflict (the runner rebases or surfaces conflicts, it never auto-resolves).

## Prompt

> Goal: collapse Reload and Stop into one control and add a loading spinner, deriving both in the toolkit-free core and painting them on the GTK edge.
>
> Read `work/specs/tasked/chrome-conventional-controls.md` first. The relevant existing derivation lives beside `load_progress_visible` / `load_progress_fraction` / `load_progress_hint` in `werust-core`, all pure functions of `ChromeState`; add the control mode and spinner visibility there, in the same style, from the same `is_loading` fact. The URL-bar progress bar is the existing loading surface (the entry paints its own progress fraction) and must not change.
>
> CRITICAL: export through BOTH carriers. `CONTEXT.md` ("chrome presentation / painter") explains the rule: one derivation, two carriers, chosen per edge by what can cross the boundary. `crates/desktop-paint` is the plain-Rust snapshot the AppKit and Win32 painters read; `werust_core::chrome_json` is what the Kotlin and Swift edges decode. Four sibling tasks consume these, so a carrier you skip is an edge that cannot follow, and a mobile edge running its own `when`/`switch` is the exact twin this repo already deleted (`mobile-chrome-presentation-from-one-derivation`) and guards against.
>
> The separate Stop button is currently the documented cancel affordance, so verify cancel survives the collapse, including its keyboard route.
>
> SEQUENCING, and the trap to avoid: this is the EXPAND step. `crates/werust-core/tests/mobile_chrome_presentation_shape.rs` enumerates the guarded fields in two hardcoded lists and asserts both mobile edges decode and paint every derived one. Adding your new fields to `DERIVED_FIELDS` now would fail the gate until the Android and iOS tasks land, so DO NOT: add the fields to the derivation and both carriers, leave the guard lists untouched, and let `register-the-new-chrome-fields-in-the-mobile-presentation-guard` (blocked on both mobile edges) close the loop. Weakening the guard instead is the wrong fix: it is the mechanism that stopped the Kotlin/Swift chrome twins from drifting.
>
> FIRST, check this task against current reality (it is a launch snapshot and may have DRIFTED): confirm the loading fact and progress derivations are still shaped as described and that no spinner has landed meanwhile.
>
> RECORD non-obvious in-scope decisions durably and link them from the done record. The likely one: where the spinner sits relative to the collapsed control and the trust badge, which the four per-edge sibling tasks will follow.

---

### Claiming this task

```sh
dorfl claim reload-stop-collapse-and-loading-spinner-core-and-gtk --arbiter origin
git fetch origin && git switch -c work/reload-stop-collapse-and-loading-spinner-core-and-gtk origin/main
git mv work/tasks/ready/reload-stop-collapse-and-loading-spinner-core-and-gtk.md work/tasks/done/reload-stop-collapse-and-loading-spinner-core-and-gtk.md
```

## Gate-3 conductor verdict (drive-tasks)

APPROVE, first attempt. Gate 1 and Gate 2 both green; reviewed the merged diff against each criterion.

- Control mode derived in the toolkit-free core: `ReloadStopControl` + `reload_stop_control(state)` in `werust-core`. The GTK edge CALLS it, never recomputes it. MET.
- Spinner derived from the same loading fact: `load_spinner_visible(state)`; the URL-bar progress bar is untouched. MET.
- BOTH carriers export the new facts. `desktop-paint`: `reload_stop_control`, `reload_stop_label`, `reload_stop_description`, `spinner_visible`. chrome JSON: `reloadStopControl`, `reloadStopControlLabel`, `reloadStopControlDescription`, `loadSpinnerVisible`. MET.
- The mobile presentation guard is NOT touched and NOT weakened: `crates/werust-core/tests/mobile_chrome_presentation_shape.rs` is absent from the diff entirely. MET (this was the explicit conductor watch-item).
- Gate green with this task alone, mobile edges not yet updated. MET.
- GTK painter shows ONE control plus the spinner, reading only derived values: single `reload_stop: Button` whose icon and tooltip come from `reload_stop_control(state)`, plus a `Spinner`. MET.
- Back and forward buttons untouched: `back: Button` / `forward: Button` still driven by `can_go_back` / `can_go_forward`. MET.
- Cancelling an in-flight load still works, including from the keyboard: the control's click handler resolves `reload_stop_control(...).action()` into the shared `ChromeAction` vocabulary and calls `perform_chrome_action`, the same path Escape-with-page-focus takes from `shortcut-resolution-in-core-and-the-gtk-edge`. MET.
- Carriers assert agreement with the core (e.g. `assert_eq!(paint.reload_stop_control, reload_stop_control(state))`). MET.

`rust-toolchain.toml` NOT touched. Conventional-commit subject.

### Conductor note: the guard exposure this OPENS

Four chrome JSON keys now cross to the Kotlin and Swift edges with NO guard coverage: `reloadStopControl`, `reloadStopControlLabel`, `reloadStopControlDescription`, `loadSpinnerVisible`. That is by design for this expand step, and it CLOSES only when `register-the-new-chrome-fields-in-the-mobile-presentation-guard` lands. Until then the guard silently under-protects the new fields.

Six non-blocking Gate-2 nits are in `work/notes/observations/review-nits-reload-stop-collapse-and-loading-spinner-core-and-gtk-2026-08-04.md`. The agent also filed `mobile-guard-forbidden-literals-are-a-hand-picked-rule-list-2026-08-04.md`, which is load-bearing for the fan-in task; a forward-note has been planted there.
