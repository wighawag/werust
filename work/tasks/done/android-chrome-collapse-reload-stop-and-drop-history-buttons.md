---
title: "Android: collapse Reload/Stop into one control, add the spinner, and DROP the on-screen back/forward buttons (the hardware Back already navigates history)"
slug: android-chrome-collapse-reload-stop-and-drop-history-buttons
spec: chrome-conventional-controls
blockedBy: [reload-stop-collapse-and-loading-spinner-core-and-gtk]
covers: [8, 9, 10, 11, 12]
needsAnswers: true
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

## Requeue 2026-08-04

Gate 2 BLOCKED the previous attempt with ONE finding. Your committed work on this branch is kept and is being CONTINUED: build on it, do not restart, and do not redo the parts that were fine.

THE FINDING: the new test the_mobile_presentation_guard_field_lists_are_not_registered_here in crates/werust-android/rust/tests/collapsed_control_and_dropped_history_buttons_shape.rs is VACUOUS - it can never fail, so the migrate/contract sequencing it advertises is in fact unguarded.

Why it cannot fail: it does scan(guard) and then asserts !code.contains(field). But scan() strips string literals OUT of the CODE view and collects them separately into a LITERALS collection. A field name can only ever appear in mobile_chrome_presentation_shape.rs AS a string literal (a DERIVED_FIELDS / FACT_FIELDS entry). Proof: the already-registered loadProgressVisible appears at lines 117, 334 and 362 of that guard, all three inside literals, so the CODE view contains it nowhere. Registering loadSpinnerVisible tomorrow would leave this assertion GREEN.

THE FIX (either is acceptable, pick one and make it honest):
(a) Assert against the LITERALS half instead - scan() already returns it - so the test really does detect a field being registered early; or
(b) DROP the assertion entirely and stop documenting it as protection.

If you keep it (option a), also correct DECISIONS.md section 7, which currently claims asserting the absence is what stops a well-meaning later change from registering the field early, and the spike README, which says the suite was mutation-checked (its five listed mutations do not include this one). A mutation-check claim must cover this mutation: registering the field early MUST turn the test red.

HARD CONSTRAINT, unchanged and load-bearing: do NOT edit, weaken or special-case crates/werust-core/tests/mobile_chrome_presentation_shape.rs. This task must NOT register the new fields; registration is owned by the fan-in task register-the-new-chrome-fields-in-the-mobile-presentation-guard. You are only asserting, honestly, that they are not registered YET.

Also unchanged: do not re-select the toolchain (rust-toolchain.toml is pinned); user-facing chrome strings must come from the ONE core derivation (reloadStopControlLabel / reloadStopControlDescription / loadSpinnerVisible via chrome JSON), never a per-edge literal; conventional-commit subjects.

(Correction: two words in the note above were lost to shell backtick expansion when the requeue was issued; restored as CODE view / LITERALS collection. The fix in option (a) is to assert against the collection scan() returns for string-literal contents, rather than against the literal-stripped code view.)

## Gate-3 conductor verdict (drive-tasks)

APPROVE, on the SECOND attempt. Gate 2 blocked the first attempt with one finding; it is fixed properly rather than papered over.

The blocked finding was a VACUOUS test: `the_mobile_presentation_guard_field_lists_are_not_registered_here` asserted a field's absence from the literal-STRIPPED code view, but a field name can only ever appear in the guard AS a string literal, so the assertion could never fail and the sequencing it advertised was unguarded.

FIXED, and fixed the honest way (option a, not by deleting the check): the test now reads the LITERAL half of the scan, and it adds a POSITIVE CONTROL pinning that the check really does see a registered field. The comment now states plainly why the stripped-code version would be an assertion that can never fail. That is a genuinely stronger guard than the one first submitted.

Criteria:
- Reload and Stop collapsed into ONE control, spinner added, on-screen back/forward DROPPED (the hardware Back already navigates history). MET.
- Strings come from the ONE core derivation via chrome JSON (`reloadStopControlLabel` / `reloadStopControlDescription` / `loadSpinnerVisible`), not per-edge Kotlin literals. MET.
- The MIGRATE/CONTRACT sequencing is respected: this task does NOT register the new fields; it asserts, and now really proves, that they are not registered yet. Registration stays owned by `register-the-new-chrome-fields-in-the-mobile-presentation-guard`. MET.

Guard check: `crates/werust-core/tests/mobile_chrome_presentation_shape.rs` NOT touched. `rust-toolchain.toml` NOT touched.

CI: `verify` SUCCESS on the merge commit. Note the evidence limit, which is pre-existing and not this task's doing: there is NO Android CI leg. The other workflows are path-filtered and did not run, so the Kotlin is never COMPILED anywhere in CI; it is guarded only by the Rust-side source-SCANNING tests on the Linux gate, which check source shape as text.

Seven non-blocking Gate-2 nits: `work/notes/observations/review-nits-android-chrome-collapse-reload-stop-and-drop-history-buttons-2026-08-04.md`. The agent also filed `kotlin-source-scanner-duplicated-across-edge-guards-2026-08-04.md`.
