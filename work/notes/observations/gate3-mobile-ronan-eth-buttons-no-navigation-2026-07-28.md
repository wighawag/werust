---
title: "Gate-3 conductor review: mobile-ronan-eth-buttons-no-navigation (APPROVE)"
date: 2026-07-28
status: open
reviewOf: mobile-ronan-eth-buttons-no-navigation
verdict: approve
---

## Verdict: APPROVE

Conductor-integrated (dorfl's recovery path skipped Gate-2, so the review AND the merge were done by the conductor per the human's instruction). Squash-merged onto `origin/main`; the work branch and its stale lock are cleaned up. Post-merge tree re-run green locally: `werust-core` suites pass and `werust-android-core` reports 60 passing including every `origin_map` and internal-origin test.

## This is the strongest diagnose-then-fix of the drive, and it is the human-directed investigation's payoff

The task was the parked mobile half of field-test finding D. What landed is a rigorous diagnosis with on-device evidence, not a guess-patch:

- **Root cause, proven on-device with verbatim logcat.** An `ipfs://` document served through `WebViewClient.shouldInterceptRequest` gets an OPAQUE origin in the Android System WebView (the origin serialises as the bare `ipfs://` with no host; Android's interception hook is interception, not scheme registration). On that origin Blink rejects `fetch('ipfs://…/blog/__data.json…')` ("URL scheme ipfs is not supported") BEFORE the network stack, and `history.pushState` throws `SecurityError` — so a SvelteKit client-side nav dies inside the renderer before ANY signal reaches werust. That is exactly why the button "does nothing": no request, no navigation event, no history update. The BEFORE probe log shows a console error and nothing else; the AFTER log shows `fetch: ok:200`, `pushState: ok`, and a `doUpdateVisitedHistory`.
- **Why desktop navigates while Android did not:** desktop's WebKitGTK registers `ipfs` as a FIRST-CLASS scheme (`webkit_web_context_register_uri_scheme`), giving the document a real tuple origin. Android has no scheme-registration API. Distinct from, and EARLIER in the chain than, the desktop query-string bug, exactly as the task predicted.
- **The fix is the recorded fallback promoted to the mechanism.** The WebView loads `https://<cid>.ipfs.werust.invalid` (a normal fetchable/`pushState`-able secure context, one origin PER SITE, `.invalid` is RFC 2606 so it can never collide with a real site), and `crates/werust-android/rust/src/origin_map.rs` maps every URL between the internal origin and the core's real `ipfs://` URLs, so history, the URL bar, trust, the `_redirects` main-frame inference, and the debug view all keep speaking `ipfs://`.

## The coherence point I most wanted to verify, and it holds

The internal `https://` origin must NOT leak into the core, or the trust indicator and ENS identity would be quietly corrupted. `origin_map.rs` is explicit: it maps the pending load OUT to the internal origin and every reported/intercepted URL BACK to `ipfs://`, "the internal origin never leaks into the core". The debug Network tab renders REAL `ipfs://` URLs with honest `content-verified` status, and the trust indicator shows ✓ verified on the home page (screenshot `debug-view-network-tab-speaks-ipfs-urls.png` / `home-page-renders-verified-after-fix.png`). Unit tests pin the round-trip, the CIDv0→base32 normalisation (Chromium lowercases hostnames, so a mixed-case CIDv0 could not round-trip), the lowercase-host case, and a fail-soft for unparseable CIDs. **No trust re-meaning; the core never sees the internal origin.**

## Acceptance criteria, ticked against the merged tree

- [x] **Diagnosed with on-device evidence** in `DIAGNOSIS.md` (the exact fetch/pushState/history signals, why desktop navigates while mobile did not).
- [x] **Fixed**: client-side nav proceeds AND completes on Android; the blog page renders its posts end to end (screenshots committed).
- [x] **iOS parity analysed**: iOS does NOT share the cause (a `WKURLSchemeHandler`-served document gets a real tuple origin; the Capacitor/Ionic serving model). Runtime confirmation left as recorded steps for a Mac, honestly flagged as analysis not a device run.
- [x] **ANR guard NOT regressed**: no threading changes; the sync-session serialisation test still passes.
- [x] **Debug-view sufficiency recorded as YES** with one named gap (favicon noise buries the interesting entry — captured as its own observation).
- [x] **Regression guards**: an on-device `SpaClientNavOriginTest` (red-capable probe) plus Rust-seam unit tests; manual device steps recorded.

## Valuable off-path observations the agent captured (worth the human's eye)

Four, all real: the debug Network tab buried by per-nav favicon noise (the one gap in the debug view as an instrument); the mobile MIME table lacks common web types; the retrieval-backend setting cannot take effect on mobile at all (a pre-existing gap that forced a scratch gateway for the run); and a favicon double-injection console error. These are the kind of finding the drive is meant to surface.

## Process note

This was the most expensive task of the drive by dispatches (~9), for two compounding reasons. The on-device investigation (emulator + network proxy + screenshots + a committed `connectedDebugAndroidTest` probe) genuinely could not fit inside dorfl's 25-minute internal deadline, so it hit the checkpoint ceiling repeatedly and lost its on-device harness each time; and once the work was done, the agent kept RE-VERIFYING on-device instead of committing, until a "commit it first" directive landed it. Then the acceptance gate's linker died on a FULL 16G /tmp tmpfs (accumulated scratch + Gate-3 target dirs), which the conductor freed. The final integration was manual because dorfl's recovery path skipped Gate-2. None of this was a code defect; all of it is process friction worth noting for future on-device tasks.
