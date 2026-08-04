---
title: "iOS: collapse Reload/Stop into one control, add the spinner, and DROP the back/forward buttons (once the swipe gesture is on)"
slug: ios-chrome-collapse-reload-stop-and-drop-history-buttons
spec: chrome-conventional-controls
blockedBy: [reload-stop-collapse-and-loading-spinner-core-and-gtk, enable-the-ios-back-forward-swipe-gesture]
covers: [8, 9, 10, 11]
---

## What to build

The iOS toolbar gets the collapse, the spinner, and loses two buttons.

The shell controller currently builds back, forward, reload and stop buttons (the history pair labelled with text arrows) plus the URL field and a menu button. On a phone that toolbar is the width-starved surface in the product, and once the edge-swipe gesture is enabled the history buttons duplicate a platform affordance. So they go, and Reload/Stop collapse into the single control the core derives.

**The blocking task is not optional.** `allowsBackForwardNavigationGestures` was measured as never set, meaning the swipe is OFF by WKWebView's default. Removing these buttons before `enable-the-ios-back-forward-swipe-gesture` lands would leave iOS with NO history navigation at all, and with no Mac on this project (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`) nobody would find out by using it. Verify the gesture is on before removing anything.

**What must NOT change:** `ChromeState::can_go_back` / `can_go_forward` and the `Renderer` history methods. This removes BUTTONS, not the capability.

**Read the derived values, do not recompute them.** Spinner visibility and the control's mode arrive in the chrome JSON this edge already decodes; a Swift `switch` deciding either is the twin this repo deleted once and now guards against. Note this edge has previous form here: its invalid-entry badge wording was once a Swift literal set at build time and never refreshed at all.

Unlike Android, iOS keeps BOTH directions after the buttons go, because the swipe covers back and forward.

## Acceptance criteria

- [ ] The iOS toolbar no longer shows back or forward buttons.
- [ ] The edge-swipe gesture is verified enabled before the buttons are removed (the blocking task's assertion is in place and passing).
- [ ] Reload and Stop are ONE control whose mode comes from the chrome JSON's derived value.
- [ ] A spinner shows while loading, its visibility read from the chrome JSON.
- [ ] No Swift conditional decides the control mode or the spinner's visibility.
- [ ] The mobile presentation guard's field lists are NOT touched here. This task is a MIGRATE step: it makes the Swift edge consume the new fields. Registering them is the fan-in task `register-the-new-chrome-fields-in-the-mobile-presentation-guard`, blocked on this task and the Android one, because the guard requires BOTH edges to consume a field before it can demand it.
- [ ] `can_go_back` / `can_go_forward` and the history seam are unchanged; only the painter changes.
- [ ] Cancelling an in-flight load is still possible.
- [ ] Covered by assertions a CI runner can make without a human at a Mac (the Simulator `.app` build and its bundle check are the existing leg).
- [ ] Tests network-isolated; mirror the repo's existing test style, including its Swift-source shape assertions.

## Blocked by

- `enable-the-ios-back-forward-swipe-gesture` — the gesture must exist before the buttons that currently provide the only history navigation are removed.
- `reload-stop-collapse-and-loading-spinner-core-and-gtk` — it adds the derived control mode and spinner visibility to the chrome JSON this edge decodes.

## Prompt

> Goal: on iOS, drop the back and forward buttons, collapse Reload and Stop into one control, and add a loading spinner.
>
> Read the done records of BOTH blocking tasks first. `enable-the-ios-back-forward-swipe-gesture` is what makes removing the history buttons safe: the swipe was measured as OFF (the flag was never set, and WKWebView defaults it to false), so confirm it is genuinely on before you remove anything. `reload-stop-collapse-and-loading-spinner-core-and-gtk` added the control mode and spinner visibility to the chrome JSON carrier this edge already decodes each refresh; READ those fields rather than writing a Swift `switch`, which is the twin `mobile-chrome-presentation-from-one-derivation` deleted and a guard now catches (`CONTEXT.md`, "chrome presentation / painter"). This edge has previous form: its invalid-entry badge text was once a Swift literal set at build time and never refreshed.
>
> Do not touch `ChromeState::can_go_back` / `can_go_forward` or the `Renderer` history methods; the swipe and the desktop shortcuts both ride on them.
>
> SEQUENCING: this is a MIGRATE step. Do not add the new fields to the mobile presentation guard's hardcoded lists; the guard demands BOTH mobile edges consume a field, so registering it here would red the gate if the Android task has not landed. The fan-in task `register-the-new-chrome-fields-in-the-mobile-presentation-guard` owns that step.
>
> Story 13 (the edge-swipe gesture) is delivered by the blocking task `enable-the-ios-back-forward-swipe-gesture`, not here; this task only depends on it. That is why it is not in this task's `covers`.
>
> Verification constraint: there is no Mac here, so CI is the only evidence this edge gets. The existing leg builds the Simulator `.app` and runs a bundle check; put your assertions where that leg can run them, and do not leave the removal resting on a visual check nobody can perform.
>
> FIRST, check this task against current reality (it is a launch snapshot and may have DRIFTED): confirm the gesture flag is set and the chrome JSON carries the new fields before building against either.
>
> RECORD non-obvious in-scope decisions durably and link them from the done record, in particular what fills the freed toolbar width and whether the spinner shares a slot with the collapsed control.

---

### Claiming this task

```sh
dorfl claim ios-chrome-collapse-reload-stop-and-drop-history-buttons --arbiter origin
git fetch origin && git switch -c work/ios-chrome-collapse-reload-stop-and-drop-history-buttons origin/main
git mv work/tasks/ready/ios-chrome-collapse-reload-stop-and-drop-history-buttons.md work/tasks/done/ios-chrome-collapse-reload-stop-and-drop-history-buttons.md
```

## Gate-3 conductor verdict (drive-tasks)

APPROVE, first attempt. Gate 1 and Gate 2 both green.

- Reload and Stop collapsed into ONE control, with the spinner added: `reloadStopButton` + `loadingSpinner` (`UIActivityIndicatorView`) in `WKWebViewShellController.swift`. MET.
- The on-screen back/forward buttons are DROPPED: no live references remain; the removed controls survive only in comments explaining WHY they went. Safe because the edge-swipe gesture landed first in `enable-the-ios-back-forward-swipe-gesture`, which is exactly the ordering this task's `blockedBy` encoded. MET.
- Strings come from the ONE core derivation, never a per-edge literal: the title is the carried `reloadStopControlLabel`, the accessible name the carried `reloadStopControlDescription`, spinner visibility is `loadSpinnerVisible` "and nothing else", and a tap performs whatever the core says via `WerustCore.activateReloadStopControl`. A grep for the derived strings as Swift literals finds none. MET.
- Cancelling an in-flight load is still reachable, and the control deliberately does NOT turn INTO the spinner (which would make it unavailable exactly when Stop is needed). MET.

Guard check: `crates/werust-core/tests/mobile_chrome_presentation_shape.rs` NOT touched. `rust-toolchain.toml` NOT touched.

CI VERIFIED on main (the Linux gate never compiles Swift, so this is the real evidence): `mobile-ios` SUCCESS and `verify` SUCCESS on the merge commit.

Four non-blocking Gate-2 nits: `work/notes/observations/review-nits-ios-chrome-collapse-reload-stop-and-drop-history-buttons-2026-08-04.md`.

With this and the Android migrate step landed, the fan-in `register-the-new-chrome-fields-in-the-mobile-presentation-guard` is UNBLOCKED. Until it lands, the four new chrome JSON keys cross to both mobile edges with no guard coverage.
