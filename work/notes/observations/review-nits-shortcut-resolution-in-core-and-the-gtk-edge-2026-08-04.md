---
title: review-gate non-blocking nits for 'shortcut-resolution-in-core-and-the-gtk-edge' (Gate 2 approve)
date: 2026-08-04
status: open
reviewOf: shortcut-resolution-in-core-and-the-gtk-edge
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'shortcut-resolution-in-core-and-the-gtk-edge' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: Escape is now claimed by the chrome in the CAPTURE phase and never reaches page content, so a web app can no longer use Escape (close its own modal, exit an overlay). Real browsers dispatch Escape to the page and stop the load only when the page does not handle it. Ratify as-is, or narrow the claim to Escape-while-loading?
  (crates/werust/src/main.rs key controller returns Propagation::Stop for any resolved action; the cost is recorded in docs/spikes/shortcut-resolution-in-core-and-the-gtk-edge/DECISIONS.md section 6, but only as a general statement about the claimed set.)
- Unrecorded in-scope decision: Escape performs Stop UNCONDITIONALLY, even with no load in flight, while the toolbar Stop button is insensitive unless is_loading and the shortcut history actions ARE gated on can_go_back / can_go_forward. Harmless in effect (renderer.stop on an idle page is a no-op) but it is the asymmetry that makes Escape swallowed even when there is nothing to stop. Intended?
  (perform_chrome_action, ChromeAction::Stop arm vs the GoBack/GoForward arms and Chrome::refresh setting stop.set_sensitive(state.is_loading()).)
- Letter chords are translated via keyval.to_unicode(), so Ctrl+L / Ctrl+R resolve only when the active layout produces the Latin letter; under a Cyrillic or Greek layout the chords silently stop working, where mainstream browsers still fire them. Worth recording as a known limit for the two sibling edge tasks, which inherit the same vocabulary.
  (fn shortcut_key in crates/werust/src/main.rs falls through to keyval.to_unicode().map(shortcuts::Key::Character).)
- Coherence: the system now has two vocabularies for a thing the chrome does: the new closed ChromeAction enum and the browser menu's item ids plus MenuItemKind::Action (werust_core::menu, MENU_ITEM_DEBUG). A later shortcut for an existing menu entry (e.g. the debug view) would have to bridge them. Worth pinning the relationship in the CONTEXT.md glossary entry so the next author does not fork it.
  (crates/werust-core/src/shortcuts.rs ChromeAction vs crates/werust-core/src/menu.rs MenuItemKind::Action; new glossary bullet in CONTEXT.md does not mention the menu vocabulary.)
