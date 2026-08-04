---
title: "The Windows edge speaks the shared shortcut resolution (keyboard + mouse back/forward), deciding no chord of its own"
slug: shortcuts-and-mouse-history-buttons-on-the-windows-edge
spec: chrome-conventional-controls
blockedBy: [shortcut-resolution-in-core-and-the-gtk-edge]
covers: [1, 2, 3, 5, 6, 7, 15]
---

## What to build

The Win32 edge's half of the shortcut layer: translate native keyboard and mouse input into the abstract form the core resolution already defines, and perform the actions it returns.

This is deliberately a THIN task. The decision of what each chord MEANS was made once, in the core, by the GTK tracer-bullet task. This edge contributes translation only: the Win32 keyboard messages and their modifier state into the abstract (key, modifiers, focus) triple, and the extended mouse buttons into history navigation. If you find yourself writing a branch that decides a chord's meaning, it belongs in the core resolution instead, shared with the other edges.

Focus matters here because Escape is focus-dependent, so the edge must report which of the page and the URL bar has focus rather than assuming.

## Acceptance criteria

- [ ] Every shortcut the shared resolution defines works on the Windows edge: focus-and-select the URL bar, reload, history back, history forward, stop, web inspector.
- [ ] Escape behaves per focus (stop the load with the page focused; revert and restore with the URL bar focused), using focus reported by this edge.
- [ ] Mouse buttons 4 and 5 navigate history.
- [ ] The edge contains NO decision about what a chord means: it translates input and performs returned actions only.
- [ ] History actions go through the existing seam and capability flags; the seam is unchanged.
- [ ] The translation is covered by tests in the style the Windows crate already uses (its shape/source tests run on Linux; the interactive parts belong in the existing Windows CI leg).
- [ ] Tests network-isolated; mirror the repo's existing test style.

## Blocked by

- `shortcut-resolution-in-core-and-the-gtk-edge` — it defines the abstract key vocabulary and the resolution this edge translates into.

## Prompt

> Goal: make the conventional browser shortcuts work on the Windows edge by translating native input into the SHARED resolution that task `shortcut-resolution-in-core-and-the-gtk-edge` put in `werust-core`. Read that task's done record and the resolution's tests first: your job is translation and execution, never interpretation.
>
> Look at the Windows crate's window procedure and its chrome module (which already owns the toolbar control handles and enables/disables them from the painted snapshot) to see how this edge receives input and drives actions today. The abstract key vocabulary is deliberately toolkit-neutral, so map Win32 virtual-key codes and modifier state onto it rather than extending the core with anything Win32-shaped.
>
> Escape is focus-dependent, so this edge must report whether the page or the URL bar has focus as an input to the resolution.
>
> Note the repo convention for this crate: it is package-scoped in CI (`cargo build -p werust-windows`), never a bare workspace build, because the GTK/WebKit crates cannot compile on a Windows runner. Its source-shape tests are written so they can be exercised from Linux; keep new tests in that style so the pure-Rust gate still covers them.
>
> FIRST, check this task against current reality (it is a launch snapshot and may have DRIFTED): confirm the shared resolution landed with the signature described, including focus as an input.
>
> RECORD non-obvious in-scope decisions durably and link them from the done record, in particular any Win32 message whose mapping is not one-to-one with the abstract vocabulary.

## FORWARD-POINTER (planted by the drive-tasks conductor, after the hinge landed)

