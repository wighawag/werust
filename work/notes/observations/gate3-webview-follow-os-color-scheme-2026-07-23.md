---
title: "Gate-3 conductor review: webview-follow-os-color-scheme (APPROVE)"
date: 2026-07-23
status: open
reviewOf: webview-follow-os-color-scheme
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: 197c1c2
---

## Verdict: APPROVE ✅ — merged as 197c1c2 (field-issue #2, corrected diagnosis)

Fixes the mandalas.eth.limo white-on-white button issue the human's two screenshots pinned: werust ignored the OS dark-mode setting and defaulted to light UA control theming.

## Diagnosis journey (worth recording)
"styling wrong" -> (human: also wrong on plain https) NOT ipfs sub-resources -> (screenshots) specifically UA-styled `<button>` controls white-on-white on a dark page -> (root cause) werust reports LIGHT regardless of OS -> (human scope decision) FOLLOW the OS setting, do NOT force dark. Each turn narrowed a vague symptom to a precise, correctly-scoped fix.

## Acceptance criteria — all met
- FOLLOWS the OS light/dark scheme, never forces: shared `OsColorScheme{Dark,Light,NoPreference}` rule (only explicit OS-dark asks dark). Test `os_color_scheme_follows_the_os_and_never_forces_a_scheme`. ADR-0009 records FOLLOW-not-force.
- Desktop reads the XDG portal `org.freedesktop.appearance color-scheme` (the real OS preference), maps to `gtk-application-prefer-dark-theme`, kept LIVE via `SettingChanged`. Test `desktop_maps_the_xdg_portal_color_scheme_to_the_os_signal`.
- Does NOT override a page's declared `color-scheme` (only supplies the OS default; cites WebKit changeset 255342).
- Cross-platform: Android (`values-night/themes.xml`, `UI_MODE_NIGHT`) + iOS (`userInterfaceStyle`); parity guard satisfied.
- Diagnosis spike documents the exact GNOME/portal quirk.

## Note
The mandalas button breakage is partly the SITE's own bug (relies on UA defaults without declaring `color-scheme`); werust's correct fix is to stop ignoring the OS preference, which it now does.

## Gate-2 nits: 3 non-blocking, recorded.
