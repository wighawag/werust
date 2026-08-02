---
title: "Android: collapse Reload/Stop into one control, add the spinner, and DROP the on-screen back/forward buttons (the hardware Back already navigates history)"
slug: android-chrome-collapse-reload-stop-and-drop-history-buttons
spec: chrome-conventional-controls
blockedBy: [reload-stop-collapse-and-loading-spinner-core-and-gtk]
covers: [8, 9, 10, 11, 12]
---

## What to build

The Android toolbar gets the collapse, the spinner, and loses two buttons.

The activity currently builds four toolbar buttons (back, forward, reload, stop) plus the URL bar. On a phone that toolbar is the width-starved surface in the whole product, and two of those four buttons duplicate an affordance the platform already provides: the SYSTEM Back button (hardware or gesture) is already wired to page history through the non-deprecated back-pressed dispatcher, landed by `android-hardware-back-button-navigates-history` after a field report that it did not navigate history. So the on-screen back and forward buttons go, and Reload/Stop collapse into the single control the core derives, leaving the URL bar the width.

**What must NOT change:** `ChromeState::can_go_back` / `can_go_forward`, the `Renderer` seam's history methods, and the system-Back wiring. This task removes BUTTONS, not the capability: the hardware Back rides on exactly those, and so will the desktop shortcuts.

**Read the derived values, do not recompute them.** The spinner's visibility and the control's mode arrive in the chrome JSON this edge already decodes each refresh. A Kotlin `when` deciding either is the twin this repo has already deleted once and now guards against.

Forward navigation has no gesture equivalent on Android, so dropping the forward button removes the only way to go forward on this edge. That is accepted (the spec's mobile stories say so): forward is rare, and the width is worth more.

## Acceptance criteria

- [ ] The Android toolbar no longer shows on-screen back or forward buttons.
- [ ] The system Back button still navigates page history, exactly as before (assert it, do not assume: this behaviour was a field-reported bug once already).
- [ ] Reload and Stop are ONE control whose mode comes from the chrome JSON's derived value.
- [ ] A spinner shows while loading, its visibility read from the chrome JSON.
- [ ] No Kotlin conditional decides the control mode or the spinner's visibility (the guard against re-minted twins still holds).
- [ ] The mobile presentation guard's field lists are NOT touched here. This task is a MIGRATE step: it makes the Kotlin edge consume the new fields. Registering them in the guard is the fan-in task `register-the-new-chrome-fields-in-the-mobile-presentation-guard`, which is blocked on this task and the iOS one, because the guard requires BOTH edges to consume a field before it can demand it.
- [ ] `can_go_back` / `can_go_forward` and the history seam are unchanged; only the painter changes.
- [ ] Cancelling an in-flight load is still possible.
- [ ] Tests network-isolated; mirror the repo's existing test style, including its Kotlin-source shape assertions.

## Blocked by

- `reload-stop-collapse-and-loading-spinner-core-and-gtk` — it adds the derived control mode and spinner visibility to the chrome JSON this edge decodes.

## Prompt

> Goal: on Android, drop the on-screen back and forward buttons, collapse Reload and Stop into one control, and add a loading spinner.
>
> Read the done record of `reload-stop-collapse-and-loading-spinner-core-and-gtk` first: it added the control mode and spinner visibility to the core derivation and to the chrome JSON carrier this edge already decodes each refresh. Read those fields; do NOT write a Kotlin `when` for either. That twin existed before, drifted, was deleted by `mobile-chrome-presentation-from-one-derivation`, and a guard reds the gate if one returns (`CONTEXT.md`, "chrome presentation / painter").
>
> The buttons are safe to remove because the SYSTEM Back button is already wired to page history via the non-deprecated back-pressed dispatcher (task `android-hardware-back-button-navigates-history`, and note it was a FIELD-REPORTED bug that it did not, so assert the behaviour survives rather than assuming). Removing forward leaves no forward affordance on Android; that is accepted in the spec.
>
> Do not touch `ChromeState::can_go_back` / `can_go_forward` or the `Renderer` history methods: the system Back rides on them, and so do the desktop shortcuts.
>
> SEQUENCING: this is a MIGRATE step. Do not add the new fields to the mobile presentation guard's hardcoded lists; the guard demands that BOTH mobile edges consume a field, so registering it here would red the gate until the iOS task lands. The fan-in task `register-the-new-chrome-fields-in-the-mobile-presentation-guard` owns that and is blocked on both edges.
>
> FIRST, check this task against current reality (it is a launch snapshot and may have DRIFTED): confirm the chrome JSON carries the new fields, and that the system-Back wiring is still as described.
>
> RECORD non-obvious in-scope decisions durably and link them from the done record, in particular what fills the freed toolbar width and whether the spinner shares a slot with the collapsed control.

---

### Claiming this task

```sh
dorfl claim android-chrome-collapse-reload-stop-and-drop-history-buttons --arbiter origin
git fetch origin && git switch -c work/android-chrome-collapse-reload-stop-and-drop-history-buttons origin/main
git mv work/tasks/ready/android-chrome-collapse-reload-stop-and-drop-history-buttons.md work/tasks/done/android-chrome-collapse-reload-stop-and-drop-history-buttons.md
```
