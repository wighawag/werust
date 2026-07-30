---
title: review-gate non-blocking nits for 'versioned-gtk-app-id-and-stale-process-detection' (Gate 2 approve)
date: 2026-07-30
status: open
reviewOf: versioned-gtk-app-id-and-stale-process-detection
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'versioned-gtk-app-id-and-stale-process-detection' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the widened sanitisation: the task prescribed replacing dots, but app_id() folds EVERY character outside [A-Za-z0-9-] to underscore, and prefixes the element with v. The agent recorded this in the spike README Decisions block with a sound rationale (a dev build version is git describe output, an operator can inject an arbitrary WERUST_VERSION, and an invalid id fails silently by dropping uniqueness). Confirm the human accepts the wider fold.
  (crates/werust/src/main.rs:81-92; docs/spikes/versioned-gtk-app-id-and-stale-process-detection/README.md section Decisions)
- The task assumed per-version cache dirs would accumulate under ~/.cache/com.github.wighawag.werust.v0_2_*. That premise looks wrong: nothing configures a WebKit data dir (backend.rs uses WebContext::new() with the default session), so WebKit/GTK storage is keyed on prgname (werust), not the application id. Good news is no per-version profile fork and no cookie loss on upgrade; the new consequence the task did not anticipate is that two DIFFERENT versions can now run CONCURRENTLY against the SAME cookie/localStorage/cache store, which was impossible before (the old process always took the session). Worth a human decision on whether concurrent shared-store access is acceptable or wants a follow-up.
  (crates/webview-renderer/src/backend.rs:58 WebContext::new(); no WebsiteDataManager / NetworkSession / data_directory anywhere in the tree)
- Nothing pins the production CALL SITE. The two new unit tests exercise the pure app_id() function, but no test asserts that main() passes werust_core::version() into it, so a future edit back to a constant id would silently restore the exact stale-process trap with a green suite. This repo already has the cheap mechanism for that (source-reading shape tests such as browser_menu_edge_wiring_shape.rs, which read crates/werust/src/main.rs). Suggest a one-line shape assertion as a named follow-up.
  (crates/werust/src/main.rs:663-666 vs crates/werust-core/tests/browser_menu_edge_wiring_shape.rs:57)
- The doc comment claims the id is made valid BY CONSTRUCTION, but only the character class is guaranteed; the 255-character application-id limit is not. Only an absurd injected WERUST_VERSION (over ~228 chars) reaches it, so impact is near zero, but the absolute wording overstates the guarantee.
  (crates/werust/src/main.rs doc comment on app_id, section Why the version is not spliced in verbatim)
- Acceptance criterion 2 (launching v0.2.9 while v0.2.8 runs opens a NEW window) is verified by proxy, not end to end: a headless Gio.Application probe shows new version -> primary, and two real binaries were built and confirmed to bake distinct versions via their banners, but the two-window launch was deliberately not run (opening windows on the operator desktop). Disclosed honestly in the spike README; residual risk is low since the probe isolates the one rule that decides the hand-off.
  (docs/spikes/versioned-gtk-app-id-and-stale-process-detection/README.md, sections The measurement and Confirming with the real binaries)
