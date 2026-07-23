---
title: "Field test v0.2.3 (desktop + mobile): back-button still leaks ipfs://, Android ANR modal, url bar not tracking in-page nav, no-protocol entry resets bar"
date: 2026-07-23
status: open
kind: field-finding
release: v0.2.3
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
---

## Context

Human field test of the v0.2.3 build (the 5 v0.2.2 field-fix tasks) on BOTH desktop and mobile. Desktop is smoother; the timeout/scope fixes helped. `jolly-roger.eth` and `ronan.eth` now work on mobile. Several NEW issues surfaced, plus one that contradicts a Gate-3 tick. Captured here as the v0.2.3 field finding; the actionable ones are drafted as tasks (this observation is the shared root the tasks link back to). None hand-fixed.

## Findings

### A. (REGRESSION vs Gate-3) Back-button still shows `ipfs://` (in fact `ipfs:///`) when the previous page was an ENS name — DESKTOP

Reported: "the back button still make the url show ipfs:/// if the previous page were ens name." This CONTRADICTS the `preserve-ens-name-in-bar-on-reload-and-history` Gate-3 tick "back/forward onto an ENS page shows the name + posture" — that tick was made against the FakeBackend tests, which the real backend does not match.

Root cause (confirmed in code): `BrowserShell::go_back` sets `url_override = None` and relies on `refresh_chrome` to RE-DERIVE the `.eth` name from `ens_pages`, keyed on `self.renderer.current_url()`. But:
- On the REAL `WebViewRenderer` (`crates/webview-renderer/src/backend.rs`), `current_url()` reads the shared `LoadLifecycle`, which WebKitGTK updates ASYNCHRONOUSLY via `load-changed` signals on the GTK main loop. Immediately after `go_back()`, `current_url()` still reports the PREVIOUS entry (not yet the ENS page's CID), so the `ens_pages` lookup MISSES and the bar follows the backend URL. The FakeBackend updates `current_url` synchronously on `go_back` and returns the identical stored string, so the tests always match and never exercise this.
- The displayed `ipfs:///` (triple slash) strongly implies a URL-NORMALIZATION mismatch too: the key stored in `ens_pages` (from `current_url()` at forward-load time) differs from the string WebKitGTK reports for the same entry after a back (e.g. an authority-less `ipfs://<cid>` normalized to `ipfs:///<cid>`, or a trailing-slash difference), so even the later async re-derivation can miss.

Fix direction (a task, NOT hand-fixed): make the ENS-history re-derivation robust on the REAL backend: (a) re-derive on the async pump once `current_url` settles (not only synchronously in `go_back`), AND (b) key `ens_pages` on a NORMALIZED CID (canonicalize the `ipfs://` URL both when storing and when looking up) so the forward-store key and the post-back key match. Add a backend-level / integration test (or a FakeBackend that models async history + the real normalization) so the fake no longer hides this. See task `ens-history-name-rederive-async-and-normalized`.

### B. Android "isn't responding" (ANR) modal, RECURRING — MOBILE (Android)

Reported: "I got the 'isn't responding android modal' regularly, and interesting pressing 'wait' I could still type in the url bar, do anything there, but the modal would keep popping up." The UI STAYS usable (URL bar typeable) yet Android's ANR watchdog keeps firing — classic signature of the MAIN (UI) thread being blocked/starved on a loop or repeated long main-thread work, even though the WebView surface itself repaints.

Context: `ipfs-retrieval-off-main-thread-no-ui-freeze` (done) moved the ipfs:// RETRIEVAL off the UI thread on the webview side, but the ANR is on Android specifically and recurs, so something is still hammering/blocking Android's main thread. Prime suspects to diagnose (NOT yet root-caused): a too-tight `pump()`/refresh loop or main-thread JNI/FFI call cadence in `crates/werust-android` (`BrowserActivity` / `WerustCore.kt` / the FFI bridge), main-thread work in the Android ipfs scheme interception, or a busy re-render triggered by the new v0.2.3 `LoadStep`/chrome-JSON polling. Needs a diagnosis pass (`diagnosing-bugs`) to find WHAT blocks the Android main thread, then a fix task. This is the biggest mobile issue. See task `android-anr-main-thread-diagnose-and-unblock`.

### C. URL bar does not update on IN-PAGE navigation — ALL PLATFORMS (noticed on the ENS page)

Reported: "when navigating in the page, the url bar do not update to show the new path, it shows when going back but only after navigating to a new site first and then back." Root cause (confirmed): for an ENS page the front door sets `url_override = Some(name)` and it PERSISTS ("the override PERSISTS across pumps so the name stays put for the whole load"). In `pump()`, the lifecycle events only write `chrome.url_text` when `!pinned` (i.e. when there is NO `url_override`). So while an ENS name is pinned, ANY subsequent in-page navigation (clicking a link that changes the backend URL) is suppressed from the bar — the name stays but the PATH never appears. The "shows on back only after visiting a new site first" symptom is the same override stickiness resolving only once a non-ENS entry clears the pin.

Fix direction (a task): distinguish "pin the ENS NAME for the resolved-root load" from "follow the backend URL as the user navigates WITHIN/AWAY from that page." Options to weigh: show `name` + the in-page path suffix (the honest identity + where you are), or drop the pin once the user navigates off the resolved root entry (a link click is a new load, not the front-door entry), while keeping the name for the root. Must stay coherent with finding A (the ens_pages re-derive) and with the reload re-resolve decision. See task `urlbar-tracks-in-page-navigation-not-just-pinned-name`.

### D. A scheme-less entry like `github.com` is rejected and RESETS the bar instead of trying `https://` — ALL PLATFORMS

Reported: "not putting protocol on for example github.com it does not tries to do https://github.com and reset the url bar to where it was. it should attempt, but if we give something that is invalid, it should show an error and not reset the url bar." Root cause (confirmed): `eth_name_from_entry` only recognises a bare `.eth` name; anything else scheme-less (`github.com`) falls through to `renderer.navigate("github.com")`, whose `validate_url` (`crates/webview-renderer/src/lib.rs`, and the iOS/Android twins) REQUIRES a `scheme://`, so it returns `Err(InvalidUrl)` BEFORE touching the chrome. The shell's `navigate` returns that `Err` early (no `url_override` update, no `last_error` set), so the bar is left showing the prior page and no error surfaces.

Fix direction (a task): a scheme-less, non-`.eth` entry that looks like a host/URL should be RETRIED as `https://<entry>` (the browser-idiomatic default), so `github.com` loads `https://github.com`. A genuinely invalid entry (that is neither a bare `.eth` name nor a valid host/URL) must surface an ERROR in the chrome and KEEP the typed text in the bar for the user to fix (mirroring how a failed ENS load keeps the name + shows the reason), NEVER silently reset the bar to the previous page. Applies on all three platforms (the three `validate_url` twins + the shell's `navigate`). See task `scheme-less-entry-https-fallback-and-keep-bar-on-error`.

### E. `mandalas.eth` still renders a BLACK page — MOBILE

Reported: "mandalas.eth is still black but jolly-roger.eth and ronan.eth works fine." A SPECIFIC ENS site is black on mobile while others render, so it is content/renderer-specific (not the whole ipfs:// path — that path works for the other two names now). This is adjacent to the PARKED `ipfs-site-mobile-black-page` (which awaited a human Android re-test): the re-test result is IN — the generic ipfs:// path renders on Android for jolly-roger/ronan, but mandalas.eth specifically is black. Route this detail into the parked task (narrow it from "ipfs site black on mobile" to "mandalas.eth specifically black on mobile while other ENS/ipfs sites render") rather than opening a duplicate; it still needs the human's device to diagnose what mandalas.eth uses (heavy CSS/JS? a resource the scoped fetch now misses? a color-scheme/background interaction?).

## What works (v0.2.3 confirmed good)

- Desktop overall smoother; the scope + timeout fixes helped.
- `jolly-roger.eth` and `ronan.eth` load fine on mobile (the whole-DAG + timeout fixes landed the reliability win the field test was chasing).
