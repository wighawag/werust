---
title: "The macOS edge speaks the shared shortcut resolution, and is what finally exercises its Cmd branch"
slug: shortcuts-and-mouse-history-buttons-on-the-macos-edge
spec: chrome-conventional-controls
blockedBy: [shortcut-resolution-in-core-and-the-gtk-edge]
covers: [1, 2, 3, 4, 5, 6, 7, 15]
---

## What to build

The AppKit edge's half of the shortcut layer, and the ONLY place the shared resolution's Cmd branch is exercised: Mac users expect Cmd+L and Cmd+R, not Ctrl.

**One shortcut is deliberately out of scope on this edge: the web inspector.** macOS is the only edge where it does not exist (the capability matrix records it `stubbed`, owned by `macos-web-inspector-safari-devtools`; neither the macOS renderer nor the shell touches `WKPreferences`), so there is nothing to open. That is not a gap in this task, it is the capability-agnostic rule working as designed: the core resolves the chord to an action, and an edge without the capability has no handler. Do not add a per-platform branch to the resolution to express this.

Like the Windows sibling this is a thin translation task. The core resolution already decides what each chord means, including the Cmd-versus-Ctrl difference, which was deliberately put in ONE branch rather than duplicated per edge. This edge translates NSEvent key input and its modifier flags into the abstract form, reports focus (Escape is focus-dependent), performs the returned actions, and maps the extended mouse buttons to history.

**Verification reality for this edge:** nobody on this project has a Mac (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`), so this cannot be field-tested and CI is the only evidence it will ever get. Prefer assertions a runner can make (the translation table, the Cmd mapping, source shape) over anything that needs a human to press a key, and lean on the existing macOS CI leg and the from-Linux typecheck harness rather than assuming a manual check will catch a mistake.

## Acceptance criteria

- [ ] Every shortcut the shared resolution defines works on the macOS edge, using the platform's Cmd modifier where the resolution specifies it, EXCEPT the web inspector (see the exclusion below).
- [ ] The web-inspector shortcut is deliberately NOT delivered here and its absence is explicit, not silent: macOS has no web inspector at all (`docs/platform-capability-matrix.toml` records `web-inspector` as `state = stubbed` on macOS, owned by `macos-web-inspector-safari-devtools`), so there is nothing for the action to open. The edge simply has no handler for that action, per the shared resolution's capability-agnostic rule. When that task lands, wiring the handler is a one-line follow-on and needs no change to the resolution.
- [ ] The Cmd branch of the shared resolution is genuinely exercised (this is the only edge that can), and its distinctness from the Ctrl branch is asserted.
- [ ] Escape behaves per focus (stop the load with the page focused; revert and restore with the URL bar focused), using focus reported by this edge.
- [ ] Mouse buttons 4 and 5 navigate history.
- [ ] The edge contains NO decision about what a chord means: translation and execution only.
- [ ] History actions go through the existing seam and capability flags; the seam is unchanged.
- [ ] The new behaviour is covered by assertions a CI runner can make without a human at a Mac.
- [ ] Tests network-isolated; mirror the repo's existing test style.

## Blocked by

- `shortcut-resolution-in-core-and-the-gtk-edge` — it defines the abstract key vocabulary, the resolution, and the Cmd branch this edge is the first to exercise.

## Prompt

> Goal: make the conventional browser shortcuts work on the macOS AppKit edge, with Cmd where a Mac user expects Cmd, by translating native input into the SHARED resolution in `werust-core` that `shortcut-resolution-in-core-and-the-gtk-edge` established. Read that task's done record first: your job is translation and execution, never interpretation.
>
> Look at the macOS crate's window module to see how this edge builds its toolbar controls, receives input and drives actions today, and at `crates/desktop-paint` for the host-independent painted snapshot this edge already consumes. Map NSEvent key codes and modifier flags onto the toolkit-neutral abstract vocabulary rather than pushing anything AppKit-shaped into the core.
>
> Escape is focus-dependent, so report whether the page or the URL bar has focus as an input to the resolution.
>
> Do NOT try to deliver the web-inspector shortcut here: macOS has no web inspector (capability matrix: `web-inspector` is `stubbed` on macOS, owned by `macos-web-inspector-safari-devtools`). The action resolves; this edge just has no handler for it. Adding a platform branch to the shared resolution to express that would re-mint the per-edge decision the seam exists to delete.
>
> IMPORTANT verification constraint: there is no Mac on this project, so CI is the ONLY evidence this edge will ever get (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`). Write assertions a runner can make. The repo already has a from-Linux typecheck harness for the macOS crates and a macOS CI leg; use them, and do not leave the Cmd mapping resting on "someone will notice".
>
> FIRST, check this task against current reality (it is a launch snapshot and may have DRIFTED): confirm the shared resolution landed with focus as an input and with the Cmd branch present but unexercised.
>
> RECORD non-obvious in-scope decisions durably and link them from the done record, in particular any place where AppKit's own key-equivalent handling would compete with this resolution for the same chord.

---

### Claiming this task

```sh
dorfl claim shortcuts-and-mouse-history-buttons-on-the-macos-edge --arbiter origin
git fetch origin && git switch -c work/shortcuts-and-mouse-history-buttons-on-the-macos-edge origin/main
git mv work/tasks/ready/shortcuts-and-mouse-history-buttons-on-the-macos-edge.md work/tasks/done/shortcuts-and-mouse-history-buttons-on-the-macos-edge.md
```
