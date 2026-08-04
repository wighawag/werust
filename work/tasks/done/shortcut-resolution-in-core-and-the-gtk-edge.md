---
title: "Browser keyboard shortcuts: ONE resolution in the toolkit-free core, proven end to end on the GTK desktop edge (plus mouse buttons 4/5)"
slug: shortcut-resolution-in-core-and-the-gtk-edge
spec: chrome-conventional-controls
blockedBy: []
covers: [1, 2, 3, 5, 6, 7, 15]
---

## What to build

The tracer bullet for the whole shortcut layer: the shared resolution in `werust-core`, plus the GTK desktop edge translating its native key events into it, end to end.

werust currently binds exactly ONE key in the entire application (F12, for the web inspector), so none of a user's browser muscle memory works. This task makes the conventional shortcuts real on desktop Linux and, more importantly, establishes the SEAM the other two desktop edges then reuse rather than reimplement.

The resolution is a pure function from an abstract (key, modifiers, focus) to a chrome ACTION, living in the toolkit-free core so it is testable with no display and so the Cmd-versus-Ctrl split is ONE branch rather than a per-edge reimplementation. Each edge translates its own native key event into the abstract form; no edge decides what a chord MEANS.

The shortcuts in scope: focus-and-select the URL bar, reload, history back and forward, stop the load, and the existing web inspector, which folds INTO the table rather than sitting beside it. Plus mouse buttons 4 and 5 mapping to history back/forward on the same edge, since that is the same input-to-action plumbing.

**Escape is the one that shapes the signature.** It is focus-dependent: it stops the load when the page has focus, and reverts the edit and restores the current URL when the URL bar has focus. So the resolution takes focus as an INPUT rather than each edge special-casing Escape, which is precisely the drift this seam exists to prevent.

