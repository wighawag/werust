---
title: "UA-styled controls render with the wrong color scheme (white buttons + invisible white-on-white text on a dark page): set the WebView color scheme"
slug: webview-ua-color-scheme-controls-wrong
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

Fix the wrong default color scheme for UA-styled form controls. FIELD EVIDENCE (v0.2.1, human, two screenshots of `mandalas.eth.limo` — a dark-themed page — side by side, werust vs Firefox): in werust the page mostly renders CORRECTLY (the mandalas art, the orange headings, the rainbow bars, the layout are all right), but the nav `<button>` elements (MANDALAS / Wallet / About) render as SOLID WHITE BOXES with INVISIBLE white-on-white text, and the "Mint It" labels/icons are barely-visible dark-on-black. In Firefox the same buttons are dark/transparent with readable white text. So this is NOT missing base styling and NOT an ipfs sub-resource issue (it reproduces on a plain https load): it is specifically the USER-AGENT color scheme for form controls.

Root cause (confirmed by inspection + WebKitGTK behaviour): the desktop `WebView` is built with no color-scheme / GTK-theme configuration, so WebKitGTK renders UA-styled controls with the LIGHT default. WebKitGTK ties the page's effective color scheme + UA control theming to the GTK theme (`gtk-application-prefer-dark-theme`; WebKit bug 196685/197947, changeset 255342): with a light GTK theme, `<button>`/form controls get light UA defaults (white background), so on a dark page whose text is white, the button text becomes white-on-white and vanishes. Firefox applies a dark color scheme, so the UA button default is dark and readable.

Fix: make werust's WebView present the correct color scheme so UA-styled controls match a dark page. Concretely: set the WebView/GTK to a dark color scheme (e.g. `gtk-application-prefer-dark-theme`, or a dark GTK theme on the web process), and/or ensure the page's `prefers-color-scheme` is honoured, so `<button>` and other UA-styled controls render with dark defaults (readable) when the page is dark. Confirm the fix on the exact mandalas.eth.limo page (buttons readable) and that a light page still renders correctly. Apply the equivalent on the mobile shells where the same default applies.

## Acceptance criteria

- [ ] On `mandalas.eth.limo` (dark page) in werust, the nav buttons and "Mint It" controls are READABLE (no white-on-white, no invisible text) — at parity with Firefox.
- [ ] UA-styled form controls (`<button>`, inputs) pick up a color scheme consistent with the page instead of a hard light default; a dark page's controls are dark/readable and a light page's controls stay correct.
- [ ] The WebView color-scheme/theme is configured explicitly (the confirmed cause — no color-scheme handling today — is fixed and recorded), not left to an unintended light default.
- [ ] Applied on desktop (WebKitGTK) and the mobile shells where the same gap exists (or tracked per the parity guard).
- [ ] The confirmed root cause is recorded (a finding, referencing the two screenshots + the WebKitGTK color-scheme/GTK-theme behaviour); the real visual check is documented.

## Blocked by

- None — can start immediately.

## Prompt

> Goal: fix UA-styled controls rendering with the wrong color scheme — on a dark page (mandalas.eth.limo) werust shows the nav `<button>`s as WHITE boxes with INVISIBLE white-on-white text, while Firefox shows them dark/readable. The rest of the page renders fine, so this is specifically the user-agent color scheme for form controls, not base styling and not ipfs (it reproduces on plain https).
>
> Where to look: `crates/webview-renderer/src/backend.rs` — the `WebView::builder()...build()` sets NO color-scheme / GTK theme, so WebKitGTK uses the light UA default for controls. WebKitGTK ties the page color scheme + UA control theming to the GTK theme (`gtk-application-prefer-dark-theme`; WebKit bugs 196685/197947, changeset 255342). Set the WebView/GTK to present a dark color scheme (and/or honour `prefers-color-scheme`) so `<button>`/form controls are readable on a dark page. Apply on the mobile shells (`crates/werust-android`, `crates/werust-ios`) too.
>
> Done = mandalas.eth.limo's controls are readable in werust at parity with Firefox, UA controls follow the page's color scheme, a light page still renders correctly, the config is applied on desktop + mobile (or tracked), and the cause is recorded. FIRST reproduce on the exact page and confirm it is the control color scheme (not fonts). RECORD the diagnosis + the color-scheme decision durably.
