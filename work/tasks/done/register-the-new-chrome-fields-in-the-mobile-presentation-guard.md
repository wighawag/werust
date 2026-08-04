---
title: "Close the loop: register the spinner + control-mode fields in the mobile presentation guard, so the new chrome facts are protected like every other one"
slug: register-the-new-chrome-fields-in-the-mobile-presentation-guard
spec: chrome-conventional-controls
blockedBy: [android-chrome-collapse-reload-stop-and-drop-history-buttons, ios-chrome-collapse-reload-stop-and-drop-history-buttons]
covers: [8, 10]
---

## What to build

The CONTRACT step of the expand -> migrate -> contract sequence that delivers the loading spinner and the collapsed reload/stop control to the mobile edges. Tiny, mechanical, and load-bearing.

`crates/werust-core/tests/mobile_chrome_presentation_shape.rs` is the guard that keeps the Kotlin and Swift chrome from re-growing its own copy of the core's derivation. It drives off two HARDCODED lists (`FACT_FIELDS` and `DERIVED_FIELDS`) and asserts that both mobile bindings DECODE every derived field and that both painters PAINT from them. That guard exists because those twins really did exist and really did drift (`mobile-chrome-presentation-from-one-derivation`).

The new fields added by `reload-stop-collapse-and-loading-spinner-core-and-gtk` (spinner visibility and the reload/stop control mode) are deliberately NOT in those lists yet, because registering them before both mobile edges consumed them would have failed the gate for three tasks running. Both edges have now consumed them, so this task registers them and the protection becomes real.

Until this lands, the new fields are the ONLY chrome facts crossing to mobile without guard coverage: an edge could quietly stop reading one, or restate its value locally, and nothing would go red.

## Acceptance criteria

- [ ] The spinner-visibility and control-mode fields are registered in the mobile presentation guard's field lists, in whichever of `FACT_FIELDS` / `DERIVED_FIELDS` matches their nature (a derived presentation value, not a raw fact, unless the implementation says otherwise).
- [ ] The guard's existing assertions now cover them: both mobile bindings decode each field, and both painters paint from it.
- [ ] The guard is unchanged in structure and strength: this task ADDS entries, it does not relax, special-case or restructure the checks.
- [ ] Deliberately verify the guard has TEETH for the new entries: temporarily removing one edge's use of a new field must red the gate. Confirm it, then restore.
- [ ] The full gate is green.
- [ ] Tests network-isolated; mirror the repo's existing test style.

## Blocked by

- `android-chrome-collapse-reload-stop-and-drop-history-buttons` — the Android edge must consume the fields before the guard can require it.
- `ios-chrome-collapse-reload-stop-and-drop-history-buttons` — likewise for the iOS edge. This task is the fan-in of both.

## Prompt

> Goal: register the spinner-visibility and reload/stop control-mode fields in the mobile presentation guard, completing the expand -> migrate -> contract sequence that brought those chrome facts to the mobile edges.
>
> Read `crates/werust-core/tests/mobile_chrome_presentation_shape.rs` first. It is the guard that keeps the Kotlin and Swift chrome reading the core's ONE derivation instead of re-deriving it, and it works off two hardcoded field lists plus assertions that both bindings decode and both painters paint every derived field. The task `reload-stop-collapse-and-loading-spinner-core-and-gtk` deliberately left the new fields out of those lists, because registering them before the mobile edges consumed them would have failed the gate for three tasks; both edges have since consumed them, which is why this task is blocked on both.
>
> Add the entries. Do NOT relax, restructure or special-case the guard: it exists because the Kotlin and Swift twins genuinely drifted once (`mobile-chrome-presentation-from-one-derivation`), and its strength is the whole point.
>
> Then prove the new entries have TEETH rather than assuming it: temporarily remove one edge's use of one new field and confirm the gate goes red, then restore. A guard entry that cannot fail is worse than none, because it reads as protection. The repo has a task in flight on exactly this class of mistake (`macos-harness-guard-teeth-and-paint-path-residue`), so it is a known local failure mode.
>
> FIRST, check this task against current reality (it is a launch snapshot and may have DRIFTED): confirm the two fields exist in the chrome JSON with the names the earlier task gave them, that both mobile edges really read them, and that the guard still uses the hardcoded-list shape described here. If a sibling task already registered them, this task is done and should say so rather than duplicating.

## FORWARD-POINTER (planted by the drive-tasks conductor, after the expand step landed)

