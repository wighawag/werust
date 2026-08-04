# Registering the spinner + collapsed-control fields in the mobile presentation guard

Task `register-the-new-chrome-fields-in-the-mobile-presentation-guard`, spec `chrome-conventional-controls` (stories 8 + 10). The CONTRACT step of the expand -> migrate -> contract sequence that brought the loading spinner and the collapsed reload/stop control to the mobile edges: the core derived them (`reload-stop-collapse-and-loading-spinner-core-and-gtk`), each mobile edge consumed them (`android-…` then `ios-chrome-collapse-reload-stop-and-drop-history-buttons`), and this change makes the CENTRAL guard demand them of both. The decisions are in `DECISIONS.md` beside this file.

## Drift check (done before anything was changed)

The task is a launch snapshot, so all three premises were re-verified against the code first:

- **The fields exist in the chrome JSON under the names the earlier task gave them.** `werust_core::chrome_json` encodes `reloadStopControl`, `reloadStopControlLabel`, `reloadStopControlDescription`, `loadSpinnerVisible` (`crates/werust-core/src/lib.rs`), each asserted there to agree verbatim with `reload_stop_control(...)` / `load_spinner_visible(...)`.
- **Both mobile edges really read them.** Kotlin decodes three of the four in `WerustCore.kt` and paints them in `BrowserActivity.kt`; Swift decodes the same three in `WerustCore.swift` and paints them in `WKWebViewShellController.swift`. Neither decodes the mode's wire name `reloadStopControl`, deliberately and identically (see `DECISIONS.md` §1).
- **The guard still has the shape the task describes**, and no sibling had registered the fields: `FACT_FIELDS` / `DERIVED_FIELDS` were untouched, and both per-edge guards still asserted the fields were absent from them.

No drift, with one mechanical consequence the task text does not name: those two per-edge assertions are the SEQUENCING hold that this step retires, so registering the fields necessarily changes them (`DECISIONS.md` §2).

## What changed

`crates/werust-core/tests/mobile_chrome_presentation_shape.rs` (the only production change; the guard IS the deliverable):

- `DERIVED_FIELDS` gains `reloadStopControlLabel`, `reloadStopControlDescription`, `loadSpinnerVisible`, so the existing assertions now cover them: both bindings must DECODE each (`both_mobile_bindings_decode_every_derived_field`) and both painters must PAINT from each (`both_mobile_painters_paint_from_the_derived_fields`).
- `every_derived_string()` gains the collapsed control's own vocabulary, driven from the core over `ReloadStopControl::ALL` (kept exhaustive by a compile-time check, exactly like the posture and step axes already there): every `label()` glyph and every `description()`. That is the FORBIDDEN-LITERAL half the task's forward-pointer flagged (`work/notes/observations/mobile-guard-forbidden-literals-are-a-hand-picked-rule-list-2026-08-04.md`) — without it a mobile edge could hardcode `⟳` or "Stop loading this page" and stay green.
- Nothing was relaxed, restructured or special-cased: the change is three list entries, one loop over an enum the core already exports, and comments.

`crates/werust-android/rust/tests/collapsed_control_and_dropped_history_buttons_shape.rs` and its iOS twin: the sequencing assertion `the_mobile_presentation_guard_field_lists_are_not_registered_here` is INVERTED into `the_mobile_presentation_guard_registers_the_fields_this_edge_consumes` (`DECISIONS.md` §2). Same list, same scan, opposite direction.

No Kotlin, Swift or `werust-core` source changed. This step adds protection to wiring that already exists.

## What CI proves, and the teeth check

The pure-Rust `verify` gate (`cargo fmt --check && cargo clippy --all-targets -D warnings && cargo build && cargo test`) runs all of it; no network, no Android SDK, no Xcode.

A guard entry that cannot fail is worse than none, so the new entries were mutation-checked rather than assumed. Each mutation below was applied to a real edge source, the gate observed RED, and the source restored:

| Mutation | Test that went red |
| --- | --- |
| Swift painter drives the spinner from `chrome.loading` instead of `chrome.loadSpinnerVisible` | `both_mobile_painters_paint_from_the_derived_fields` ("must paint from the carried `chrome.loadSpinnerVisible`") |
| Kotlin binding stops decoding `reloadStopControlDescription` and hardcodes "Reload this page" | `both_mobile_bindings_decode_every_derived_field` AND `no_mobile_edge_restates_a_string_the_core_derivation_produces` |
| Swift painter titles the control with a literal `"⟳"` instead of `chrome.reloadStopControlLabel` | `no_mobile_edge_restates_a_string_the_core_derivation_produces` ("restates the core's own derivation") |

The second and third mutations are what prove the `every_derived_string()` half has teeth: both the DESCRIPTION and the GLYPH halves of the new rule are really forbidden at the edges, not merely documented as such.

The inverted per-edge assertions were likewise seen red before the inversion: registering the fields made both `the_mobile_presentation_guard_field_lists_are_not_registered_here` tests fail with their sequencing message, which is the hand-off firing exactly as designed.

## Residual risk

`every_derived_string()` is still exhaustive over ENUM AXES but names the RULES it drives BY HAND, so the NEXT presentation rule's strings will again reach mobile unguarded until someone remembers to add them. This change adds one more hand-picked entry; it does not fix the pattern. The signal stays captured in `work/notes/observations/mobile-guard-forbidden-literals-are-a-hand-picked-rule-list-2026-08-04.md`.
