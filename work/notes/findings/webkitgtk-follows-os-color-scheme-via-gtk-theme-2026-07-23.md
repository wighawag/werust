---
title: "WebKitGTK reports `prefers-color-scheme` + themes UA controls from `gtk-application-prefer-dark-theme`; a plain GTK4 app does NOT inherit the OS dark preference, so werust reported light in OS dark mode"
date: 2026-07-23
status: verified
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
kind: finding
source:
  - https://bugs.webkit.org/show_bug.cgi?id=196685
  - https://bugs.webkit.org/show_bug.cgi?id=197947
  - https://trac.webkit.org/changeset/255342/webkit
  - https://developer.android.com/develop/ui/views/layout/webapps/dark-theme
  - https://www.w3.org/TR/css-color-adjust-1/#propdef-color-scheme
---

## Ground truth (gathered while building `webview-follow-os-color-scheme`)

- **WebKitGTK ties the page color scheme + UA control theming to the GTK theme.**
  Its web process reports `prefers-color-scheme: dark` (and themes UA-styled
  controls dark) iff `gtk-application-prefer-dark-theme` is set in GTK settings
  (WebKit bugs 196685 "Support prefers-color-scheme media query" + 197947, changeset
  255342). Per 255342, the page is kept on the LIGHT theme UNLESS the page declares
  dark support in its `color-scheme` property; the web process is still told a dark
  theme is in use so a page that prefers dark gets it. So the GTK flag is the OS
  DEFAULT the page resolves against, NOT an override of a page's own declaration.
- **A plain GTK4 app does NOT inherit the OS dark preference into that flag.** On a
  dark-mode GNOME, `org.gnome.desktop.interface color-scheme` = `prefer-dark` and the
  XDG portal `org.freedesktop.appearance color-scheme` = `1` (prefer dark), yet a
  plain GTK4 app's `gtk-application-prefer-dark-theme` defaults to `false` (and
  `gtk-theme-name` stays `"Adwaita"`, not the dark variant). Verified by direct
  reproduction on this machine 2026-07-23 (see
  `docs/spikes/webview-follow-os-color-scheme/DIAGNOSIS.md`). That mismatch is the
  werust-side bug: WebKitGTK read `false` -> reported LIGHT UA defaults in OS dark
  mode -> `mandalas.eth.limo`'s UA-styled buttons themed light on a dark page ->
  invisible white-on-white text.
- **The OS signal to follow is the XDG desktop portal `color-scheme` value:** `0` =
  no preference, `1` = prefer dark, `2` = prefer light (freedesktop settings spec).
  It is the cross-desktop OS preference, distinct from the app's own GTK theme name,
  and it is readable over the session bus without any portal-specific dependency.
- **Android's System WebView sets `prefers-color-scheme` from the app theme's
  `isLightTheme`** (Android "Darken web content in WebView"): `isLightTheme=true` (or
  unset) -> `light`, else `dark`. A page's own `color-scheme` is still applied. So
  following the OS on Android = giving the app a DayNight theme (light + night
  variants) whose `isLightTheme` tracks `UI_MODE_NIGHT`; force-dark / algorithmic
  darkening is a SEPARATE opt-in that inverts pages lacking `prefers-color-scheme`
  and is NOT wanted here (it would override the page).
- **iOS WKWebView follows `UITraitCollection.userInterfaceStyle` by default,** which
  follows the OS unless the app pins `UIUserInterfaceStyle` in Info.plist or sets
  `overrideUserInterfaceStyle`. werust does neither, so iOS already follows the OS.

## How werust used it

The follow-OS fix (never force dark, never override a page's declared
`color-scheme`) applies the OS preference as the WebView's default on all three
platforms through the one shared `renderer::OsColorScheme` rule: desktop reads the
portal and sets `gtk-application-prefer-dark-theme` to match (tracking live
`SettingChanged`); Android uses a DayNight `Theme.Werust` whose `isLightTheme`
follows `UI_MODE_NIGHT`; iOS keeps WKWebView's default OS-following (no
`UIUserInterfaceStyle` pin, `overrideUserInterfaceStyle == .unspecified`). Recorded
as `docs/adr/0009-follow-the-os-color-scheme-not-force-dark.md`; diagnosis +
reproduction in `docs/spikes/webview-follow-os-color-scheme/DIAGNOSIS.md`.
