---
title: review-gate non-blocking nits for 'enable-the-ios-back-forward-swipe-gesture' (Gate 2 approve)
date: 2026-08-04
status: open
reviewOf: enable-the-ios-back-forward-swipe-gesture
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'enable-the-ios-back-forward-swipe-gesture' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the trust-posture reset on a gesture move: entering an entry by swipe forces TrustPosture back to UnverifiedOrigin and clears the ENS/mutable-name axes, so swiping back onto a page WebKit restores from its page cache shows a weaker badge than the page earned (no scheme task re-runs to re-mark it). Recorded and argued fail-closed in DECISIONS.md decision 4 and pinned by a_gesture_history_move_never_carries_the_previous_documents_trust_posture; confirm the understatement is the wanted user-visible default.
  (crates/werust-ios/rust/src/backend.rs on_history_navigated (posture/ens_origin/mutable_name reset); docs/spikes/enable-the-ios-back-forward-swipe-gesture/DECISIONS.md section 4)
- Ratify the drift branch: a MAIN-FRAME .backForward target that matches neither adjacent entry is FOLLOWED by pushing, which truncates the forward history. With the new main-frame guard the hostile-iframe path is gone, but a benign mismatch (a url the core normalised differently from WebKit request.url) still silently destroys forward entries on an ordinary back swipe. Recorded in DECISIONS.md decision 2 with the real fix (mirror WKBackForwardList) named as out of scope.
  (backend.rs on_history_navigated, (None, None) arm; test a_gesture_navigation_the_core_history_does_not_know_is_followed_not_dropped)
- Ratify the new shared-core surface: BrowserShell::note_history_navigated is public on the toolkit-free core and today has exactly one caller (iOS CoreSession), with go_back/go_forward refactored onto a shared private enter_history_entry. The rejected alternatives (a LoadEvent::HistoryMoved variant, emitting a fake Started) are recorded in DECISIONS.md decision 6; confirm the one-method shape is the seam you want if a second backend ever moves its own history.
  (crates/werust-core/src/lib.rs note_history_navigated + enter_history_entry)
- Un-recorded divergence from the button path: note_history_navigated calls end_back_skip, so a swipe back does NOT skip redirect-source entries that BrowserShell::go_back skips (back_skip is armed only by go_back). On an ipfs site with _redirects, swiping back onto a redirect source can bounce the user forward again, where the toolbar Back would have stepped past it. The parity tests use redirect-free histories, so the parity claim has this one unexercised hole. Probably unfixable without fighting the gesture, but it is worth a line in DECISIONS.md decision 6 or an observation note.
  (lib.rs:2398 go_back arms back_skip; note_history_navigated calls end_back_skip; crates/werust-ios/rust/tests/back_forward_gesture_wiring_shape.rs parity fixtures)
- The main-frame guard is a strict equality on an optional (targetFrame?.isMainFrame == true), so any .backForward action WebKit delivers with a nil targetFrame is dropped rather than reported; the KVO url observer would then push a duplicate entry instead of moving the cursor. Safe direction, but no note or test states it. Worth one sentence beside the guard.
  (crates/werust-ios/App/Sources/WKWebViewShellController.swift:698-703)
- Process nit: no Decisions block was appended to the done record and the two feat commits carry no body, though this repo's convention (see work/tasks/done/shortcut-resolution-in-core-and-the-gtk-edge.md) and the task prompt both ask for decisions linked FROM the done record. The decisions themselves ARE durable and linked from crates/werust-ios/README.md, so this is bookkeeping only.
  (work/tasks/done/enable-the-ios-back-forward-swipe-gesture.md ends at the requeue section; git log -1 --format=%B c57883e is subject-only)
