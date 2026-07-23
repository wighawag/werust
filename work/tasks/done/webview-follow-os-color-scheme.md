---
title: "werust WebView should FOLLOW the OS color scheme (light/dark), not default to light — so prefers-color-scheme and UA controls match the user's setting"
slug: webview-follow-os-color-scheme
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

Make werust's WebView RESPECT the operating system's color-scheme (light/dark) preference, instead of silently defaulting to light. FIELD EVIDENCE (v0.2.1, human, two screenshots of `mandalas.eth.limo` with the phone in DARK mode): in werust the nav `<button>`s render as SOLID WHITE boxes with INVISIBLE white-on-white text; in Firefox on the same device they are dark/readable. Root cause: werust's WebView is built with no color-scheme configuration, so WebKitGTK uses the LIGHT UA default for form controls even though the OS is in dark mode. So `prefers-color-scheme` reports light (mismatching the user's actual dark setting), and UA-styled controls get light defaults that break on the site's dark background.

IMPORTANT SCOPE (human decision): the fix is to FOLLOW the OS setting, NOT to force dark. Do NOT hard-code dark (or override the page's own `color-scheme`). If the OS is dark, the WebView reports dark (so `prefers-color-scheme: dark` matches and UA controls theme dark); if the OS is light, it reports light. A page that explicitly declares its own `color-scheme` is still respected. (Note: the specific mandalas button breakage is partly the SITE's own bug — it relies on UA defaults without declaring `color-scheme` or styling its buttons — but werust ignoring the OS dark preference is the werust-side bug this task fixes, and fixing it makes the dark-mode case correct.)

Wire the WebView to the OS/desktop color-scheme signal: on desktop, follow the GTK/desktop dark preference (e.g. `gtk-application-prefer-dark-theme` / the portal color-scheme, propagated so WebKitGTK reports the matching `prefers-color-scheme` and themes UA controls to match); on the mobile shells, follow the platform dark-mode setting (Android `UI_MODE_NIGHT` / `isLightTheme`; iOS `UITraitCollection.userInterfaceStyle`) so the WebView reports the matching scheme. Ideally follow LIVE OS changes, but at minimum apply the correct scheme at load time.

## Acceptance criteria

- [ ] With the OS in DARK mode, werust reports `prefers-color-scheme: dark` and UA-styled controls theme dark — `mandalas.eth.limo`'s nav buttons are readable (no white-on-white), at parity with Firefox on the same device.
- [ ] With the OS in LIGHT mode, werust reports light and a light page renders correctly (no forced dark).
- [ ] werust does NOT override a page's explicitly declared `color-scheme`; it only supplies the OS preference as the default the page and UA styling resolve against.
- [ ] Applied on desktop (WebKitGTK) and the mobile shells (Android + iOS) — following each platform's OS dark-mode setting — or explicitly tracked per the parity guard.
- [ ] The confirmed cause + the follow-OS approach are recorded (a finding referencing the screenshots + the WebKitGTK color-scheme/GTK-theme behaviour); the real visual check (dark and light) is documented.

## Blocked by

- None — can start immediately.

## Prompt

> Goal: make werust's WebView FOLLOW the OS color-scheme (light/dark) rather than defaulting to light. On a phone in dark mode, werust shows mandalas.eth.limo's `<button>`s as white boxes with invisible white-on-white text (Firefox shows them dark/readable) because werust reports light UA defaults regardless of the OS setting. Fix = report the OS preference so `prefers-color-scheme` and UA controls match. Do NOT force dark and do NOT override a page's own declared `color-scheme` — just stop ignoring the OS setting.
>
> Where to look: `crates/webview-renderer/src/backend.rs` — the `WebView::builder()...build()` sets no color scheme, so WebKitGTK uses the light default. WebKitGTK ties the page color scheme + UA control theming to the GTK theme (`gtk-application-prefer-dark-theme`; WebKit bugs 196685/197947, changeset 255342) — propagate the desktop/GTK dark preference so the web process reports the matching `prefers-color-scheme`. On mobile, read the platform dark-mode flag (Android `UI_MODE_NIGHT`; iOS `userInterfaceStyle`) and apply the matching scheme to the WebView (`crates/werust-android`, `crates/werust-ios`).
>
> Done = werust follows the OS light/dark setting on all three platforms: dark OS -> dark UA controls (mandalas buttons readable, parity with Firefox), light OS -> light, page-declared color-scheme respected, nothing force-overridden; the cause + approach recorded. FIRST reproduce with the OS in dark mode and confirm werust is reporting light. RECORD the diagnosis + the follow-OS decision durably.
