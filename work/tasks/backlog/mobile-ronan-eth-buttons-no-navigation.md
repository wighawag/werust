---
title: "Diagnose + fix why blog/portfolio buttons do NOT navigate on mobile (ronan.eth SvelteKit client-side nav does nothing), using the in-app debug view"
slug: mobile-ronan-eth-buttons-no-navigation
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

FIELD FINDING (v0.2.4, human, MOBILE): on `ronan.eth` (a SvelteKit `@sveltejs/adapter-static` prerendered site), clicking the **blog** or **portfolio** buttons does NOTHING on Android — no navigation at all. The same buttons work on desktop (and desktop's blog-data failure was separately diagnosed + fixed in `diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture`, a query-string-leaks-into-the-DAG-path bug). Root-cause source: `work/notes/observations/field-test-v0.2.4-spa-clientrouting-eth-path-blank-links-2026-07-23.md` finding D's MOBILE half, explicitly parked there and in that task's DIAGNOSIS.md as "needs on-device diagnosis". **This task is that parked follow-on.**

**This is the FIRST task that gets to use the in-app debug menu landed in this drive.** On a phone with no tethered desktop, the human can now open the general menu -> Debug and see the CONSOLE log and NETWORK requests live in-app. The debug view is the natural diagnosis instrument for a "button does nothing" symptom: is SvelteKit's click handler even firing (a console error?), is the client router's `__data.json` fetch being attempted and failing (a Network entry?), or is the navigation itself being swallowed before any of that (no entries at all?). Diagnose with `~/.agents/skills/diagnosing-bugs/SKILL.md`, record the root cause with evidence, then fix. Do NOT guess-patch.

What we already know (so the diagnosis starts informed, not cold):

- **The mechanism is client-side.** The blog/portfolio buttons are SvelteKit client-side `pushState` navigations; NO WebView full page load and no `load-changed`/`onPageStarted` fires for them (this is the finding-A reality). Desktop tracked the URL via `notify::uri` / the SPA-nav handling; whether the Android edge routes an equivalent signal is a suspect.
- **The DESKTOP data bug is fixed, but is NOT obviously this symptom.** The fixed bug was `/blog/__data.json?x-sveltekit-invalidated=01` leaking the query string into the resolved DAG path, failing the fetch closed with a SvelteKit "500" and NO POSTS — but the page DID navigate. The mobile symptom is NO navigation AT ALL, which is earlier in the chain: the client nav never proceeds. Confirm whether the query-string fix (now on main) changes the mobile behaviour, but expect a distinct cause.
- **Suspects named in the original finding:** (a) the ANR-fix executor serialisation swallowing or blocking the client nav (capture/push work serialised on the same executor the WebView callbacks need); (b) the Android WebView not routing a SPA `pushState` nav into any signal werust listens to, so the "nav" is invisible and the app looks frozen; (c) a `shouldInterceptRequest` / `shouldOverrideUrlLoading` interaction where the client nav's data request is intercepted and the response never reaches the page.
- **ronan.eth is the fixture.** Source at `../ronan-eth/web` (`prerender = true`, `trailingSlash = 'always'`, `ssr = true`; build has `blog/index.html` + `blog/__data.json`, `portfolio/...`, `_app/...`).

Diagnose (with evidence): on a real Android device/emulator (or the strongest automatable harness werust has), load `ronan.eth` (or the committed minimal SvelteKit fixture from the sibling task), open the in-app debug view, click blog and portfolio, and capture: does a console error fire? does a Network entry for `__data.json` appear and with what outcome? does the Android WebView receive ANY signal for the click (an `onConsoleMessage`, a `shouldOverrideUrlLoading`, a `doUpdateVisitedHistory`)? From that evidence, name the root cause: is the client nav swallowed before it starts, or does it start and fail to complete?

Fix (only what the diagnosis proves): make a SvelteKit client-side navigation proceed AND complete on the Android WebView (and confirm iOS parity, whose client-nav routing is a different code path). If the cause is the ANR executor serialisation, the fix must NOT reintroduce the ANR (keep capture off the UI thread, per the store/capture tasks' guard).

## Acceptance criteria

- [ ] The mobile no-navigation is DIAGNOSED with on-device evidence recorded durably (`docs/spikes/<slug>/DIAGNOSIS.md`): what fires (or does not) on a blog/portfolio click, grounded in the in-app debug view's console + network output and/or the strongest automatable Android harness, and WHY desktop navigates while mobile does not.
- [ ] The root cause is FIXED so a SvelteKit client-side nav (blog/portfolio buttons) navigates AND completes on Android; the blog page renders its posts end to end (no silent no-op, no SvelteKit error boundary).
- [ ] iOS parity confirmed (the client-nav routing differs per platform; record whether iOS shares the cause or is already correct).
- [ ] The fix does NOT regress the ANR guard (capture stays off the UI thread; the executor serialisation is not loosened into an ANR).
- [ ] Whether the debug view was sufficient to diagnose it is RECORDED (it is the first real field test of the debug menu as an instrument); any gap in what it shows is a named observation.
- [ ] A regression guard where the seam allows (the mobile SPA-nav signal routing is a testable seam); manual device steps recorded. Network-isolated tests.

## Blocked by

- None. (Best landed after the in-app debug menu subsystem — store, menu, capture, both views — so it can be used as the diagnosis instrument; all now on main.)

## Prompt

> Goal: diagnose + fix why blog/portfolio buttons do NOTHING on Android on `ronan.eth` (a SvelteKit client-side `pushState` nav) while the same buttons work on desktop. This is the parked MOBILE half of field-test finding D. DIAGNOSE first with `~/.agents/skills/diagnosing-bugs/SKILL.md`, record the root cause with evidence, then fix; do NOT guess-patch.
>
> USE THE IN-APP DEBUG MENU as the instrument (it is the first real field test of it): on the device open the menu -> Debug and watch the Console + Network tabs while clicking the buttons. Does a console error fire? does a `__data.json` Network entry appear and with what outcome? or is there NOTHING — which would mean the client nav is swallowed before any signal reaches werust? The suspects are: the ANR-fix executor serialisation swallowing the client nav; the Android WebView not routing a SPA `pushState` into any signal werust listens to (no `shouldOverrideUrlLoading` / `doUpdateVisitedHistory` fires); or a `shouldInterceptRequest` interaction where the client nav's data request never reaches the page. The desktop query-string bug is fixed on main but produced a "500 + no posts" WITH navigation; the mobile symptom is NO navigation, earlier in the chain — expect a distinct cause but confirm.
>
> Where to look: `crates/werust-android/app/.../BrowserActivity.kt` (the WebViewClient callbacks, the ANR executor, `shouldOverrideUrlLoading`/`doUpdateVisitedHistory`), `crates/werust-android/rust/src/` (the backend's nav/signal routing), and the desktop SPA-nav handling for what signal desktop gets that mobile may not. ronan.eth source is `../ronan-eth/web`; the sibling task's committed minimal SvelteKit fixture is the network-isolated repro. Fix so a client-side nav proceeds AND completes on Android, confirm iOS parity, do NOT regress the ANR guard, record whether the debug view was sufficient to diagnose it.
