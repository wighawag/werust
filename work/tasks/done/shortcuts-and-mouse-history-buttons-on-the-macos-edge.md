---
title: "The macOS edge speaks the shared shortcut resolution, and is what finally exercises its Cmd branch"
slug: shortcuts-and-mouse-history-buttons-on-the-macos-edge
spec: chrome-conventional-controls
blockedBy: [shortcut-resolution-in-core-and-the-gtk-edge]
covers: [1, 2, 3, 4, 5, 6, 7, 15]
needsAnswers: true
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
dorfl claim shortcuts-and-mouse-history-buttons-on-the-macos-edge --arbiter origin
git fetch origin && git switch -c work/shortcuts-and-mouse-history-buttons-on-the-macos-edge origin/main
git mv work/tasks/ready/shortcuts-and-mouse-history-buttons-on-the-macos-edge.md work/tasks/done/shortcuts-and-mouse-history-buttons-on-the-macos-edge.md
```

## Requeue 2026-08-04

Gate 2 BLOCKED the previous attempt with ONE finding, and it needed a DESIGN DECISION that has now been made. Your committed work on this branch is kept and is being CONTINUED: build on it, do not restart.

THE FINDING: Cmd+Left / Cmd+Right are claimed unconditionally in sendEvent:, so they are taken away from the URL bar's field editor and from page text fields, where Cmd+Arrow is macOS's standard move-to-beginning/end-of-line binding. A user editing an address who presses Cmd+Left gets a history navigation and loses the edit. No Mac browser does that.

WHY IT COULD NOT BE FIXED IN THE EDGE: the meaning of a chord is decided ONCE in werust_core::shortcuts, and this task is forbidden to fork that. So the fix belongs in the core.

THE DECISION (made by the conductor; implement exactly this):

Make the history chords FOCUS-SENSITIVE, but ONLY on platforms where the history chord collides with text editing. Do NOT gate the history rows on Focus::Page unconditionally: that would REGRESS the GTK and Windows edges, where Alt+Arrow is the history chord, is NOT a text-editing binding, and correctly navigates even while the URL bar has focus (which is what real Linux/Windows browsers do).

Concretely, in crates/werust-core/src/shortcuts.rs:

1. Add a method on PrimaryModifier expressing this ONE platform fact, beside the existing history() method and documented the same way. Something like:

   const fn history_chord_is_a_text_editing_binding(self) -> bool {
       match self {
           PrimaryModifier::Control => false, // Alt+Arrow is not a text binding on Linux/Windows
           PrimaryModifier::Meta => true,     // Cmd+Arrow is move-to-line-start/end on macOS
       }
   }

2. Make the ArrowLeft / ArrowRight history rows resolve only when the chord does not collide, OR the page has focus:

   let history_wins = history
       && (matches!(focus, Focus::Page) || !primary.history_chord_is_a_text_editing_binding());

   ... Key::ArrowLeft if history_wins => Some(ChromeAction::GoBack),
   ... Key::ArrowRight if history_wins => Some(ChromeAction::GoForward),

   When it does not win, resolve to None so the edge leaves the key to the field editor / the page. This is the behaviour every Mac browser has: Cmd+Left navigates back on the page, and moves the caret to the start of the line while you are editing text.

3. Extend the table test to pin BOTH platforms in BOTH focus states, as four explicit cases:
   - MAC_PLATFORM + Cmd+Left + Focus::Page      -> Some(GoBack)
   - MAC_PLATFORM + Cmd+Left + Focus::UrlBar    -> None          (the new guarantee; the regression that was caught)
   - CTRL_PLATFORM + Alt+Left + Focus::Page     -> Some(GoBack)
   - CTRL_PLATFORM + Alt+Left + Focus::UrlBar   -> Some(GoBack)  (UNCHANGED; this is the GTK/Windows behaviour you must not regress)
   Same four for the Right/GoForward direction.

4. This is a change to a SHARED seam that the GTK and Windows edges already consume. Re-run their tests and confirm neither changes behaviour. crates/werust-core/tests/shortcut_edge_wiring_shape.rs and the Windows crate's coverage test must both still pass; if either encodes an assumption that history is focus-independent, correct the ASSUMPTION, do not weaken the new rule.

5. RECORD this in DECISIONS.md as its own numbered decision, and CORRECT the existing text. Decision 1 currently lists Cmd+Arrow among the swallowed chords and calls it the conventional browser trade. That is true for Escape and Cmd+L; it is FALSE for Cmd+Arrow, and that inaccuracy is part of what Gate 2 blocked. Fix that wording. Also fix README manual step 3, which tests Cmd+Left only with the page focused: it must now also state the URL-bar-focused expectation (the caret moves, no navigation).

OPTIONAL, only if it falls out cleanly and is covered by tests: macOS browsers ALSO bind Cmd+[ and Cmd+] to back/forward, and those do not collide with text editing. If you add them, add them in the core table for the Meta platform only, with tests. If it is not clean, leave it out and note it; it is not required by this task.

HARD CONSTRAINTS, unchanged: do NOT edit or weaken crates/werust-core/tests/mobile_chrome_presentation_shape.rs. Do not re-select the toolchain (rust-toolchain.toml is pinned). The edge translates and performs; it never decides what a chord means. Keep the resolution CAPABILITY-AGNOSTIC (no capability parameter in the core). User-facing chrome strings come from the ONE core derivation. Conventional-commit subjects.

## Requeue 2026-08-04

Conductor note: the kept branch was STRANDED by a rebase conflict against latest main (requeue --reconcile could not auto-resolve it). The conflict has now been resolved BY HAND on the branch itself and force-pushed, so the branch is rebased cleanly onto main and its tip is verified: cargo fmt --check clean, cargo clippy --all-targets -D warnings clean, and the previously-conflicted test crates/werust-core/tests/shortcut_edge_wiring_shape.rs passes all 7 cases.

The two conflicts were both additive-registry collisions with the Windows edge task that landed meanwhile, resolved to keep BOTH intents: docs/platform-capability-matrix.toml now records the conventional-shortcuts row with macos = implemented AND windows = implemented (each task had flipped only its own cell), and both platform description blocks are kept with each side's now-stale 'X is the remaining sibling edge task' sentence dropped. In shortcut_edge_wiring_shape.rs the generalised amendment from main was kept and a macOS amendment added beside it.

Your 2026 lines of macOS work are INTACT. Continue from this tip and implement the DESIGN DECISION recorded in the requeue note above (the focus-sensitive history chord in werust_core::shortcuts). Do not redo the edge wiring that is already there.

## Requeue 2026-08-04

Gate 2 BLOCKED again, on a NARROWER point than last time. The focus-sensitive history fix itself is ACCEPTED and stays. Your work on the branch is kept and continued: do not restart, do not revert the core change.

THE FINDING: the fix covers only the URL bar. shortcut_focus in crates/werust-macos/src/window.rs is two-valued and returns Focus::Page for everything inside the WKWebView, so Cmd+Left while a user is typing in a PAGE text field is still claimed as GoBack and destroys the edit. The CODE is acceptable; the RECORDS are not, because both of them claim the opposite:
- docs/spikes/shortcuts-and-mouse-history-buttons-on-the-macos-edge/DECISIONS.md decision 8 (and decision 1) list a page text field among the cases the fix covers.
- The README in that same directory, manual step 3, instructs the one human who will ever test this that a page text field behaves the same way. It will not.

THE DECISION (made by the conductor; implement exactly this): ACCEPT the limit and record it HONESTLY. Do NOT try to add a third Focus value for a page text field. The edge decides synchronously inside the sendEvent: override, and the app cannot know what is focused INSIDE the WKWebView without asking the web content asynchronously, so a PageTextField focus value is not implementable at that decision point. Do not escalate the core Focus vocabulary.

What to change, documentation only (plus the optional binding below):

1. DECISIONS.md decision 8 and decision 1: correct the claim. State plainly that the focus-sensitive history chord protects the URL BAR's field editor ONLY, and that a text field INSIDE the page still loses Cmd+Left / Cmd+Right to history navigation. Say WHY it is not fixable here: Focus is two-valued in the shared core, the WKWebView is opaque to the host at sendEvent: time, and werust's model gives the CHROME first look at every event (the AppKit analogue of the GTK capture phase) rather than letting the page consume the key first, which is how real browsers avoid this. Record it as a KNOWN, ACCEPTED LIMIT with that reasoning, not as a covered case.

2. README manual step 3: correct the instruction so the human tests what is actually true. The URL bar case must behave (caret moves, no navigation). Add the page-text-field case as a KNOWN limit the tester should expect and NOT report as a bug.

3. Add the same limit as a one-line note to the capability matrix row for conventional-shortcuts if that row's macOS prose describes the focus behaviour, so the matrix does not over-claim either.

4. RECOMMENDED and in scope now, because it is the honest mitigation for exactly this limit: also bind Cmd+[ to history back and Cmd+] to history forward for the Meta platform in the shared core table. Those two chords are the macOS browser convention and they do NOT collide with any text-editing binding, so they give a Mac user a history chord that is always safe, including inside a page text field. Add them in werust_core::shortcuts for PrimaryModifier::Meta ONLY, with table tests covering both focus states, and wire the macOS edge to translate them. If this turns out not to be clean, leave it out and say so explicitly in DECISIONS.md rather than half-doing it.

HARD CONSTRAINTS, unchanged: do NOT edit or weaken crates/werust-core/tests/mobile_chrome_presentation_shape.rs. Do not re-select the toolchain. The edge translates and performs; it never decides what a chord means. Keep the resolution CAPABILITY-AGNOSTIC. Do not regress the Ctrl platforms: Alt+Arrow must still resolve to history in BOTH focus states, and the four-case table test must still pass. Conventional-commit subjects.

## Gate-3 conductor verdict (drive-tasks)

APPROVE ON THE CODE, on the FOURTH attempt, but it leaves `main` with a RED `macos-renderer` leg (one check), tasked as `macos-smoke-blur-url-bar-does-not-end-the-field-editor`.

History of this item, because it is the one that fought back:
1. Gate 2 blocked: `Cmd+Left`/`Cmd+Right` claimed unconditionally, stealing macOS's move-to-line-start/end binding from the URL bar. The reviewer correctly said the fix belonged in the core this task may not fork, so it needed a decision.
2. The kept branch was then STRANDED by a rebase conflict against latest `main`; `requeue --reconcile` could not auto-resolve it. Resolved by hand ON the branch (both conflicts were additive-registry collisions with the Windows edge task: the capability matrix now reads `macos = implemented` AND `windows = implemented`, each task having flipped only its own cell), verified `fmt` + `clippy` + the conflicted test, and force-pushed. 2,026 lines of macOS work preserved rather than `--reset`.
3. Gate 2 blocked again, narrower: the fix covered only the URL bar, while the RECORDS claimed it also covered page text fields.
4. Approved.

The design decision (made by the conductor, implemented by the agent):

History chords are focus-sensitive ONLY on platforms where the history chord collides with text editing, expressed once as `PrimaryModifier::history_chord_is_a_text_editing_binding` (`Control => false`, `Meta => true`). Gating on `Focus::Page` unconditionally would have been shorter and would have REGRESSED GTK and Windows, where `Alt+Arrow` is nobody's text binding and navigating from the URL bar is correct browser behaviour.

Verified on the diff:
- The four-case table is pinned exactly: MAC + Cmd+Left + Page -> GoBack; MAC + Cmd+Left + UrlBar -> None (the new guarantee); CTRL + Alt+Left + Page -> GoBack; CTRL + Alt+Left + UrlBar -> **GoBack, unchanged** (the no-regression case), plus both directions and cross-platform negatives. MET.
- The recommended mitigation was implemented: `Cmd+[` / `Cmd+]` via `PrimaryModifier::history_is_also_spelled_with_brackets`, Meta-only. These collide with no text binding, so a Mac user always has a history chord that works even inside a page text field.
- The known limit is now recorded HONESTLY (decision 8): `Focus` is two-valued, a host cannot see what a WKWebView has focused, and werust gives the chrome first look at every event rather than letting the page consume first. It correctly notes that fixing this properly is a whole-product event-order decision belonging in its own spec, not a macOS detail.
- The edge decides no chord: translation lives in `crates/werust-macos/src/input.rs`, deliberately not target-gated so the Cmd branch is unit-tested on the Ubuntu gate.
- Guard file and `rust-toolchain.toml` NOT touched.

CI: `macos-renderer` FAILS on exactly one check, "Escape with the PAGE focused stops the load instead of reverting the bar". Diagnosed as a SMOKE-HARNESS defect, not a product defect: `blur_url_bar` only calls `makeFirstResponder(None)` and discards the result, which does not reliably tear down the field editor that `shortcut_focus` checks first, so the harness cannot reach the page-focused state it is trying to test. Tasked separately; the check must be fixed, never weakened.

Four non-blocking Gate-2 nits: `work/notes/observations/review-nits-shortcuts-and-mouse-history-buttons-on-the-macos-edge-2026-08-04.md`.