> The shared resolution landed in `crates/werust-core/src/shortcuts.rs`
> (`resolve_chord(chord, focus, primary)` / `resolve_pointer_button`). INHERIT that
> vocabulary; do NOT fork it or re-decide what a chord means in this edge.
>
> - The Cmd-versus-Ctrl split is already expressed ONCE as `PrimaryModifier{Control, Meta}`.
>   This edge selects its primary modifier; it does not add a second branch.
> - The resolution is CAPABILITY-AGNOSTIC by settled design. An action this edge cannot
>   perform is simply left unhandled. Do NOT add a capability parameter to the core.
> - KNOWN, ACCEPTED LIMIT, out of scope here: letter chords are translated via the active
>   keyboard layout, so Ctrl/Cmd+L and +R resolve only under a Latin layout. Recorded in
>   `work/notes/observations/review-nits-shortcut-resolution-in-core-and-the-gtk-edge-2026-08-04.md`.
>   Do not "fix" it unilaterally in this edge, which would re-fork the vocabulary.
> - `ChromeAction` and the browser menu's `MenuItemKind::Action` are two vocabularies for
>   chrome actions. Do not bridge or merge them here; that coherence question is open in the
>   same nits note.
>
> Read `docs/spikes/shortcut-resolution-in-core-and-the-gtk-edge/DECISIONS.md` before starting.


---

### Claiming this task

```sh
dorfl claim shortcuts-and-mouse-history-buttons-on-the-windows-edge --arbiter origin
git fetch origin && git switch -c work/shortcuts-and-mouse-history-buttons-on-the-windows-edge origin/main
git mv work/tasks/ready/shortcuts-and-mouse-history-buttons-on-the-windows-edge.md work/tasks/done/shortcuts-and-mouse-history-buttons-on-the-windows-edge.md
```

## Gate-3 conductor verdict (drive-tasks)

APPROVE ON THE CODE, but the task is NOT truly done: it leaves `main` with a RED `windows-renderer` CI leg.

Criteria met, on the diff:
- The edge speaks the SHARED resolution and decides no chord of its own: `crates/werust-windows/src/shortcuts.rs` is pure TRANSLATION (`shortcut_key` maps Win32 virtual keys to `shortcuts::Key`, `shortcut_modifiers` maps key state) and defers every meaning to `shortcuts::resolve_chord` / `resolve_pointer_button`. `crates/werust-core/src/shortcuts.rs` is NOT modified, so the core seam was not forked. MET.
- Keyboard + mouse history both routed: `shortcut_pointer_button` (`WM_XBUTTONDOWN`) and `app_command_pointer_button` (`WM_APPCOMMAND`). MET.
- History gated on the same capability the on-screen control reads: `perform_chrome_action` returns early unless `can_go_back` / `can_go_forward`. MET.
- Coverage test `every_conventional_chord_the_core_defines_is_reachable_from_a_win32_key_press` pins the whole table from the Win32 side. MET.
- The forward-note planted by the conductor was honoured: no capability parameter was added to the core, the Latin-layout limit was not "fixed" unilaterally, and `ChromeAction` was not bridged to the menu vocabulary.
- Guard file and `rust-toolchain.toml` NOT touched.

CI CHECK (caution: the Linux gate never compiles the Win32 edge, so this leg is the only real evidence):
- `macos-renderer` SUCCESS, `verify` SUCCESS.
- `windows-renderer` **FAILURE** — a regression introduced by this task, on the smoke section this task itself added.

The three new KEYBOARD smoke checks pass on the real Win32 message loop. The two failures are both in the new MOUSE section, which is sequenced AFTER the tampered-CID negative control and assumes that failed load left a history entry. It did not, so `can_go_back` is false, the precondition check fails, and the button check then burns its full 30s timeout waiting for a navigation that correctly never happens.

This is a TEST-SEQUENCING defect, not a product defect: `window.rs:593-601` deliberately refuses a history move the on-screen control would refuse, so XBUTTON1 doing nothing with empty history is the specified behaviour.

Full diagnosis and the fix: `work/notes/observations/windows-smoke-mouse-back-check-is-sequenced-after-a-failed-load-2026-08-04.md`. NEEDS-ATTENTION: it was already merged under `--merge` (lock released, so not `requeue`-able) and a fix falls outside this drive's ten-task scope.

Three non-blocking Gate-2 nits: `work/notes/observations/review-nits-shortcuts-and-mouse-history-buttons-on-the-windows-edge-2026-08-04.md`.
