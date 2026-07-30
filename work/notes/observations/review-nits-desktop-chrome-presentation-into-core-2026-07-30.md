---
title: review-gate non-blocking nits for 'desktop-chrome-presentation-into-core' (Gate 2 approve)
date: 2026-07-30
status: open
reviewOf: desktop-chrome-presentation-into-core
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'desktop-chrome-presentation-into-core' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the API shape chosen for the moved derivation: the 12 rules landed as crate-root `pub fn` in werust-core (plus a new `#[must_use]` on each) rather than a `chrome` submodule or methods on ChromeState. This is the public surface the three downstream tasks (windows-webview2-backend-and-window, macos-wkwebview-backend-and-window, mobile-chrome-presentation-from-one-derivation) will consume, and no Decisions block records the choice (bare commit body, no spike dir).
  (crates/werust-core/src/lib.rs:601-870 (pub fn status_line / trust_indicator* / error_banner_* / invalid_entry_badge_* / load_progress_*); git log -1 has no Decisions section)
- The exhaustive CSS-class toggle lists still live hard-coded in the GTK painter while the class NAMES are now produced in another crate, with no test tying the two together. Adding a fifth posture in core would silently leave every painter with a stale class. Worth exporting the class set (or an enum) from core when the second window lands.
  (crates/werust/src/main.rs:1039-1068 hard-codes trust-loading/trust-verified/trust-name-trusted-rpc/trust-mutable-name/trust-unverified and error-banner/error-banner-transient; core decides them at lib.rs:172,264)
- The debug-view row presentation helpers stayed private in the GTK edge, but macos-wkwebview-backend-and-window's acceptance says the debug view paints from THE shared derivation this task produced. That second extraction is currently unowned; name it (a note or a clause on the macOS task) so the AppKit/Win32 builder does not quietly re-derive them.
  (crates/werust/src/main.rs:524-609 (console_level_css_class, console_source_line, console_row_text, network_status_text/mime_text/size_text/trust_label/trust_css_class); work/tasks/backlog/macos-wkwebview-backend-and-window.md acceptance clause naming the debug view)
- Should CONTEXT.md pin the presentation-in-the-core / edge-is-a-PAINTER pair now that it is load-bearing across ADR-0011, three backlog tasks and the code? The glossary defines seam but not painter, so the next author could re-fork the term.
  (CONTEXT.md has no chrome/presentation/painter entry; the words are used normatively in crates/werust-core/src/lib.rs:601-622 and crates/werust/src/main.rs:17-23)

## Conductor triage (2026-07-30, via drive-tasks)

- **The unowned debug-view extraction: FIXED.** `macos-wkwebview-backend-and-window` now OWNS extracting the debug-view row helpers (`console_level_css_class`, `console_source_line`, `console_row_text`, `network_status_text`/`_mime_text`/`_size_text`/`_trust_label`/`_trust_css_class`) into `werust-core` behaviour-preservingly before painting its debug view, and `windows-webview2-backend-and-window` says to consume them if that landed first, or extract them the same way if not. Neither shell may re-derive them.
- Awaiting the human: ratifying the crate-root `pub fn` API shape for the 12 moved rules (cheaper to change now than after two more edges consume it), and whether `CONTEXT.md` should pin the "presentation in the core / an edge is a PAINTER" vocabulary.
- Still open, unowned: the CSS-class toggle lists are hard-coded in the GTK painter while the class NAMES are now decided in core, with no test tying them together, so a fifth trust posture would silently leave painters stale. Fix by exporting the class set from core; it becomes urgent when the second painter exists.
