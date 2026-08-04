<!-- dorfl-sidecar: item=task:shortcuts-and-mouse-history-buttons-on-the-macos-edge type=task slug=shortcuts-and-mouse-history-buttons-on-the-macos-edge allAnswered=false -->

## Q1

**'task:shortcuts-and-mouse-history-buttons-on-the-macos-edge' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - Cmd+Left / Cmd+Right are claimed unconditionally in sendEvent:, so they are taken away from the URL bar's field editor and from page text fields, where Cmd+Arrow is macOS's standard move-to-beginning/end-of-line binding. A user editing an address (or typing in a web form) who presses Cmd+Left gets a history back navigation and loses the edit, which no Mac browser does. On GTK the identical table cost nothing because Alt+Arrow is not a text binding, so this is the first edge where the shared history chord collides with the platform's own text editing. resolve_chord already takes Focus, so a fix exists (gate the history rows on Focus::Page, or let the edge forward when the bar is being edited), but it belongs in the core the task forbids this edge to fork, so a human must choose. It is also not recorded: DECISIONS.md decision 1 lists Cmd+Arrow among the swallowed chords and calls it the conventional browser trade (true for Escape/Cmd+L, not for Cmd+Arrow on a Mac), decision 2 covers only menu key equivalents, and README's manual step 3 tests Cmd+Left only with the page focused. (crates/werust-macos/src/window.rs claim/claim_key (no focus condition) + werust-core/src/shortcuts.rs history rows resolve on any Focus; DECISIONS.md section 1 Cost, recorded honestly; README manual step 3)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):

## Q2

**'task:shortcuts-and-mouse-history-buttons-on-the-macos-edge' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - Cmd+Left / Cmd+Right are claimed unconditionally in sendEvent:, so they are taken away from the URL bar's field editor and from page text fields, where Cmd+Arrow is macOS's standard move-to-beginning/end-of-line binding. A user editing an address (or typing in a web form) who presses Cmd+Left gets a history back navigation and loses the edit, which no Mac browser does. On GTK the identical table cost nothing because Alt+Arrow is not a text binding, so this is the first edge where the shared history chord collides with the platform's own text editing. resolve_chord already takes Focus, so a fix exists (gate the history rows on Focus::Page, or let the edge forward when the bar is being edited), but it belongs in the core the task forbids this edge to fork, so a human must choose. It is also not recorded: DECISIONS.md decision 1 lists Cmd+Arrow among the swallowed chords and calls it the conventional browser trade (true for Escape/Cmd+L, not for Cmd+Arrow on a Mac), decision 2 covers only menu key equivalents, and README's manual step 3 tests Cmd+Left only with the page focused. (crates/werust-macos/src/window.rs claim/claim_key (no focus condition) + werust-core/src/shortcuts.rs history rows resolve on any Focus; DECISIONS.md section 1 Cost, recorded honestly; README manual step 3)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q2 fields: id=q2 kind=stuck -->

**Your answer** (write below this line):

## Q3

**'task:shortcuts-and-mouse-history-buttons-on-the-macos-edge' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - Cmd+Left / Cmd+Right are claimed unconditionally in sendEvent:, so they are taken away from the URL bar's field editor and from page text fields, where Cmd+Arrow is macOS's standard move-to-beginning/end-of-line binding. A user editing an address (or typing in a web form) who presses Cmd+Left gets a history back navigation and loses the edit, which no Mac browser does. On GTK the identical table cost nothing because Alt+Arrow is not a text binding, so this is the first edge where the shared history chord collides with the platform's own text editing. resolve_chord already takes Focus, so a fix exists (gate the history rows on Focus::Page, or let the edge forward when the bar is being edited), but it belongs in the core the task forbids this edge to fork, so a human must choose. It is also not recorded: DECISIONS.md decision 1 lists Cmd+Arrow among the swallowed chords and calls it the conventional browser trade (true for Escape/Cmd+L, not for Cmd+Arrow on a Mac), decision 2 covers only menu key equivalents, and README's manual step 3 tests Cmd+Left only with the page focused. (crates/werust-macos/src/window.rs claim/claim_key (no focus condition) + werust-core/src/shortcuts.rs history rows resolve on any Focus; DECISIONS.md section 1 Cost, recorded honestly; README manual step 3)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q3 fields: id=q3 kind=stuck -->

**Your answer** (write below this line):

## Q4

