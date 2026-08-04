---
title: "The macOS window collapses Reload/Stop into one control and shows the spinner, reading the painted snapshot"
slug: reload-stop-collapse-and-spinner-on-the-macos-window
spec: chrome-conventional-controls
blockedBy: [reload-stop-collapse-and-loading-spinner-core-and-gtk, shortcuts-and-mouse-history-buttons-on-the-macos-edge]
covers: [8, 9, 10, 14]
---

## What to build

The AppKit painter's half of the collapse: replace the separate Reload and Stop buttons with the ONE control the core now derives, and show the spinner, both read from the `desktop-paint` snapshot rather than computed here.

This edge currently builds distinct buttons for back, forward, reload and stop and enables them from the painted snapshot (Stop only while loading, Reload only when not). That pair collapses into one control whose MODE comes from the derivation.

Purely a painter change: no new chrome fact is invented here.

**Verification reality:** nobody on this project has a Mac (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`), so CI is the only evidence this edge will ever get, and a visual regression here will not be caught by a human. Prefer assertions a runner can make, and lean on the existing macOS CI leg and the from-Linux typecheck harness.

## Acceptance criteria

- [ ] The macOS toolbar shows ONE control that reloads when idle and stops while loading, replacing the separate pair.
- [ ] The control's mode and the spinner's visibility are read from the painted snapshot; neither is computed in this edge.
- [ ] Cancelling an in-flight load is still possible from the toolbar and from the keyboard.
- [ ] Back and forward buttons are untouched (desktop keeps them, per the spec).
- [ ] Covered by assertions a CI runner can make without a human at a Mac.
- [ ] Tests network-isolated; mirror the repo's existing test style.

## Blocked by

- `reload-stop-collapse-and-loading-spinner-core-and-gtk` — it adds the derived control mode and spinner visibility to the `desktop-paint` snapshot this task reads.
- `shortcuts-and-mouse-history-buttons-on-the-macos-edge` — no logical dependency, but both edit this crate's window module, so they are serialised to avoid a merge conflict.

## Prompt

> Goal: on the macOS AppKit edge, replace the separate Reload and Stop buttons with the single mode-switching control the core derives, and show the loading spinner.
>
> Read the done record of `reload-stop-collapse-and-loading-spinner-core-and-gtk` first: it added the control mode and spinner visibility to the core derivation and to `crates/desktop-paint`, the plain-Rust snapshot this edge already consumes (it is a CARRIER, not a second derivation: every field is a core function's return value, and tests assert exactly that). This task READS those values; a local conditional for either is the drift the one-derivation rule forbids.
>
> Look at the macOS crate's window module, which builds the toolbar buttons and enables them from the painted snapshot; the reload/stop pair is what collapses.
>
> IMPORTANT verification constraint: there is no Mac on this project (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`), so write assertions a runner can make and use the existing macOS CI leg and from-Linux typecheck harness. A "looks right" check is not available here.
>
> FIRST, check this task against current reality (it is a launch snapshot and may have DRIFTED): confirm the snapshot actually carries the new fields before building against them.
>
> RECORD non-obvious in-scope decisions durably and link them from the done record, in particular whether the spinner uses AppKit's own progress indicator and where it sits relative to the collapsed control.

---

### Claiming this task

```sh
dorfl claim reload-stop-collapse-and-spinner-on-the-macos-window --arbiter origin
git fetch origin && git switch -c work/reload-stop-collapse-and-spinner-on-the-macos-window origin/main
git mv work/tasks/ready/reload-stop-collapse-and-spinner-on-the-macos-window.md work/tasks/done/reload-stop-collapse-and-spinner-on-the-macos-window.md
```

## Gate-3 conductor verdict (drive-tasks)

APPROVE, first attempt. The last of the ten chrome tasks.

- ONE control replacing the separate Reload/Stop pair, plus the spinner: a single `reload_stop: Retained<NSButton>` with an `NSProgressIndicator` spinner beside it. MET.
- Mode and spinner visibility READ from the painted snapshot, derived in neither: driven by `ChromePaint::reload_stop_control` / `ChromePaint::spinner_visible`, with the module doc stating plainly that this edge decides nothing of its own. MET.
- The layout does not shift when the spinner appears, so starting a load never moves the URL bar. MET.
- Back and forward controls untouched. MET.
- Guard file and `rust-toolchain.toml` NOT touched.

CI VERIFIED: `macos-renderer` SUCCESS on the merge commit (the Ubuntu gate never compiles AppKit, so this is the real evidence).

Eight non-blocking Gate-2 nits: `work/notes/observations/review-nits-reload-stop-collapse-and-spinner-on-the-macos-window-2026-08-04.md`. The agent also filed two observations, one of which (`windows-capability-cell-still-reads-stubbed-after-its-collapse-task-landed`) the conductor has since fixed and closed.
