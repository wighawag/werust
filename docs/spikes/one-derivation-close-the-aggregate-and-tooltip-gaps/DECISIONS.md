# Decisions — close the aggregate and tooltip gaps in the shared chrome derivation

Task: `one-derivation-close-the-aggregate-and-tooltip-gaps`. Source spec: `work/specs/ready/rust-successor-native-renderer-architecture-benchmark.md`. Predecessor decisions (the class SETS themselves): `docs/spikes/export-the-chrome-css-class-set-from-core/DECISIONS.md`.

Both choices below are shape choices on a surface every future painter binds to (the queued `windows-webview2-backend-and-window` and `mobile-chrome-presentation-from-one-derivation`), so they are recorded here to be ratified or reversed deliberately rather than discovered later.

**Path note (2026-07-31, task `macos-harness-guard-teeth-and-paint-path-residue`).** The macOS painter half named below as `crates/werust-macos/src/paint.rs` no longer exists at that path: `windows-win32-window-and-chrome` EXTRACTED it verbatim, with its tests, to [`crates/desktop-paint`](../../../crates/desktop-paint) so both native desktop windows consume one carrier (`werust_macos::paint` re-exports it). `every_exported_class_has_a_colour` and the `CssClassFamily::ALL` iteration live there now, and there is one native-widget coverage gate rather than a per-edge one. The paths below are left as they were when the choices were made; only this note is new.

## D1 — the family aggregate is an ENUM (`CssClassFamily::ALL`), not a third `&[&[&str]]` const

**Chosen:** `pub enum CssClassFamily { TrustIndicator, ErrorBanner, DebugConsole }` in `werust-core`, with `CssClassFamily::ALL` (the aggregate the coverage gates iterate) and a total `const fn classes()` that hands back the very family `const` the rules are exported beside. `CssClassFamily::ALL` is kept complete by `_CSS_CLASS_FAMILY_ALL_IS_EVERY_FAMILY_IN_SLOT_ORDER`, the same anonymous-`const` total-`match` construction `TrustPosture::ALL` and `LoadStep::ALL` already use.

The repo's exhaustive-by-construction trick NEEDS an enum: the tooth is a `match` with no wildcard arm (a new variant does not compile until it is named) plus an `ALL[n]` index in the arm the author then writes (which does not compile until the variant joins `ALL`). A bare `pub const CSS_CLASS_FAMILIES: &[&[&str]] = &[…]` has no such tooth at all — nothing forces a new family into it, which is precisely the hole this task closes one level up from the classes. The enum buys a second tooth for free: `classes()` is total too, so a family that names itself in the enum but lists no classes does not compile either.

**Alternative considered:** a flat `pub const EXPORTED_CSS_CLASS_FAMILIES: &[&[&str]]` shaped exactly like `CHROME_CSS_CLASS_SETS`, pinned by a const check comparing it to a private enum. Rejected: comparing `&[&str]` values in const context is awkward (slice `==` is not const), so the check would degrade to a length/slot proxy that proves much less than a total match, while still costing a second list to keep in sync.

**What it touches:** both painter coverage gates (`crates/werust/src/main.rs`, `crates/werust-macos/src/paint.rs`) iterate `CssClassFamily::ALL`; the queued Windows painter and any later edge should do the same rather than naming families itself.

**Coherence with the existing vocabulary:** `CHROME_CSS_CLASS_SETS` is deliberately UNCHANGED and keeps its narrower meaning — what a chrome painter TOGGLES on one widget, exactly one class on and every other off — which is why the debug view's console levels are not a member of it. The new aggregate means something else and says so in its name and docs: every exported family, for COVERAGE only. The two are related by a test (`the_family_aggregate_holds_every_exported_class_family_for_the_coverage_gates`) asserting the aggregate is a strict SUPERSET of the toggling set, so neither can quietly become the other. No painter toggles a console class on a chrome widget.

## D2 — the stop-affordance label is a PARAMETER, with one shared default const

**Chosen:** `load_progress_tooltip(state: &ChromeState, stop_label: &str) -> Option<String>` beside the other `load_progress_*` rules, plus `pub const STOP_AFFORDANCE_LABEL: &str = "✕"`. Both desktop edges call `load_progress_tooltip(state, STOP_AFFORDANCE_LABEL)`.

What each edge actually shows today (the task asked for this check before choosing): the AppKit toolbar button's title is the literal `"✕"` (`crates/werust-macos/src/window.rs`), the Android and iOS Stop buttons are the literal `"✕"` too, and the GTK button is the themed `process-stop-symbolic` icon — the same stop cross, drawn by the icon theme rather than typed as a character. So the tooltip sentence is identical on both desktops today, and behaviour is unchanged by this move.

The label is nonetheless a parameter because the sentence names a UI affordance the EDGE owns, not a fact about `ChromeState`: an edge whose Stop control is really labelled differently (a Windows painter using a text "Stop", say) must be able to say so without forking the sentence — forking is exactly the failure this task exists to end. The shared `STOP_AFFORDANCE_LABEL` const keeps the default single: an edge passes the const unless it has a reason not to, so the parameter is an escape hatch rather than an invitation.

**Alternative considered:** no parameter, with `"✕"` baked into the core sentence. Simpler at every call site, and honest today since no edge differs. Rejected as the more expensive mistake to unwind: an edge that later needs a different label would have to either accept a wrong glyph in its tooltip or re-compose the sentence locally, which is the duplication being deleted here. Adding the parameter later is cheap; discovering a second fork is not.

**What it touches:** the two desktop painters today; the queued Windows painter and the mobile chrome-JSON path (`mobile-chrome-presentation-from-one-derivation`), where the Kotlin/Swift twins currently re-derive `loadProgressVisible` / `loadProgressPercent` / the phase hint by hand. Those twins have NOT forked this sentence — mobile shows no hover tooltip at all — so this task leaves them alone.

## Verification of the family tooth (acceptance criterion 3)

A fourth family was added in core during development (`NETWORK_ROW_SEVERITY_CSS_CLASSES = ["network-row-slow"]`, a plausible future "network-row severity") and then reverted. Three things happened, in this order:

1. Naming the variant `NetworkRowSeverity` in the const check without growing `ALL` did not COMPILE: `expected an array with a size of 3, found one with a size of 4` at `CssClassFamily::ALL`, plus `error: this operation will panic at runtime … index out of bounds: the length is 3 but the index is 3` (the deny-by-default `unconditional_panic` lint) on the `CssClassFamily::ALL[3]` arm. That is the by-construction half: the family cannot exist without joining the aggregate.
2. With `ALL` grown, the GTK stylesheet gate RED: `the core exports "network-row-slow" but "APP_CSS" has no ".network-row-slow { … }" rule, so the state would render invisibly` (`crates/werust/src/main.rs`, `every_chrome_css_class_the_core_exports_has_a_rule_in_the_app_css`).
3. The macOS palette gate RED on the same fact: `the core exports "network-row-slow" but this edge has no colour for it, so the state would render invisibly` (`crates/werust-macos/src/paint.rs`, `every_exported_class_has_a_colour`) — asserted on the Ubuntu gate, with no Mac involved.

Before this task, the same fourth family would have reddened NEITHER gate: each gate named its own families, so it would have kept a green suite while the new state painted with no rule and no colour on both desktops.