**The existing F12 behaviour must not regress.** Its current pure function is pinned by tests that assert F12 with no modifiers opens the WEB inspector and that Ctrl+Shift+I / Ctrl+Shift+D do NOT (they are GTK's interactive debugger). Those guarantees survive the move into the table.

Scope note: DESKTOP only. No story asks for hardware-keyboard shortcuts on the mobile edges, and they are not in scope here.

## Acceptance criteria

- [ ] A pure, display-free resolution in `werust-core` maps an abstract (key, modifiers, focus) to a chrome action, covering: focus-and-select the URL bar, reload, history back, history forward, stop, and the web inspector.
- [ ] The Cmd-versus-Ctrl difference is expressed ONCE in that resolution, not per edge (this task only exercises the Ctrl side; the macOS edge task exercises Cmd).
- [ ] Escape resolves differently by focus: stop the load with the page focused, revert-and-restore with the URL bar focused.
- [ ] The GTK edge translates its native key events into the abstract form and performs the resolved actions; no chord's MEANING is decided in the edge.
- [ ] Mouse buttons 4 and 5 navigate history on the GTK edge.
- [ ] F12 still opens the web inspector, and Ctrl+Shift+I / Ctrl+Shift+D still do not; the existing assertions survive (moved, not weakened).
- [ ] History actions go through the existing `Renderer` seam methods and `ChromeState::can_go_back` / `can_go_forward`; nothing in that seam changes.
- [ ] The resolution is CAPABILITY-AGNOSTIC: it resolves a chord to an action regardless of whether a given edge can perform it, and an edge that lacks the underlying capability simply has no handler for that action. This is load-bearing for the macOS sibling task, where the web inspector does not exist at all (`docs/platform-capability-matrix.toml` records `web-inspector` as `stubbed` on macOS, owned by `macos-web-inspector-safari-devtools`), so a capability-aware resolution would fork per platform and re-mint exactly the per-edge decision this seam removes.
- [ ] A table test pins the whole shortcut set, including negative cases (a modified F12 is not the plain-F12 shortcut) and both Escape focus states.
- [ ] Tests cover the new behaviour and mirror the repo's existing test style; no display required.

## Blocked by

- None — can start immediately.

## Prompt

> Goal: give werust the keyboard shortcuts every browser has, resolved in ONE place, and prove it end to end on the GTK desktop edge.
>
> Read `work/specs/tasked/chrome-conventional-controls.md` first (Problem, Solution, Implementation Decisions). The precedent you are generalising already exists in the desktop binary: a pure function that decides whether a (keyval, modifiers) pair means "open the web inspector", deliberately written display-free and pinned by tests, including negative cases that stop it colliding with GTK's interactive debugger (Ctrl+Shift+I / Ctrl+Shift+D). Find it, and grow it into a general (key, modifiers, focus) -> chrome-action resolution in the toolkit-free `werust-core`, then wire the GTK edge to translate its native events into that form.
>
> The repo's ONE-derivation rule governs this (`CONTEXT.md`, "chrome presentation / painter"): the core owns the DECISION, the edge owns the translation and the doing. An edge that decides what a chord means is the drift this seam exists to prevent, and the repo has already paid for that class of drift twice.
>
> Escape is focus-dependent (stop the load with the page focused; revert and restore the URL with the bar focused), so focus is an INPUT to the resolution, not an edge special case. Get that into the signature before writing the table, or every edge will grow its own Escape branch.
>
> History actions must go through the existing `Renderer` seam (`go_back` / `go_forward`) and the existing `ChromeState` capability flags. Do not change that seam: the Android hardware Back button already rides on it.
>
> FIRST, check this task against current reality (it is a launch snapshot and may have DRIFTED): confirm the web-inspector function and its tests are still shaped as described, and that no other shortcut has landed since.
>
> One design point is already settled and must not be re-opened per edge: the resolution is CAPABILITY-AGNOSTIC. It maps a chord to an action; whether an edge can PERFORM that action is the edge's business. macOS is the live case (it has no web inspector at all, per the capability matrix), and a resolution that knew about per-platform capabilities would fork into exactly the per-edge branching this seam exists to delete.
>
> RECORD non-obvious in-scope decisions durably and link them from the done record. The likely one: how the abstract key/modifier vocabulary is spelled (it must be expressible by GTK, Win32 and AppKit alike without leaking any one toolkit's enum into the core), which every later edge inherits. Record the capability-agnostic rule above too, since three sibling edge tasks depend on it.

---

### Claiming this task

```sh
dorfl claim shortcut-resolution-in-core-and-the-gtk-edge --arbiter origin
git fetch origin && git switch -c work/shortcut-resolution-in-core-and-the-gtk-edge origin/main
git mv work/tasks/ready/shortcut-resolution-in-core-and-the-gtk-edge.md work/tasks/done/shortcut-resolution-in-core-and-the-gtk-edge.md
```

## Requeue 2026-08-03

Previous run CRASHED for an ENVIRONMENTAL reason only: the acceptance gate ran in /tmp (a 16G tmpfs already 100% full from unrelated projects) and rustc died with 'No space left on device' while linking. This was NOT a code defect and NOT a gate finding. Your committed work on this branch is intact and is being CONTINUED. Do not restart or rewrite the implementation; re-verify it compiles and passes the gate, and only fix genuine defects the gate now reports.

## Decisions

The non-obvious, in-scope judgement calls this task baked in are recorded, one entry each (what was chosen, why, the alternatives, and which sibling task inherits it), in **`docs/spikes/shortcut-resolution-in-core-and-the-gtk-edge/DECISIONS.md`**: the abstract key/modifier vocabulary (spelled after W3C UI Events, so no toolkit enum crosses into the core), the CAPABILITY-AGNOSTIC rule the three sibling edge tasks depend on, the `PrimaryModifier` parameter carrying the Cmd-versus-Ctrl split on both the accelerator and the history axis, focus as a two-valued input, the mouse side buttons riding the same vocabulary, the GTK controllers sitting on the window in the CAPTURE phase, what Escape restores in the URL bar, and why the parity matrix's mobile cells are `n-a`.

The same file's siblings carry the rest: `docs/spikes/shortcut-resolution-in-core-and-the-gtk-edge/README.md` (where things live, the shortcut set, and the manual display-needing check), the `shortcut resolution (chord -> chrome action)` glossary entry in `CONTEXT.md`, the `conventional-shortcuts` row in `docs/platform-capability-matrix.toml`, and the module docs on `crates/werust-core/src/shortcuts.rs`.

## Gate-3 conductor verdict (drive-tasks)

APPROVE. Reviewed the merged diff (`6be3164..3731cf7`) against each acceptance criterion.

- Pure display-free resolution in core: `crates/werust-core/src/shortcuts.rs`, `resolve_chord(chord, focus, primary) -> Option<ChromeAction>`; covers FocusUrlBar, Reload, GoBack, GoForward, Stop, OpenWebInspector (+ RevertUrlBar for Escape). MET.
- Cmd-versus-Ctrl expressed ONCE: `PrimaryModifier{Control, Meta}` + `for_target()`; no per-edge branch. Core tests drive both `CTRL_PLATFORM` and `MAC_PLATFORM`. MET.
- Escape focus-dependent: `Focus::Page => Stop`, `Focus::UrlBar => RevertUrlBar`. MET.
- GTK edge translates, never decides: `main.rs` calls `resolve_chord`/`resolve_pointer_button` and matches on `ChromeAction`; no chord meaning in the edge. MET.
- Mouse buttons 4/5: `resolve_pointer_button` wired in the GTK edge. MET.
- F12 intact, GTK debugger chords still excluded: `f12_opens_the_web_inspector_and_the_gtk_debugger_chord_does_not` moved, not weakened; modifier matching is EXACT so Ctrl+Shift+I / Ctrl+Shift+D cannot collide. MET.
- `Renderer` seam unchanged: no file under `crates/renderer/`, `crates/native-renderer/`, `crates/webview-renderer/` touched. MET.
- Capability-agnostic: no capability parameter; unperformable actions are simply unhandled by an edge. MET (load-bearing for the macOS sibling).
- Table test with negatives and both Escape focus states: present. MET.

Guard check: `crates/werust-core/tests/mobile_chrome_presentation_shape.rs` NOT touched. `rust-toolchain.toml` NOT touched. Commits are conventional-commit.

History note: this task landed after an environment-only failure (the acceptance gate ran out of space on a full `/tmp` tmpfs). It was recovered with `requeue` keep+continue, so the branch carries TWO `feat(...)` commits with the same subject (`dfc9acb` the implementation, `3731cf7` a 30-line follow-up). Content is not duplicated; the generated changelog will show the line twice.

Four non-blocking Gate-2 nits are recorded in `work/notes/observations/review-nits-shortcut-resolution-in-core-and-the-gtk-edge-2026-08-04.md` and left open for triage.
