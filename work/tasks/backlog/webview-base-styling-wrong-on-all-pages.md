---
title: "Pages render with wrong styling on ALL loads (even plain https): fix the WebView base rendering config (default fonts / WebKitSettings / UA styling)"
slug: webview-base-styling-wrong-on-all-pages
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

Fix the base rendering so a normal web page renders CORRECTLY (right colours, right layout, right fonts). KEY FIELD FINDING (v0.2.1, human): the styling is wrong even on `https://mandalas.eth.limo` — a PLAIN `https://` load through the ordinary path, NOT werust's `ipfs://` verified path. So this is NOT an `ipfs://` sub-resource problem: EVERY page renders wrong, which points at werust's own WebView/renderer configuration, common to all loads. (This is why the original "sub-resource resolution" framing was wrong — a plain https page has no werust-mediated sub-resources yet still renders wrong.)

Diagnose against a PLAIN https page first (not ipfs), then fix. The desktop `WebView` is built with `WebView::builder().user_content_manager(...).web_context(...).build()` and ZERO `WebKitSettings` configured (`crates/webview-renderer/src/backend.rs`). Likely causes to investigate, in order:
- **Default fonts not configured / not available** — WebKitGTK renders with a default serif/sans/monospace + default size; if no usable fonts are installed in the run environment, or the default font family/size is not set, text colour looks fine but metrics/appearance are off (wrong-looking text, off layout). Set sensible default font family + size + standard/serif/sans/monospace via `WebKitSettings`.
- **Missing `WebKitSettings` altogether** — construct and attach a `Settings` object with the defaults a browser needs (default font sizes, enable smooth scrolling / JS as appropriate, a real user-agent). Absence of a settings object is the smoking-gun candidate.
- **UA / default stylesheet** — confirm the WebView applies its user-agent stylesheet (element defaults). If a user-content stylesheet or a reset is interfering, or the UA sheet is not applied, element defaults (colours, margins) break.
- **Color-scheme / theme** — a GTK dark theme or a missing `prefers-color-scheme` default can make text colour wrong; confirm the WebView's default is sane.

Fix the confirmed cause(s) so a plain https page renders at parity with a normal browser, then confirm an ipfs:// site inherits the fix. Apply consistently to the mobile shells where the same base-config gap exists.

## Acceptance criteria

- [ ] A plain `https://` page renders with correct colours, layout, and fonts on desktop (WebKitGTK) — at parity with a normal browser; the v0.2.1 wrong-text-colour / off-layout symptom is gone.
- [ ] The WebView is configured with sensible base settings (default fonts + sizes via `WebKitSettings`, a real user-agent, UA styling intact); the confirmed root cause is recorded (a finding), not guessed.
- [ ] An `ipfs://` verified site inherits the same correct base rendering (styling correct once fetched).
- [ ] The mobile shells apply the equivalent base rendering config where the same gap exists (or it is tracked per the parity guard).
- [ ] Tests/fixtures cover the config (the settings are attached with the expected defaults); the real visual check is documented since a pixel assertion is impractical in CI.

## Blocked by

- None — can start immediately. (Distinct from `ipfs-site-mobile-black-page`: the mobile black page is ipfs-specific since the same site renders fine via https on mobile.)

## Prompt

> Goal: fix werust's BASE rendering so a normal page renders correctly (colours/layout/fonts). FIELD FINDING: styling is wrong even on a plain `https://mandalas.eth.limo` load, so this is werust's WebView/renderer config, NOT `ipfs://` sub-resources. Diagnose against a plain https page, fix the config, confirm ipfs inherits it.
>
> Where to look: `crates/webview-renderer/src/backend.rs` — the `WebView::builder()...build()` attaches NO `WebKitSettings`. Prime suspects: no default font family/size configured (WebKitGTK font defaults + font availability), no settings object at all, UA stylesheet / color-scheme. The mobile shells (`crates/werust-android`, `crates/werust-ios`) likely share the gap. Compare against how a correctly-rendering embed configures WebKitGTK settings.
>
> Done = a plain https page renders correctly on desktop (and the config is applied on mobile / tracked), an ipfs site inherits it, and the confirmed root cause is recorded as a finding. FIRST reproduce the wrong styling on a plain https page and capture what is actually wrong (fonts? colours? layout?) before changing config. RECORD the diagnosis + the settings decision durably.
