---
title: review-gate non-blocking nits for 'reload-stop-collapse-and-spinner-on-the-windows-chrome' (Gate 2 approve)
date: 2026-08-04
status: open
reviewOf: reload-stop-collapse-and-spinner-on-the-windows-chrome
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'reload-stop-collapse-and-spinner-on-the-windows-chrome' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The spike README states what the windows-latest leg PROVES (five new properties on a real window), but the Win32 half is cfg(windows) so the Ubuntu gate compiles none of it, and nothing in the diff says whether that leg was actually run for this change. The sibling spike README for the original Win32 task is explicit that it was written blind on Linux and then run on a runner; this one is not. Can the author state plainly what has been observed versus what is expected?
  (docs/spikes/reload-stop-collapse-and-spinner-on-the-windows-chrome/README.md, section What CI proves; crates/werust-windows/src/{chrome,window}.rs are behind cfg(windows))
- The task's forward-pointer said the windows-renderer leg is already red on two mouse-back checks, and that if the sequencing is left untouched the done record must say so. The sequencing IS correctly left untouched and un-weakened, but no file in the diff mentions the known-red baseline, so a reader of the new README will take a red leg as a regression from this change. Add one sentence naming the two expected baseline failures.
  (crates/werust-windows/examples/window_smoke.rs mouse section unchanged; work/notes/observations/windows-smoke-mouse-back-check-is-sequenced-after-a-failed-load-2026-08-04.md not referenced anywhere in the diff)
- watch_a_load_and_cancel_it only samples the in-flight state AFTER a full pump (pump_messages + 20ms sleep + tick). With the canned in-memory ipfs fixture the load can settle inside that first pump, in which case is_loading is never observed, four new checks fail (in flight, STOP mode, spinner, cancel) and a flake is indistinguishable from a real regression. Should the first sample be taken before the first pump, or the watch keyed off the shell's own loading transition?
  (crates/werust-windows/examples/window_smoke.rs, the 0..(10*50) loop; the existing smoke never asserted an in-flight state before, so this sampling window is new ground)
- RATIFY: with no Win32 spinner control, the spinner is a one-glyph STATIC cycling four Geometric-Shapes frames at 20fps off the existing pump tick, in a permanently reserved 20px (96 DPI) slot between the collapsed control and the URL bar, centred via a new SS_CENTER constant. All user-visible defaults, none specified by the task.
  (DECISIONS.md sections 1-3; SPINNER_FRAMES in chrome.rs, SPINNER_WIDTH in dpi.rs, Chrome::spin called from Controller::tick)
- RATIFY: a click builds a FULL ChromePaint snapshot (ChromePaint::of) just to read one enum, and only this control was converted to the shared ChromeAction performer; back, forward and URL-enter still call the shell directly, so the edge now carries two dispatch idioms. Deliberate per DECISIONS 4, but worth a human nod.
  (window.rs Controller::reload_stop_action and the ID_RELOAD_STOP arm of handle_command; DECISIONS.md section 4)
- RATIFY cross-task: ChromePaint::is_loading is now unused by this edge but deliberately LEFT on the shared desktop-paint carrier because the not-yet-landed macOS collapse still reads it. That leaves the removal question unowned until reload-stop-collapse-and-spinner-on-the-macos-window lands.
  (DECISIONS.md section 5; the shape guard instead asserts the Windows apply never mentions paint.is_loading)
