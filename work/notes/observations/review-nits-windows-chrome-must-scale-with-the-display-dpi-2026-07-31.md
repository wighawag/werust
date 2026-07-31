---
title: review-gate non-blocking nits for 'windows-chrome-must-scale-with-the-display-dpi' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: windows-chrome-must-scale-with-the-display-dpi
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'windows-chrome-must-scale-with-the-display-dpi' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The debug view's list COLUMN widths are scaled once at creation (from the BROWSER window's DPI) and are never re-set on a DPI change, but the spike README's manual step 5 tells the human to expect 'its title, CLEAR button, tabs and list columns re-scale' after a cross-monitor drag. Since a human on hardware is the ONLY verification this task has, either re-apply the column widths in relayout_debug_window_of or correct the step's expected result.
  (window.rs:857 and :877 call add_column with metrics from controller.chrome.metrics(); relayout_debug_window_of places title/clear/tabs/lists only, and LVM_SETCOLUMN is never sent again. README step 5.)
- Coherence: the change calls dpi.rs 'the DPI seam' throughout code, tests and docs, while CONTEXT.md reserves seam for a hot-swappable BACKEND interface (Renderer, ScriptEngine, Fetcher) and explicitly contrasts it with a painter. The builder recorded the tension and kept the task's wording, always qualified as 'the DPI seam'. A human should pick one of the two resolutions it names (pin the loose sense in the glossary, or rename to Dpi scale / Metrics) so the next author cannot re-fork the word.
  (DECISIONS.md section 7; CONTEXT.md line 24 glossary entry for painter vs seam.)
- Ratify an unrecorded in-scope decision: six new public methods were added to BrowserWindow (dpi, metrics, url_bar, trust, control_rect, page_client_rect) that hand out raw chrome HWNDs purely so the smoke can measure widgets. That widens the crate's public surface for a test-only need and is not in DECISIONS.md. Keep, or narrow to one measurement method?
  (window.rs BrowserWindow impl, added block after window().)
- Ratify recorded decision 3: ONE chrome font per process, so a debug view alone on a differently scaled monitor keeps text one size behind until the browser window is dragged too. Reversal is local (per-window HFONT plus release on WM_CLOSE). Accept the limitation?
  (DECISIONS.md section 3; rescale_font pushes the browser window's font to debug.controls(); debug_wndproc's WM_DPICHANGED does rect plus relayout only.)
- The finding that the DPI work does NOT close the phantom-scrollbar task (nothing horizontal changed) is recorded only in work/notes/observations/dpi-work-does-not-touch-the-page-container-width-2026-07-31.md; the sibling task windows-page-shows-a-phantom-horizontal-scrollbar still says to re-check on top of the DPI fix and carries no pointer to that note. A one-line pointer in the sibling task would stop the next agent re-deriving it.
  (work/tasks/backlog/windows-page-shows-a-phantom-horizontal-scrollbar.md hypothesis 1 and its criterion 'If the DPI task already fixed it, that is recorded here'.)
- Nit: the raw-pixel guard scans the layout functions' RAW source (not code_only), so any future COMMENT containing a digit inside relayout or relayout_debug_window reds the Ubuntu gate with a misleading 'still carries raw 96-DPI pixels' message. Stripping comments first would keep the guard honest.
  (tests/windows_window_shape.rs unscaled_literals, called on between(&chrome, ...) rather than on code_only.)
