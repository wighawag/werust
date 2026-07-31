---
title: "Make the Win32 chrome's system-drawn controls follow dark mode, which the comctl32 v6 manifest does not do"
slug: windows-chrome-dark-mode-for-common-controls
blockedBy: [windows-release-packaging-leg]
covers: []
---

## What to build

The remaining half of ADR-0009 (`follow the OS light/dark setting, never force`) on Windows, and the reason `docs/platform-capability-matrix.toml`'s `follow-os-color-scheme` `windows` cell is still `stubbed` after the application manifest landed.

**What already works.** The PAGE follows the OS (`SetPreferredColorScheme(…AUTO)` on the WebView2 profile). The window READS the OS setting through the engine's one registry read, maps it with the shared `renderer::OsColorScheme` rule, re-reads it on `WM_SETTINGCHANGE`, paints its own `STATIC`s and `EDIT` dark through `WM_CTLCOLORSTATIC` / `WM_CTLCOLOREDIT`, and darkens the title bar with `DWMWA_USE_IMMERSIVE_DARK_MODE`.

**What does not.** The chrome's push `BUTTON`s (`◀ ▶ ⟳ ✕ ⋮`) are drawn by the theme engine and stay LIGHT on a dark-mode Windows, so a dark window has dark surfaces and light buttons. This was believed to be the comctl32 v6 manifest's job; it is NOT. The v6 dependency buys VISUAL STYLES only, and dark mode for standard Win32 controls has no public API at all: it is `uxtheme.dll` functions exported by ORDINAL (`SetPreferredAppMode` 135, `AllowDarkModeForWindow`, `RefreshImmersiveColorPolicyState`) plus per-class `SetWindowTheme(hwnd, L"DarkMode_Explorer" …)`, and even that is documented by the community as partial. Ground truth, with sources: `work/notes/findings/win32-common-controls-dark-mode-needs-more-than-a-v6-manifest-2026-07-31.md`.

So this task has a REAL DECISION to make before it has code to write, and it should be made explicitly rather than by picking the first snippet that compiles:

- **the undocumented uxtheme path** — small, matches what Explorer and Notepad++ do, and gets the platform's own dark visuals; but it is ordinal-only, needs a Windows build-number guard and a graceful no-op when the ordinals move, and it is a trust-carrying binary loading an undocumented entry point;
- **owner-drawing the handful of buttons werust has** — five glyph buttons, no third-party surface, and their colours would come from the SHARED `desktop-paint` palette the rest of the chrome already reads (which is the repo's own habit: one derivation, one palette); but werust then draws chrome the OS would otherwise draw, and must handle hover/pressed/disabled/focus itself;
- **doing nothing and re-marking the row honestly** — a legitimate outcome if the cost is judged not worth it, but then the cell's prose must say WHY rather than pointing at an open task forever.

Whatever is chosen must not fork ADR-0009's rule: the light/dark ANSWER stays `renderer::OsColorScheme` (`NoPreference` is light — werust never guesses dark), and any colour comes from the shared palette, not a new table. The macOS sibling has no equivalent gap (AppKit propagates the appearance into its controls), so this is Windows-only and must not change the shared carrier for the other edge's sake.

## Acceptance criteria

- [ ] On a dark-mode Windows the chrome's push buttons no longer read as light against dark surfaces — or, if the chosen answer is "do not", the matrix cell says so with the reason and this task is closed as a decision rather than left open.
- [ ] The choice between the uxtheme-ordinal path and owner-drawing is RECORDED with what it rejected and what it touches, beside the window's other decisions.
- [ ] The light/dark decision still comes from the shared `renderer::OsColorScheme` rule and any colour from the shared `desktop-paint` palette; no second reader and no second palette (the existing source-shape guard must stay green without weakening).
- [ ] If the uxtheme path is taken, an unsupported/older Windows is a graceful NO-OP (light chrome), never a crash or a hard error, and that fallback is tested rather than asserted.
- [ ] The `follow-os-color-scheme` `windows` cell in `docs/platform-capability-matrix.toml` is updated to match what is now true, naming what proves it — and it is proven by a HUMAN looking at a dark-mode Windows box, since no CI runner flips its colour scheme.
- [ ] `docs/spikes/windows-win32-window-and-chrome/README.md`'s manual-verification step 9 is updated to say what the reader should now expect to see.

## Prompt

> Goal: close the last ADR-0009 gap on Windows. The chrome's push buttons stay LIGHT on a dark-mode Windows, and the comctl32 v6 manifest that landed with `windows-release-packaging-leg` does not fix that — dark mode for standard Win32 controls is an undocumented uxtheme-ordinal path (see the finding note). Decide, explicitly and on the record, between that path (with a build-number guard and a graceful no-op) and owner-drawing werust's five glyph buttons from the SHARED `desktop-paint` palette, then implement it without forking the shared `OsColorScheme` rule or minting a second palette. Confirm it BY HAND on a dark-mode Windows box (no runner can), update the `follow-os-color-scheme` `windows` matrix cell to what is then true, and update manual step 9 of the window spike's README.