**'task:shortcuts-and-mouse-history-buttons-on-the-macos-edge' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - Cmd+Left / Cmd+Right are claimed unconditionally in sendEvent:, so they are taken away from the URL bar's field editor and from page text fields, where Cmd+Arrow is macOS's standard move-to-beginning/end-of-line binding. A user editing an address (or typing in a web form) who presses Cmd+Left gets a history back navigation and loses the edit, which no Mac browser does. On GTK the identical table cost nothing because Alt+Arrow is not a text binding, so this is the first edge where the shared history chord collides with the platform's own text editing. resolve_chord already takes Focus, so a fix exists (gate the history rows on Focus::Page, or let the edge forward when the bar is being edited), but it belongs in the core the task forbids this edge to fork, so a human must choose. It is also not recorded: DECISIONS.md decision 1 lists Cmd+Arrow among the swallowed chords and calls it the conventional browser trade (true for Escape/Cmd+L, not for Cmd+Arrow on a Mac), decision 2 covers only menu key equivalents, and README's manual step 3 tests Cmd+Left only with the page focused. (crates/werust-macos/src/window.rs claim/claim_key (no focus condition) + werust-core/src/shortcuts.rs history rows resolve on any Focus; DECISIONS.md section 1 Cost, recorded honestly; README manual step 3)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q4 fields: id=q4 kind=stuck -->

**Your answer** (write below this line):

## Q5

**'task:shortcuts-and-mouse-history-buttons-on-the-macos-edge' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - Cmd+Left / Cmd+Right are claimed unconditionally in sendEvent:, so they are taken away from the URL bar's field editor and from page text fields, where Cmd+Arrow is macOS's standard move-to-beginning/end-of-line binding. A user editing an address (or typing in a web form) who presses Cmd+Left gets a history back navigation and loses the edit, which no Mac browser does. On GTK the identical table cost nothing because Alt+Arrow is not a text binding, so this is the first edge where the shared history chord collides with the platform's own text editing. resolve_chord already takes Focus, so a fix exists (gate the history rows on Focus::Page, or let the edge forward when the bar is being edited), but it belongs in the core the task forbids this edge to fork, so a human must choose. It is also not recorded: DECISIONS.md decision 1 lists Cmd+Arrow among the swallowed chords and calls it the conventional browser trade (true for Escape/Cmd+L, not for Cmd+Arrow on a Mac), decision 2 covers only menu key equivalents, and README's manual step 3 tests Cmd+Left only with the page focused. (crates/werust-macos/src/window.rs claim/claim_key (no focus condition) + werust-core/src/shortcuts.rs history rows resolve on any Focus; DECISIONS.md section 1 Cost, recorded honestly; README manual step 3)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q5 fields: id=q5 kind=stuck -->

**Your answer** (write below this line):

## Q6

**'task:shortcuts-and-mouse-history-buttons-on-the-macos-edge' was bounced — how should we proceed?**

> continuing the kept work/task-shortcuts-and-mouse-history-buttons-on-the-macos-edge: rebase onto the latest main conflicted (aborted, never auto-resolved) — run `requeue --reconcile` to non-destructively re-sync the mirror and retry the rebase (keeps the work). Last resort: `requeue --reset` DESTRUCTIVELY discards the branch and starts fresh.

<!-- q6 fields: id=q6 kind=stuck -->

**Your answer** (write below this line):

## Q7

**'task:shortcuts-and-mouse-history-buttons-on-the-macos-edge' was bounced — how should we proceed?**

> continuing the kept work/task-shortcuts-and-mouse-history-buttons-on-the-macos-edge: rebase onto the latest main conflicted (aborted, never auto-resolved) — run `requeue --reconcile` to non-destructively re-sync the mirror and retry the rebase (keeps the work). Last resort: `requeue --reset` DESTRUCTIVELY discards the branch and starts fresh.

<!-- q7 fields: id=q7 kind=stuck -->

**Your answer** (write below this line):

## Q8

**'task:shortcuts-and-mouse-history-buttons-on-the-macos-edge' was bounced — how should we proceed?**

> continuing the kept work/task-shortcuts-and-mouse-history-buttons-on-the-macos-edge: rebase onto the latest main conflicted (aborted, never auto-resolved) — run `requeue --reconcile` to non-destructively re-sync the mirror and retry the rebase (keeps the work). Last resort: `requeue --reset` DESTRUCTIVELY discards the branch and starts fresh.

<!-- q8 fields: id=q8 kind=stuck -->

