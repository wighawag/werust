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
