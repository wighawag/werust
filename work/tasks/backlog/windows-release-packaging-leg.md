---
title: "Package the Windows desktop build in CI: an unsigned zip attached to the tagged Release, with the app manifest the chrome is waiting for"
slug: windows-release-packaging-leg
blockedBy: [windows-win32-window-and-chrome]
covers: []
---

## What to build

The Windows twin of `macos-release-packaging-leg`, and the last sub-task of the Windows split in `docs/adr/0011-webview2-for-windows.md`. `windows-win32-window-and-chrome` shipped a window a person can use; nothing yet ships it to them.

Build `werust-windows` for `x86_64-pc-windows-msvc` on the `windows-latest` runner and attach a zip to the tagged GitHub Release beside the existing desktop Linux binary, Android APK and iOS Simulator `.app`.

**Reuse the existing job shapes.** `.github/workflows/release.yml` already runs decoupled per-platform jobs (`android-apk`, `ios-simulator-app`); model this on them — `needs: verify`, idempotent `gh release create`, dry-run artifact upload — as a SIBLING job, so a Windows failure cannot block another platform's artifact and vice versa. The runner and the toolchain are already proven by `.github/workflows/windows-renderer.yml`.

**The APP MANIFEST is part of this task, and it is not cosmetic.** The window deliberately ships without one (`docs/spikes/windows-win32-window-and-chrome/DECISIONS.md` §4), because embedding a Win32 resource is a build-time and packaging concern. Two real user-visible consequences are waiting on it, both recorded in that spike's "what awaits real Windows hardware":

- **Visual styles.** Without a comctl32 v6 dependency in the manifest the chrome draws in the pre-Vista style, and system-drawn push buttons do not follow dark mode (so a dark-mode window has dark surfaces and light buttons — a visible ADR-0009 gap that no code change in the window can close).
- **DPI awareness.** Without `<dpiAwareness>PerMonitorV2</dpiAwareness>` the chrome is bitmap-scaled on a 150%/200% display: laid out correctly, but blurry, and untested across monitors with different scale factors.

