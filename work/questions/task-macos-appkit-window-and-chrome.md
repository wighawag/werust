<!-- dorfl-sidecar: item=task:macos-appkit-window-and-chrome type=task slug=macos-appkit-window-and-chrome allAnswered=false -->

## Q1

**'task:macos-appkit-window-and-chrome' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The macos-14 leg was EXTENDED but never DISPATCHED, so no line of the AppKit half (about 1300 lines in src/window.rs plus the 370-line window_smoke) has ever been compiled against a macOS SDK or executed. Acceptance criterion 1 (a native macOS window RENDERS a page through the WKWebView backend, with every surface present) has no runtime evidence at all. The task body said this trap is now avoidable (gh workflow run macos-renderer.yml --ref <branch>) and asked for a dispatched run recorded; the sibling engine task was blocked at Gate 2 on exactly this condition. Can the human run the macos-renderer leg against this PR (its path filters already include crates/werust-macos/**) and confirm cargo build/test -p werust-macos and the window_smoke step are green, recording the run in the spike README's What CI proved section, before this lands? (docs/spikes/macos-appkit-window-and-chrome/README.md, section What CI proved: Nothing about this window, yet - the leg has not been run against this code. DECISIONS.md 11 repeats it. Everything else in the task is delivered; only hardware evidence is missing.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):

## Q2

**'task:macos-appkit-window-and-chrome' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The macos-14 leg was EXTENDED but never DISPATCHED, so no line of the AppKit half (about 1300 lines in src/window.rs plus the 370-line window_smoke) has ever been compiled against a macOS SDK or executed. Acceptance criterion 1 (a native macOS window RENDERS a page through the WKWebView backend, with every surface present) has no runtime evidence at all. The task body said this trap is now avoidable (gh workflow run macos-renderer.yml --ref <branch>) and asked for a dispatched run recorded; the sibling engine task was blocked at Gate 2 on exactly this condition. Can the human run the macos-renderer leg against this PR (its path filters already include crates/werust-macos/**) and confirm cargo build/test -p werust-macos and the window_smoke step are green, recording the run in the spike README's What CI proved section, before this lands? (docs/spikes/macos-appkit-window-and-chrome/README.md, section What CI proved: Nothing about this window, yet - the leg has not been run against this code. DECISIONS.md 11 repeats it. Everything else in the task is delivered; only hardware evidence is missing.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q2 fields: id=q2 kind=stuck -->

**Your answer** (write below this line):
