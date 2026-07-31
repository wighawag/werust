---
title: "The Windows shell ran on REAL hardware for the first time (2026-07-31), and it works — with three native-integration defects CI could never have seen"
date: 2026-07-31
status: open
---

## What happened

The human built and ran `werust-windows` on their own Windows machine and reported: **"I just tested on windows and it works!"** — with a screenshot showing `https://example.com/` rendered through the WebView2 backend, the toolbar, URL bar, `unverified origin` trust indicator and status line all present and correct.

This is a first for this project and it retires a caveat, so it is recorded as a FINDING rather than an observation. Every Windows claim in this repo until now has been explicitly "proven on a `windows-latest` CI runner; hardware pending" (`docs/adr/0011` Amendment 1, and the "What still awaits real Windows hardware" section of both Windows spike READMEs). The load-bearing half of that caveat is now discharged: the shell launches, creates its window, realises WebView2, navigates, and paints its chrome from the shared derivation, on a real desktop in front of a person.

**What is still NOT verified** even by this run: input/focus routing beyond what was clicked, the `ipfs://` trust hooks against a real gateway on this machine, the debug view, the ⋮ menu contents, devtools, resize/maximise behaviour, and multi-monitor moves. This was a launch-and-look, not a sweep. It should not be read as retiring the manual-verification list.

## The three defects the human found, all of them native-integration

None of these is a browsing bug — the engine, the navigation and the trust posture all did their jobs. All three are the shell failing to behave like a native Windows application, and all three are invisible to CI, which has no display, no DPI, and never looks at a window.

### 1. The chrome is drawn at half size on a high-DPI display

Diagnosed from the screenshot plus the source. The PAGE is correctly scaled (WebView2 performs its own DPI handling); the CHROME is tiny.

**Cause, and it is ours, from the same day.** `windows-release-packaging-leg` added `crates/werust-windows/app.manifest` declaring `PerMonitorV2` DPI awareness. That declaration tells Windows: do NOT bitmap-scale this process, it scales itself. The Win32 code then does not: `win32.rs::ui_font()` hard-codes `CreateFontW(-15, …)`, and every layout metric is a raw 96-DPI pixel (`chrome.rs`'s `MARGIN: i32 = 8`, `PROGRESS_HEIGHT: i32 = 3`, the `90 x 26` buttons, `place(title, MARGIN, MARGIN, 300, 20)`, and the `DEFAULT_WIDTH/HEIGHT` of 1024x768). On a 150%/200% display the chrome therefore draws at 50-66% of its intended size while the page is right.

Before the manifest the same binary would have looked BLURRY but correctly sized, because Windows would have scaled the whole process. So the manifest did not introduce a bug so much as convert a hidden one into a visible one: it made the process claim a responsibility the window never implemented. The manifest is still right and should stay.

Worth noting for the record that the manifest's own comment block anticipated the DPI half correctly ("without it Windows bitmap-scales the whole process on a 150%/200% display, blurry chrome AND a blurry page") but nothing checked the converse.

### 2. A horizontal scrollbar appears with nothing to scroll

Reported by the human; not yet root-caused. `example.com` has no horizontal overflow in any browser, so this is werust-specific. There is no `WS_HSCROLL` anywhere in `crates/werust-windows` (the only scroll-ish style is `ES_AUTOHSCROLL` on the URL edit, which is unrelated), so the scrollbar is almost certainly INSIDE the WebView2 page, meaning the page's layout viewport is slightly wider than the visible area. Leading hypotheses, in order: the page container HWND is placed a few pixels wider than the host's client area; a physical-versus-logical pixel mismatch between the container's client rect and WebView2's `RasterizationScale` (which would be the same DPI bug as defect 1 wearing a different hat); or the controller's bounds being set from a rect that includes a border the client area does not.

### 3. A console window opens alongside the app

Confirmed by reading the source: there is no `#![windows_subsystem = "windows"]` anywhere in `crates/werust-windows`, so the binary links as a CONSOLE subsystem application and Windows allocates a console for it. `main.rs` also prints a startup banner with `println!`, which is what that console is currently showing.

## Why this matters beyond the three bugs

Two of these three (the DPI half-scaling and the console window) are properties of the BINARY and its manifest, not of any code path a test could exercise. The Windows CI leg builds, tests and runs the crates and even drives a real off-screen window — and it would report all three as green forever, because a headless runner has no DPI, no desktop and nobody watching. That is the concrete, non-hypothetical answer to the question this drive kept raising as "nobody owns the human-on-hardware sweep": the sweep found three real defects in its first five minutes.

A manual-verification pass per platform is therefore worth a task rather than a README section.
