---
title: review-gate non-blocking nits for 'ipns-tofu-pin-and-warn-on-change' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: ipns-tofu-pin-and-warn-on-change
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'ipns-tofu-pin-and-warn-on-change' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the eight recorded decisions in docs/spikes/ipns-tofu-pin-and-warn-on-change/DECISIONS.md. The one genuinely worth a human second opinion is decision 3: error_banner_visible / error_banner_text now mean a failure-CLASS state (a failed load OR a changed trusted name), so the rule name error_banner_* is wider than what it covers, and the prominent-load-failure capability row has a second occupant. The alternative (a second banner surface on four edges) was rejected for good reason and the human answer settled the prominence, so this reads correct, but the vocabulary widening is load-bearing across every edge.
  (crates/werust-core/src/lib.rs error_banner_visible/_text; docs/platform-capability-matrix.toml prominent-load-failure row; DECISIONS.md section 3)
- An in-scope, user-visible decision that is NOT in the Decisions block: the GTK trust badge was a plain Label with a hover tooltip and this diff turns it into a MenuButton opening a Popover (a new desktop trust SURFACE, not just one more line in an existing one). Worse, DECISIONS.md section 8 and the new backlog task both assert the opposite (that GTK already had a popover behind the badge and only had to add a line and a button), and the matrix trust-explanation row still describes desktop as tooltip-only. The macOS follow-on premise still holds, but the history it cites is wrong. Ratify the change and correct the two claims?
  (crates/werust/src/main.rs open_window (MenuButton + Popover + trust_surface, new); HEAD^ had Popover only for the menu button; docs/platform-capability-matrix.toml line ~357)
- The pin store is loaded ONCE per shell and saved by rewriting the whole file, so two shells (a second werust launch activates the same GTK app and opens a second window in-process, and two different versions are two processes) each hold a stale snapshot: blessing in window A then in window B silently DROPS A's pin, which is exactly the miss the module says a TOFU store cannot have. RetrievalSettings avoids this by doing load_from -> mutate -> save_to per action. Should bless_current_name re-read and merge before saving?
  (crates/werust-core/src/lib.rs:1748 pins loaded in the constructor, bless_current_name saves self.pins wholesale; crates/werust-core/src/retrieval.rs apply_settings_request_in does load-modify-write)
- Test hermeticity: BrowserShell::new calls TrustedNamePins::load(), which resolves the REAL settings dir, so every core test that does not use with_pins_dir reads the developer's real pins.json. A dev who has blessed ronan.eth in their own build would flip the TOFU axis inside fixtures that use the same name and could red unrelated chrome assertions. Nothing WRITES the real store today (the only bless without with_pins_dir returns early at the visibility gate), and no test asserts the real file is untouched. Worth defaulting test shells to an empty store?
  (crates/werust-core/src/lib.rs:1748; tests at 4099/4122 use with_pins_dir, the nothing-to-bless test at ~4180 does not; pins.rs isolation test asserts only the scratch dir contents)
- The changed-name banner is not gated on is_loading, while the badge (loading wins) and trust_pin_action_visible are. A reload of a changed site therefore shows the loading badge (which by its own doc asserts nothing) above a failure-class banner asserting the change, and the two failure-class cases differ in flight because navigate clears last_error but re-derives the mutable-name axis. Deliberate (the warning should not flicker) or an oversight?
  (crates/werust-core/src/lib.rs error_banner_visible vs trust_indicator/trust_pin_action_visible; navigate clears chrome.last_error, refresh_chrome re-derives chrome.mutable_name)
