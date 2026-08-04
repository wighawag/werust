---
title: review-gate non-blocking nits for 'shortcuts-and-mouse-history-buttons-on-the-macos-edge' (Gate 2 approve)
date: 2026-08-04
status: open
reviewOf: shortcuts-and-mouse-history-buttons-on-the-macos-edge
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'shortcuts-and-mouse-history-buttons-on-the-macos-edge' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- RATIFY: the diff adds a new user-visible default to the SHARED table, Cmd+left-bracket / Cmd+right-bracket as unconditional history back/forward, for PrimaryModifier::Meta only. It is a new row every edge consumes and it is claimed unconditionally in sendEvent:, so a Mac page can never see those two chords (same trade already recorded for Cmd+L / Escape). It follows Safari/Chrome/Firefox convention, is Meta-gated by a third platform fact beside the existing two, and the Ctrl platforms are provably untouched (Windows shortcut_key maps VK_A..=VK_Z only; GTK is Control so bracket_history is false). The conductor recommended it in the requeue, so this is ratification rather than a surprise.
  (crates/werust-core/src/shortcuts.rs history_is_also_spelled_with_brackets + the bracket rows; tests the_mac_spells_history_a_second_way_that_no_text_field_ever_claims and input.rs the_bracket_history_chords_reach_history_from_either_focus; DECISIONS.md decision 9)
- RATIFY (cross-task): this task repaired ANOTHER spike's shared tooling, docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh, adding six missing stand-in core symbols plus a whole stand-in shortcuts module whose resolve_chord/resolve_pointer_button return None. That is fine as a repair (the harness failed to compile on main), but it grows a second, hand-maintained copy of the core's shape that nothing on the gate compiles, so it can rot again. The rot itself is captured as an observation note rather than fixed.
  (docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh (+138); work/notes/observations/macos-typecheck-harness-stand-in-core-drifts-silently-2026-08-04.md)
- Doc-comment typo left by the amendment: the sentence ends with a literal //! glued to the previous line instead of a line break, so the rendered module doc reads '...flip its own cell the same way.//! AMENDMENT...'. One-character fix.
  (crates/werust-core/tests/shortcut_edge_wiring_shape.rs:48)
- Stale count in the record the one human Mac tester will read: the sentence still says the shortcut layer adds three items to the awaits-a-human list, but the list now has six bullets (two added this round).
  (docs/spikes/shortcuts-and-mouse-history-buttons-on-the-macos-edge/README.md:57)
