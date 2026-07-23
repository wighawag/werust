# Follow the OS color-scheme (light/dark), never force dark and never override a page's declared `color-scheme`

werust's WebView must FOLLOW the operating system's light/dark color-scheme
preference so `prefers-color-scheme` and UA-styled controls match the user's OS
setting, instead of silently defaulting to light. We supply the OS preference as the
WebView's DEFAULT only: we do NOT hard-code dark, and we do NOT override a page that
declares its own `color-scheme` (that resolution stays the engine's). This fixes the
field bug where, on an OS in dark mode, werust reported light UA defaults and
`mandalas.eth.limo`'s UA-styled nav buttons themed light on the site's dark
background (invisible white-on-white text), while Firefox on the same device rendered
them dark/readable.

## Status

accepted

## Context

WebKitGTK ties the page color scheme + UA control theming to the GTK theme: its web
process reports `prefers-color-scheme: dark` iff `gtk-application-prefer-dark-theme`
is set (WebKit bugs 196685 / 197947, changeset 255342). A plain GTK4 app does not
inherit the OS dark preference into that flag, so on a dark-mode desktop werust
reported light (reproduced: portal `color-scheme` = prefer-dark while
`gtk-application-prefer-dark-theme` defaulted to false). The mobile shells have the
analogous coupling: Android's System WebView sets `prefers-color-scheme` from the app
theme's `isLightTheme`, and iOS WKWebView from `UITraitCollection.userInterfaceStyle`.
Full diagnosis + reproduction: `docs/spikes/webview-follow-os-color-scheme/DIAGNOSIS.md`;
external ground truth: `work/notes/findings/webkitgtk-follows-os-color-scheme-via-gtk-theme-2026-07-23.md`.

## Decision

Follow the OS preference on all three platforms through ONE shared cross-platform
rule (`renderer::OsColorScheme`, `prefer_dark()` = only an explicit OS dark
preference), mirroring how `TrustPosture::after_verify` is the single shared trust
rule:

- **Desktop (WebKitGTK):** `WebViewRenderer::follow_os_color_scheme` reads the XDG
  desktop portal `org.freedesktop.appearance color-scheme` (0 = no pref, 1 = dark,
  2 = light) and sets `gtk-application-prefer-dark-theme` to match, tracking live
  `SettingChanged`. A missing portal falls back to no-preference (never forces dark).
- **Android:** the app theme `Theme.Werust` has a light variant (`res/values`) + a
  night variant (`res/values-night`) selected by the OS `UI_MODE_NIGHT` qualifier;
  the WebView reads the theme's `isLightTheme`, so `prefers-color-scheme` follows the
  OS.
- **iOS:** WKWebView follows `userInterfaceStyle` by default; the app pins no
  `UIUserInterfaceStyle` and leaves `overrideUserInterfaceStyle == .unspecified`.

## Considered Options

- **Force dark always (chosen: NO).** The precise mandalas fix at first looked like
  "set the WebView to dark". Rejected by explicit human scope decision: forcing dark
  breaks light-mode users and misrepresents the user's OS preference. The bug is that
  werust IGNORES the OS setting, not that it is not dark.
- **Enable algorithmic darkening / force-dark inversion on Android (NO).** That
  inverts pages that lack `prefers-color-scheme` and can override a page's intent; it
  is a different capability from "report the OS preference". Out of scope here.
- **Read the app's own GTK theme name instead of the portal (NO).** The GTK theme
  name is the app's dressing, not the OS preference; on GNOME it stays `Adwaita` in
  dark mode. The portal `color-scheme` is the cross-desktop OS signal.

## Consequences

- Dark OS -> dark UA controls (mandalas buttons readable, parity with Firefox); light
  OS -> light; a page's declared `color-scheme` is still respected (changeset 255342
  keeps a page light unless it declares dark support). Nothing is force-overridden.
- This is a cross-cutting user-facing capability, so it is tracked on all three
  platforms by the parity guard (`docs/platform-capability-matrix.toml`,
  `docs/adr/0005`): `follow-os-color-scheme`, implemented desktop + iOS + Android.