Add the manifest (an `embed-resource`-style build script, or the linker's `/MANIFEST` inputs — pick one and record why), then RE-CHECK the two consequences above by hand on a real Windows box and update that spike's list, because CI cannot judge either.

**The visual-styles half is a tracked PARITY GAP, so this task also owns a matrix cell.** `windows-parity-column-and-stub-tasks` had to mark `follow-os-color-scheme`'s `windows` cell `stubbed` and point it HERE, because the light-buttons-in-dark-mode behaviour is a user-visible ADR-0009 shortfall no code in the window can close and the manifest is the fix — see `work/notes/observations/windows-parity-column-decisions-2026-07-31.md` (decision 1) for why a second manifest task was NOT cut instead. So when the manifest lands and the by-hand check confirms the chrome follows dark mode, flip that cell to `implemented` in the same change, naming what proves it.

**Unsigned, deliberately.** No code signing, no installer: those need a certificate and are a separate follow-on, the Windows analogue of `android-apk-signing` (and of the macOS notarization gap). An unsigned zip means SmartScreen will warn on first run; say so in the README rather than pretending the artifact is ready for general distribution. If you add a signing path later, follow the Android precedent: gate on a secrets-presence env flag, graceful no-op without it, honest artifact naming.

**The WebView2 Runtime is a RUNTIME DEPENDENCY, and the artifact must say so.** It ships with Windows 11 and is on most Windows 10 machines, but "no installer ever needed" is not a promise werust can make (`docs/adr/0011` finding 6). The engine already fails honestly, naming the runtime and pointing at the download; the release notes / README must name it too, so a user meets that fact before the download rather than after.

**Version:** the reported version must come from the SAME version source the Rust core resolves (`WERUST_VERSION` on a tag, else `build.rs`'s `git describe`), never a second one. The Android sibling task `android-apk-version-from-the-release-tag` exists precisely because that was got wrong there.

Pin the workflow shape with a test in the existing `crates/werust-core/tests/release_plumbing_shape.rs` style, which parses the workflow inside the pure-Rust `verify` gate (no Windows, no network).

ADR sizing: 2 to 4 person-days.

> FORWARD-POINTER (planted by the conductor at Gate-3 of `windows-backend-error-mapping-and-leg-header-accuracy`, 2026-07-31). **You will be FORCED to edit `crates/werust-core/tests/windows_renderer_leg_shape.rs`, by design.** That task converted the leg's `pull_request` guard from a must-have/must-not-have pair into an EXACT-set pin (`PULL_REQUEST_FILTER`), so any new PR-filter path this leg needs reds the Ubuntu gate until you update the constant in the same change. That is intended (a widening must be a decision, not an accretion), not a bug to route around; do not loosen the pin to make your change easier. While you are in those exact files, carry these two three-line prose corrections that Gate-2 raised and that are not worth a task of their own:
>
> - `crates/windows-renderer/src/pure.rs`'s rustdoc, the new `DECISIONS.md` item 1, and the pure test's assertion message all say the environment-creation error LEADS with the platform detail. It TRAILS it, after a colon. Either reword all three to "carries"/"keeps", or reorder the message to put the `HRESULT` first, and make the three agree. This is the same doc-overclaims-the-tool class that task existed to close, so leaving it would be a small joke at the repo's expense.
> - Two guard comments overclaim what they enforce: `macos_backend_shape.rs` says pinning `desktop-paint` means "the next widening of either filter is an edit to a test rather than an accretion", but that test only asserts `contains(...)` plus a two-entry deny list, so a NEW macOS PR-filter path still lands silently; and `windows-renderer.yml`'s header says the list and the header "neither can move without the other going red", yet no test holds the header PROSE. Soften both comments to claim only what is really pinned, or make the macOS pin exact like the Windows one. Do not let a comment promise a guard that does not exist.

## Acceptance criteria

- [ ] A tagged release attaches a Windows desktop artifact (a zip containing `werust-windows.exe`) alongside the existing artifacts.
- [ ] The application manifest is embedded: comctl32 v6 (visual styles) AND per-monitor-v2 DPI awareness, with the chosen embedding mechanism recorded.
- [ ] The reported version comes from the existing version source; no second source is introduced.
- [ ] The job runs on the existing `windows-latest` runner shape, decoupled so it cannot be blocked by (or block) the other platform legs, and the dry-run path uploads an artifact without publishing.
- [ ] The workflow shape is pinned by a `release_plumbing_shape.rs`-style test; network-isolated.
- [ ] The README states the artifact is UNSIGNED (and what SmartScreen will do), and that the WebView2 Runtime is required, and names the signing follow-on.
- [ ] `docs/spikes/windows-win32-window-and-chrome/README.md`'s "what awaits real Windows hardware" list is updated for the two items the manifest closes — by MANUAL check on a real box, not by assertion.
- [ ] Once that manual check confirms the chrome follows dark mode, the `follow-os-color-scheme` row's `windows` cell in `docs/platform-capability-matrix.toml` flips from `stubbed` to `implemented`, naming what proves it; the parity guard stays green with no weakening. (If the check does NOT confirm it, the cell stays `stubbed` and says what is still wrong — do not flip it on the manifest alone.)

## Prompt

> Goal: add the Windows desktop packaging leg to the release workflow. Build `werust-windows` for `x86_64-pc-windows-msvc` on `windows-latest` and attach a zip to the tagged Release beside the Linux binary, Android APK and iOS Simulator `.app`, as a SIBLING job modelled on the existing `android-apk` / `ios-simulator-app` jobs (`needs: verify` decoupling, idempotent `gh release create`, dry-run artifact upload). Embed the application MANIFEST the window is waiting for — comctl32 v6 for visual styles (without it the chrome is classic-styled and its buttons ignore dark mode) and per-monitor-v2 DPI awareness (without it the chrome is bitmap-scaled on a HiDPI display) — then re-check both BY HAND on a real Windows box and update `docs/spikes/windows-win32-window-and-chrome/README.md`, because CI cannot judge either. Unsigned only: no certificate, no installer (a separate follow-on; when it comes, copy the Android secrets-presence-flag pattern), and say plainly that SmartScreen will warn and that the WebView2 Runtime is required. The version comes from the SAME source the Rust core uses, never a second one. Pin the workflow shape with a `release_plumbing_shape.rs`-style test that parses the YAML inside the pure-Rust verify gate.