> Registering `FACT_FIELDS` / `DERIVED_FIELDS` is NOT the whole contract. The guard in
> `crates/werust-core/tests/mobile_chrome_presentation_shape.rs` has a THIRD list that this
> task's acceptance criteria do not currently name: `every_derived_string()`, the
> forbidden-literal set a mobile edge must never hardcode.
>
> `every_derived_string()` is exhaustive over the ENUM AXES (`TrustPosture::ALL`,
> `LoadStep::ALL`) but it names the RULES it drives BY HAND (`trust_indicator`,
> `trust_indicator_detail`, `load_progress_hint`, `trust_pin_action_label`,
> `invalid_entry_badge_text`). The new `ReloadStopControl::label()` /
> `ReloadStopControl::description()` strings are NOT on that list, so today a Kotlin or
> Swift edge could hardcode "⟳", "Reload this page" or "Stop loading this page" and the
> guard would stay GREEN. That is exactly the twin-drift this guard exists to stop.
>
> So this task must ALSO add the new presentation rules to `every_derived_string()`, not
> just the two field lists. Registering the fields while leaving the literal half unguarded
> would close the fan-in only halfway.
>
> Full detail: `work/notes/observations/mobile-guard-forbidden-literals-are-a-hand-picked-rule-list-2026-08-04.md`.
>
> The four chrome JSON keys awaiting registration are `reloadStopControl`,
> `reloadStopControlLabel`, `reloadStopControlDescription` and `loadSpinnerVisible`.
> This task is the ONE place the guard may be edited; the sibling tasks are forbidden to touch it.


---

### Claiming this task

```sh
dorfl claim register-the-new-chrome-fields-in-the-mobile-presentation-guard --arbiter origin
git fetch origin && git switch -c work/register-the-new-chrome-fields-in-the-mobile-presentation-guard origin/main
git mv work/tasks/ready/register-the-new-chrome-fields-in-the-mobile-presentation-guard.md work/tasks/done/register-the-new-chrome-fields-in-the-mobile-presentation-guard.md
```

## Gate-3 conductor verdict (drive-tasks)

APPROVE, first attempt. This is the fan-in that CLOSES the guard gap opened by the expand step; it is the one task in the set permitted to edit the guard, and it strengthened it rather than weakening it.

- `DERIVED_FIELDS` now carries `reloadStopControlLabel`, `reloadStopControlDescription` and `loadSpinnerVisible`. MET.
- The wire name `reloadStopControl` is deliberately EXCLUDED, and the reasoning is sound rather than an omission: this list is what every edge must DECODE and PAINT, and the wire name's only tempting consumer is precisely the `when`/`switch` this whole collapse deletes. Both mobile edges forbid reading it in their own guards (`no_kotlin_conditional_decides_the_control_mode_or_the_spinner` and its Swift twin), so requiring it here would demand the very wiring the repo forbids. Its values are carrier vocabulary, not presentation strings.
- The FORWARD-POINTER planted by the conductor was honoured, and improved on. The concern was that `every_derived_string()` names the RULES it drives BY HAND, so the new strings could be left unguarded while the field lists looked complete. The task did not merely hand-add the three strings; it drives them from the enum:

      for control in ReloadStopControl::ALL {
          produced.push(control.label().to_string());
          produced.push(control.description().to_string());
      }

  `ReloadStopControl::ALL` is kept complete by a compile-time check, exactly like the `TrustPosture` and `LoadStep` axes, so a re-drawn glyph or a reworded description joins the forbidden-literal list WITHOUT anyone remembering to. That is a better fix than the note asked for: it converts a hand-picked rule into an exhaustive axis.

- The both-edges-consume mechanism is intact and is what proves the migrate steps were honest: the gate is green WITH the fields registered, which it could only be if both the Kotlin and the Swift edge actually decode and paint them.

`rust-toolchain.toml` NOT touched. Conventional-commit subject.

### The guard gap is CLOSED

Between the expand step (`reload-stop-collapse-and-loading-spinner-core-and-gtk`) and this commit, the new chrome facts crossed to both mobile edges with no guard coverage, and a hardcoded `⟳` / "Reload this page" / "Stop loading this page" in Kotlin or Swift would have stayed green. From this commit on, both the FIELD half and the LITERAL half are protected, so the Kotlin and Swift chrome twins cannot drift back.

Six non-blocking Gate-2 nits: `work/notes/observations/review-nits-register-the-new-chrome-fields-in-the-mobile-presentation-guard-2026-08-04.md`.
