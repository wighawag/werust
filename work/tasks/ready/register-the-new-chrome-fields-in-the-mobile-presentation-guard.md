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

---

### Claiming this task

```sh
dorfl claim register-the-new-chrome-fields-in-the-mobile-presentation-guard --arbiter origin
git fetch origin && git switch -c work/register-the-new-chrome-fields-in-the-mobile-presentation-guard origin/main
git mv work/tasks/ready/register-the-new-chrome-fields-in-the-mobile-presentation-guard.md work/tasks/done/register-the-new-chrome-fields-in-the-mobile-presentation-guard.md
```
