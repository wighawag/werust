# Diagnosis: werust ignored the OS color-scheme, reporting light in OS dark mode

Task: `webview-follow-os-color-scheme`. This records the confirmed cause of the
mandalas.eth.limo white-on-white buttons and the follow-OS fix, with the real
before/after reproduction on desktop. See also the finding
`work/notes/findings/webkitgtk-follows-os-color-scheme-via-gtk-theme-2026-07-23.md`
and the decision ADR `docs/adr/0009-follow-the-os-color-scheme-not-force-dark.md`.

## Field evidence (v0.2.1, human, two screenshots)

Side-by-side screenshots of `mandalas.eth.limo` with the phone in DARK mode: in
werust the nav `<button>`s (MANDALAS / Wallet / About) render as SOLID WHITE boxes
with INVISIBLE white-on-white text; in Firefox on the same device they are
dark/readable. The page otherwise renders correctly (art, headings, layout), and
the breakage reproduces on plain `https://mandalas.eth.limo` (not werust's `ipfs://`
path), so it is neither missing base styling nor an ipfs sub-resource problem. It is
the USER-AGENT color scheme for form controls.

## Confirmed cause (desktop, reproduced on this machine 2026-07-23)

WebKitGTK ties the page color scheme + UA control theming to the GTK theme: its web
process reports `prefers-color-scheme: dark` (and themes UA controls dark) iff
`gtk-application-prefer-dark-theme` is set (WebKit bugs 196685 / 197947, changeset
255342). A plain GTK4 app does NOT inherit the desktop's OS dark preference into that
flag. Reproduced on a dark-mode GNOME:

- OS preference (the real signal), both agree the OS is DARK:
  - `gsettings get org.gnome.desktop.interface color-scheme` -> `'prefer-dark'`
  - XDG portal `org.freedesktop.appearance color-scheme` -> `1` (prefer dark)
- werust's actual GTK state (the bug):
  - `gtk-theme-name` = `"Adwaita"` (not the dark variant)
  - `gtk-application-prefer-dark-theme` (default) = `false`

So WebKitGTK read `false` and reported the LIGHT UA default even though the OS is
dark -> `prefers-color-scheme: dark` never matched -> UA controls themed light on a
dark page -> white-on-white buttons.

## The fix (follow the OS, never force)

Read the OS preference from the XDG desktop portal
(`org.freedesktop.appearance color-scheme`, the cross-desktop OS signal) and set
`gtk-application-prefer-dark-theme` to MATCH it, tracking live changes. Only an
explicit OS dark preference sets prefer-dark; light / no-preference keep light. This
does NOT force dark and does NOT override a page's declared `color-scheme`: changeset
255342 keeps the page on the light theme UNLESS the page declares dark support, so
the flag only supplies the OS default the page + UA styling resolve against.

## Before/after (real desktop run of `WebViewRenderer::follow_os_color_scheme`)

On this dark-mode machine, constructing the real backend and calling
`follow_os_color_scheme()`:

```
prefer-dark BEFORE follow = false   # the bug: werust reports light despite dark OS
prefer-dark AFTER  follow = true    # fixed: werust now matches the dark OS
```

So werust now follows the OS: dark OS -> `prefers-color-scheme: dark` + dark UA
controls (mandalas buttons readable, parity with Firefox); a light OS keeps light.

## Per-platform mechanism

- **Desktop (WebKitGTK):** `WebViewRenderer::follow_os_color_scheme` reads the portal
  and sets `gtk-application-prefer-dark-theme`; the pure portal-value -> `OsColorScheme`
  mapping is `os_color_scheme_from_portal` (0 = no pref, 1 = dark, 2 = light, unknown
  = no pref), pinned by a headless test.
- **Android (System WebView):** the WebView sets `prefers-color-scheme` from the app
  theme's `isLightTheme`. `Theme.Werust` has a light variant (`res/values`) + a night
  variant (`res/values-night`) selected by the OS `UI_MODE_NIGHT` qualifier, so it
  follows the OS. No force-dark / algorithmic darkening (which would override the
  page's `color-scheme`).
- **iOS (WKWebView):** WKWebView reports `prefers-color-scheme` from
  `userInterfaceStyle`, which follows the OS by default. The app pins no
  `UIUserInterfaceStyle` and leaves `overrideUserInterfaceStyle == .unspecified`, so
  it follows the OS without forcing.

## Real visual check (dark and light)

- Dark OS was reproduced and the before/after flag flip verified on desktop (above).
- Light OS: the same code path sets `gtk-application-prefer-dark-theme = false` for
  portal value `2` (prefer light) or `0` (no preference), so a light page renders
  correctly and nothing is forced dark (pinned by
  `desktop_maps_the_xdg_portal_color_scheme_to_the_os_signal`).
