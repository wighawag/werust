---
title: "Gate-3 conductor review: debug-console-network-capture-per-platform (APPROVE)"
date: 2026-07-28
status: open
reviewOf: debug-console-network-capture-per-platform
verdict: approve
---

## Verdict: APPROVE

Merged as `6386050`, after three Gate-2 blocks (all CORRECT) and two conductor-prescribed recoveries, plus a model switch that ended a resource stall. 311 `werust-core` tests re-run locally green.

## A task premise was FALSE, and it is worth naming plainly

The task body prescribed "wire the WebView's console-message signal (webkit6)" for desktop. That signal **does not exist** in WebKitGTK 6 (webkit6 0.4.0, the pinned binding — I checked the crate source myself). An agent hunting for a nonexistent signal is exactly what exhausted two dispatches' entire output budgets before any code was written. The correct mechanism was decided in the requeue and landed: desktop and iOS share ONE injected `werustDebug` script shim (the same document-start pattern iOS already uses for its EIP-1193 provider shim), while Android uses the genuinely native `WebChromeClient.onConsoleMessage`, which is strictly better than a shim. The per-platform difference is deliberate and recorded.

## Acceptance criteria, ticked against the merged tree

- [x] **Console captured on all three**, with level/message/source/line and one shared level vocabulary. Tests: `a_shim_console_envelope_maps_onto_a_console_entry`, `every_platform_console_level_spelling_maps_onto_the_one_vocabulary`, `the_console_shim_chains_to_the_original_and_wraps_every_level_once` (a shim that swallowed the real console output would be a worse bug than no debug menu, and it is guarded).
- [x] **Network captured per each platform's real reach.** Desktop via `connect_resource_load_started` + per-resource `finished`/`failed` reading `response()` for status/mime/length (this is what captures https too, which the scheme handler never sees). Android in the existing `shouldInterceptRequest` for both intercepted and passed-through requests. iOS via the reachable points plus a fetch/XHR wrapper in the shared shim, with its coverage limits recorded honestly.
- [x] **The honest per-request posture, and the indicator cannot be contradicted.** This is the criterion that took three rounds, and each was a real trust-honesty defect in the surface whose whole job is trust honesty: (1) a FAILED desktop resource was recorded TWICE and the second row stamped it content-verified (WebKit emits `failed` then `finished`, and the comment claiming otherwise was factually wrong) — now one honest row, `failed` only sets a flag; (2) iOS's main-frame compare ran against the DISPLAY identity, so it never fired on an ENS page — now both edges reuse the ONE shared main-frame predicate the 3xx task already built and tested against the WebKit authority-less URL form; (3) the mobile edges read a CACHED chrome snapshot (stale at capture time) where desktop read the live posture, so an ENS row could read too LOW as well as too high — now mobile reads the live load posture like desktop. Tests: `the_main_document_entry_can_carry_the_loads_own_two_axis_posture`, `a_capture_point_never_labels_a_request_verified_from_its_url_alone`, `the_main_frame_check_survives_the_webkit_authority_less_url_form`.
- [x] **The forward-pointer was honoured, including the ANR guard.** Capture pushes through a CLONED `DebugCapture` handle, NOT the whole session lock: `werust-android/rust/src/lib.rs:391` clones the store, and the doc at line 513 explicitly explains why it is not `self.with(|s| s.debug_capture())` — `resolve_ipfs` can hold the session lock for seconds on a worker thread and `onConsoleMessage` is on the UI thread, which is precisely the ANR shape user story 4 forbids. **The ANR fix is NOT regressed.** Entries are built through the constructors, so `MAX_TEXT_CHARS` truncation cannot be bypassed: `a_capture_point_entry_is_bounded_because_it_goes_through_the_constructors`.
- [x] **Capture is READ-ONLY.** Tests pin that capture does not disturb the chrome state or the posture: `the_debug_capture_does_not_disturb_the_chrome_state`.
- [x] **Bounded, always-on, parity-tracked**, with a new `debug-capture-console-and-network` capability row.

## Nit triage (4 non-blocking findings)

All four are ratifications for the human, none a defect. Decision 7 (Android passed-through https requests recorded with status/mime/size honestly NULL) means the Network tab will render rows with an *unknown* status for every non-intercepted request: that is the honest answer, and the view tasks must render it as such rather than implying a zero. Decision 9 marks the capture row implemented on all three even though nothing is user-visible yet (the views are the next tasks) — correct, just over-readable in isolation. Two are surfaces the debug-view tasks will now depend on: the new one-way `werustDebug` page-reachable channel, and the fact that both shims inject into ALL frames (iframes included), so iframe console entries are expected. Both are reasonable; DECISIONS.md should just say the iframe part out loud.

## Process note — why this task took five dispatches, and what fixed it

This task had the widest blast radius (six integration points) and hit two distinct stall modes. The first was the by-now-familiar 16384/16384 reasoning-exhaustion: two full dispatches wrote zero code. I addressed it two ways — prescribing the decided mechanism with verified API signatures (which removed the need to deliberate), and an incremental commit-per-slice instruction so the kept branch accumulated progress across dispatches. The second was a genuine Gate-2 trust-honesty block, recovered twice by requeue. The decisive change, though, was the **user-directed switch of the build + review models to `openrouter/moonshotai/kimi-k3`**: on that model the pending posture fix was committed within minutes and the run flowed straight through Gate-1 and Gate-2 with no reasoning exhaustion. That is the clearest signal yet that the stall was model behaviour, not the task.
