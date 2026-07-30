---
title: "Gate-3 conductor review: desktop-chrome-presentation-into-core (APPROVE)"
date: 2026-07-30
status: open
reviewOf: desktop-chrome-presentation-into-core
verdict: approve
---

## Verdict: APPROVE

Merged as `e8342b8` on `origin/main` (drive-tasks, `--allow-backlog --review --merge`, `etherplay/opus-5`). Gate-1 and Gate-2 green, 4 non-blocking nits. Full gate re-run locally after the merge: `cargo fmt --check` clean, clippy 0 errors, all tests pass.

786 lines left `crates/werust/src/main.rs`; 792 arrived in `crates/werust-core/src/lib.rs`. This is the enabler ADR-0011 asked for, and it is the reason the next two windows paint instead of re-deriving.

## Acceptance criteria, ticked against the merged tree

- [x] **All 12 presentation rules live in `werust-core` and the GTK file calls them.** `pub fn status_line`, `trust_indicator` / `_detail` / `_css_class`, `error_banner_visible` / `_text` / `_css_class`, `invalid_entry_badge_visible` / `_text`, `load_progress_visible` / `_fraction` / `_hint`, imported by `main.rs` from `werust_core`.
- [x] **Toolkit-free.** Zero `gtk4` / `webkit6` references in `werust-core/src/lib.rs`, and no toolkit dependency in its `Cargo.toml`. The GTK file keeps the stylesheet and the widget calls, which is correct: the class NAME is a derivation, the stylesheet is painting.
- [x] **Behaviour preserved, and I checked it properly rather than trusting the label.** I diffed four moved test bodies (`load_progress_is_a_url_bar_fraction_that_never_displaces_the_page`, `trust_indicator_shows_a_neutral_loading_state_that_hides_the_posture_while_loading`, `status_line_names_the_live_pipeline_step_while_loading`, `a_transient_timeout_banner_is_distinct_and_retryable_while_a_hard_fail_keeps_its_reason`) between the pre-move file and the core: byte-identical apart from two now-unnecessary `use` lines. Not one assertion was weakened to make the move fit.
- [x] **No new dependency, no new seam, `ChromeState`'s public surface unchanged.** The only additions are the moved functions (each now `#[must_use]`).
- [x] **The mobile follow-up is named.** `mobile-chrome-presentation-from-one-derivation` in the backlog, carrying the FFI-versus-extended-chrome-JSON fork as its stated decision.
- [x] **`docs/platform-capability-matrix.toml` was kept truthful.** Five capability comments that pointed at `crates/werust/src/main.rs` for these rules now say "in werust-core (painted by crates/werust/src/main.rs)". Unasked-for and exactly right: those comments are how the guard explains itself.

## Nit triage (4 non-blocking findings)

**Acted on by me (conductor): the debug-view helpers are a SECOND, unowned extraction.** The debug-view row presentation (`console_level_css_class`, `console_source_line`, `console_row_text`, `network_status_text` / `_mime_text` / `_size_text` / `_trust_label` / `_trust_css_class`) stayed private in the GTK edge, but `macos-wkwebview-backend-and-window`'s acceptance says the debug view paints from the shared derivation. That gap was mine to fix, since I wrote the task; both the macOS and Windows shell tasks now name it explicitly, so the AppKit/Win32 builder cannot quietly re-derive them.

**For the human: ratify the API shape.** The 12 rules landed as crate-root `pub fn` in `werust-core`, not as a `chrome` submodule and not as methods on `ChromeState`. That is now the public surface three downstream tasks consume, and no DECISIONS block records the choice. My view: crate-root functions are fine and match `status_line`'s existing style, but if you prefer a `chrome::` submodule, changing it AFTER two more edges consume it is much more expensive than changing it now.

**Real latent bug, worth fixing when the second window lands:** the exhaustive CSS-class toggle lists are still hard-coded in the GTK painter while the class NAMES are now decided in another crate, with no test tying the two together. Adding a fifth trust posture in core would silently leave every painter with a stale class list. Exporting the class set (or an enum) from core is the fix; it becomes urgent the moment a second painter exists.

**For the human: a glossary question I did not act on.** The nit asks whether `CONTEXT.md` should pin the "presentation lives in the core / an edge is a PAINTER" pair, now that it is load-bearing across ADR-0011, four backlog tasks and the code. The glossary defines "seam" but not "painter", so the next author can re-fork the term. I left `CONTEXT.md` alone deliberately: it is the curated domain vocabulary, so the wording should be yours.

**Pre-existing noise, captured not fixed:** the build agent filed `fetcher-generic-array-as-slice-deprecation-warnings-2026-07-30.md` for the two `generic-array` deprecation warnings that sit permanently in the gate output. Unrelated to this task; will become a hard break when the `sha2`/`digest` lineage moves to `generic-array 1.x`.
