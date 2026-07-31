# A comctl32 v6 manifest gives VISUAL STYLES, not DARK MODE — Win32 controls need an undocumented path

date: 2026-07-31
source:
- https://learn.microsoft.com/en-us/windows/win32/controls/cookbook-overview ("Enabling Visual Styles" — the canonical comctl32 v6 manifest; it says what the dependency buys, and dark mode is not in it)
- https://stackoverflow.com/questions/79331514/certain-controls-arent-drawing-in-dark-mode-for-native-win32-application (2025-01; answer: "The Dark theme for Win32 is mostly undocumented … implemented through several sub-styles. For the few controls that actually support it", with the `SetWindowTheme(…, L"DarkMode_Explorer")` family)
- https://gist.github.com/rounk-ctrl/b04e5622e30e0d62956870d5c22b7017 ("Win32 Dark Mode" — the uxtheme.dll ORDINALS every app that does this uses: `SetPreferredAppMode` #135, `AllowDarkModeForWindow` #133/#137, `RefreshImmersiveColorPolicyState` #104)
found-while: task `windows-release-packaging-leg` (embedding the application manifest)

## The claim this corrects

Several places in this repo said, in effect, "without a comctl32 v6 manifest the chrome's system-drawn push BUTTONs do not follow dark mode" — which reads as "with the manifest, they will". They will not.

- `docs/spikes/windows-win32-window-and-chrome/DECISIONS.md` §6 ("push BUTTONs are system-drawn and, without a v6 manifest, do not honour dark mode … until decision 4's manifest lands").
- The same spike's README, "what awaits real Windows hardware".
- `docs/platform-capability-matrix.toml`'s `follow-os-color-scheme` `windows` cell, which was marked `stubbed` **pointing at the manifest as the fix**.
- The `windows-release-packaging-leg` task body, which called the manifest "the fix" and made flipping that cell an acceptance criterion.

## What is actually true

A comctl32 v6 dependency in the application manifest switches the process from comctl32 5.82 to 6, which is what makes controls draw in the **current visual style** instead of the pre-Vista one. That is real and user-visible, and it is what the manifest buys.

**Dark mode for standard Win32 controls is a separate, UNDOCUMENTED mechanism.** There is no public API and no manifest entry for it. Applications that do it (Explorer, Notepad++, and every open-source example above) load `uxtheme.dll` and call functions exported **by ordinal only** — `SetPreferredAppMode` (135), `AllowDarkModeForWindow` (133/137), `RefreshImmersiveColorPolicyState` (104) — and then `SetWindowTheme(hwnd, L"DarkMode_Explorer" / L"DarkMode_CFD" / L"DarkMode_ItemsView", nullptr)` per control class. Even then the coverage is partial: the 2025 report above lists group-box text, check boxes / radio buttons and combo boxes as still drawing light, and the answer says the dark sub-styles "are hopelessly incomplete, and unless your use-case exactly matches Explorer's, you may find it distorted or outright better to fully owner-draw them yourself".

werust's own surfaces are unaffected either way: the chrome paints its `STATIC`s and its `EDIT` through `WM_CTLCOLORSTATIC` / `WM_CTLCOLOREDIT`, which the theme engine leaves alone, and it sets `DWMWA_USE_IMMERSIVE_DARK_MODE` on the title bar itself. It is specifically the push **BUTTON**s (`◀ ▶ ⟳ ✕ ⋮`), which are theme-drawn, that stay light in dark mode — before the manifest and after it.

## Consequence recorded elsewhere

- The manifest landed (`windows-release-packaging-leg`) and the `follow-os-color-scheme` `windows` cell **stays `stubbed`**, now pointing at `work/tasks/backlog/windows-chrome-dark-mode-for-common-controls.md` instead of at the packaging task, because that is the work that would actually close it. Flipping the cell on the manifest alone would have been exactly the flatter-the-column move the parity column's decision 1 refused.
- One more consequence of ordinal-only APIs, for whoever picks that task up: they are undocumented and version-gated, so anything built on them needs a build-number guard and a graceful no-op, which is a real design decision rather than a wiring job.
