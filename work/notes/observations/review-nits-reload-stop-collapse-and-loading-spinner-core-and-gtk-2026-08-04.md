---
title: review-gate non-blocking nits for 'reload-stop-collapse-and-loading-spinner-core-and-gtk' (Gate 2 approve)
date: 2026-08-04
status: open
reviewOf: reload-stop-collapse-and-loading-spinner-core-and-gtk
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'reload-stop-collapse-and-loading-spinner-core-and-gtk' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the spinner rule: the task said the spinner is a second presentation of the same is_loading fact, but load_spinner_visible is load_progress_visible (is_loading OR load_step != Idle). Consequence: during the pre-content name-resolution window the spinner turns while the one control still offers Reload, so there is no cancel affordance in that window (unchanged from the old insensitive Stop button). Reasonable and argued, but it is the agent's call, and all four sibling edge tasks inherit it.
  (crates/werust-core/src/lib.rs load_spinner_visible = load_progress_visible; recorded in docs/spikes/reload-stop-collapse-and-loading-spinner-core-and-gtk/DECISIONS.md section 1)
- Ratify routing the toolbar control's click through perform_chrome_action via ReloadStopControl::action(), which extends shortcuts::ChromeAction from what an INPUT means to what a toolbar control DOES. Back/forward/URL-entry deliberately still call BrowserShell directly, so the edge now has two idioms for toolbar handlers.
  (crates/werust/src/main.rs reload_stop.connect_clicked; DECISIONS.md section 2)
- Ratify the user-visible layout default the four sibling edge tasks are told to follow: spinner immediately after the collapsed control and before the URL bar, in a permanently allocated slot driven by opacity rather than visibility.
  (crates/werust/src/main.rs toolbar.append order and set_opacity; DECISIONS.md section 5)
- The done record was moved with zero content change, so it links neither DECISIONS.md, nor the spike README, nor the new observation note. The sibling done record for shortcut-resolution-in-core-and-the-gtk-edge carries exactly such a paragraph, and CLAIM-PROTOCOL asks for the durable record to be linked from the done record for discoverability. Worth adding a short Decisions/Notes section.
  (work/tasks/done/reload-stop-collapse-and-loading-spinner-core-and-gtk.md (rename only) vs work/tasks/done/shortcut-resolution-in-core-and-the-gtk-edge.md lines 76-78)
- The new guard's module doc says both values are pure functions of the same ChromeState::is_loading fact, which is true for the control but not for the spinner (it is load_progress_visible). Since the four sibling edge tasks will read this header as their brief, the one wider rule should be stated there.
  (crates/werust-core/tests/collapsed_reload_stop_control_shape.rs header vs its own test the_spinner_never_changes_what_the_url_bar_progress_bar_does)
- The captured observation notes that the fan-in task register-the-new-chrome-fields-in-the-mobile-presentation-guard only names FACT_FIELDS / DERIVED_FIELDS, not every_derived_string's hand-picked rule list, so the new label/description strings could stay hardcodeable on mobile even after the contract step. The fan-in task was not amended; someone should fold that into its acceptance.
  (work/notes/observations/mobile-guard-forbidden-literals-are-a-hand-picked-rule-list-2026-08-04.md; work/tasks/ready/register-the-new-chrome-fields-in-the-mobile-presentation-guard.md)