**Your answer** (write below this line):

## Q9

**'task:shortcuts-and-mouse-history-buttons-on-the-macos-edge' was bounced — how should we proceed?**

> continuing the kept work/task-shortcuts-and-mouse-history-buttons-on-the-macos-edge: rebase onto the latest main conflicted (aborted, never auto-resolved) — run `requeue --reconcile` to non-destructively re-sync the mirror and retry the rebase (keeps the work). Last resort: `requeue --reset` DESTRUCTIVELY discards the branch and starts fresh.

<!-- q9 fields: id=q9 kind=stuck -->

**Your answer** (write below this line):

## Q10

**'task:shortcuts-and-mouse-history-buttons-on-the-macos-edge' was bounced — how should we proceed?**

> continuing the kept work/task-shortcuts-and-mouse-history-buttons-on-the-macos-edge: rebase onto the latest main conflicted (aborted, never auto-resolved) — run `requeue --reconcile` to non-destructively re-sync the mirror and retry the rebase (keeps the work). Last resort: `requeue --reset` DESTRUCTIVELY discards the branch and starts fresh.

<!-- q10 fields: id=q10 kind=stuck -->

**Your answer** (write below this line):

## Q11

**'task:shortcuts-and-mouse-history-buttons-on-the-macos-edge' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The focus-sensitive history fix covers only the URL bar. shortcut_focus is two-valued and returns Focus::Page for everything inside the WKWebView, so Cmd+Left while a user is typing in a PAGE text field is still claimed as GoBack and destroys the edit (the same class of regression the previous gate blocked, and something no Mac browser does). That could be an accepted limit, but both records claim the opposite: DECISIONS.md decision 8 (and decision 1) list a page text field among the cases the fix covers, and README manual step 3 instructs the one human who will ever test this that a page text field must behave the same way, which it will not. Record it as a known limit and correct both sentences, or escalate the core-side Focus vocabulary as a decision. (crates/werust-macos/src/window.rs shortcut_focus (only url_field.currentEditor / firstResponder distinguish UrlBar); docs/spikes/shortcuts-and-mouse-history-buttons-on-the-macos-edge/DECISIONS.md decision 8; same dir README.md manual step 3)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q11 fields: id=q11 kind=stuck -->

**Your answer** (write below this line):

## Q12

**'task:shortcuts-and-mouse-history-buttons-on-the-macos-edge' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The focus-sensitive history fix covers only the URL bar. shortcut_focus is two-valued and returns Focus::Page for everything inside the WKWebView, so Cmd+Left while a user is typing in a PAGE text field is still claimed as GoBack and destroys the edit (the same class of regression the previous gate blocked, and something no Mac browser does). That could be an accepted limit, but both records claim the opposite: DECISIONS.md decision 8 (and decision 1) list a page text field among the cases the fix covers, and README manual step 3 instructs the one human who will ever test this that a page text field must behave the same way, which it will not. Record it as a known limit and correct both sentences, or escalate the core-side Focus vocabulary as a decision. (crates/werust-macos/src/window.rs shortcut_focus (only url_field.currentEditor / firstResponder distinguish UrlBar); docs/spikes/shortcuts-and-mouse-history-buttons-on-the-macos-edge/DECISIONS.md decision 8; same dir README.md manual step 3)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q12 fields: id=q12 kind=stuck -->

**Your answer** (write below this line):

## Q13

**'task:shortcuts-and-mouse-history-buttons-on-the-macos-edge' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The focus-sensitive history fix covers only the URL bar. shortcut_focus is two-valued and returns Focus::Page for everything inside the WKWebView, so Cmd+Left while a user is typing in a PAGE text field is still claimed as GoBack and destroys the edit (the same class of regression the previous gate blocked, and something no Mac browser does). That could be an accepted limit, but both records claim the opposite: DECISIONS.md decision 8 (and decision 1) list a page text field among the cases the fix covers, and README manual step 3 instructs the one human who will ever test this that a page text field must behave the same way, which it will not. Record it as a known limit and correct both sentences, or escalate the core-side Focus vocabulary as a decision. (crates/werust-macos/src/window.rs shortcut_focus (only url_field.currentEditor / firstResponder distinguish UrlBar); docs/spikes/shortcuts-and-mouse-history-buttons-on-the-macos-edge/DECISIONS.md decision 8; same dir README.md manual step 3)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q13 fields: id=q13 kind=stuck -->

**Your answer** (write below this line):
