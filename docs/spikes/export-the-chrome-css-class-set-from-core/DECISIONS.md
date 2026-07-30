# Decisions — export the chrome's CSS-class set from core

Task: `export-the-chrome-css-class-set-from-core`.

These are the shape choices the task left open ("a `pub const` slice or an enum, whichever reads better"). They are recorded because the exported shape is the surface THREE queued painters bind to (`macos-wkwebview-backend-and-window`, `windows-webview2-backend-and-window`, `mobile-chrome-presentation-from-one-derivation`), so a reviewer or a later author should be able to ratify or reverse them deliberately, exactly as the `pub fn` API shape for the twelve chrome rules was ratified on 2026-07-30.

## D1 — a `pub const` slice of names, not an enum of states

**Chosen:** two `pub const &[&str]` families in `werust-core` beside the rules that produce them: `TRUST_INDICATOR_CSS_CLASSES` and `ERROR_BANNER_CSS_CLASSES`.

The existing `*_css_class` rules already return `&'static str`, and every consumer wants exactly that: a painter interpolates the name into `add_css_class` / `remove_css_class`, and the no-unstyled-class guard interpolates it into a stylesheet selector. A slice of the very strings the rules return needs no conversion at any call site and cannot disagree with them.

**Alternative considered:** an enum (e.g. `ChromeCssClass`) with a `css_name()` method, with the `*_css_class` rules returning the enum. Rejected for now: it would change the SIGNATURE of two rules that three edges (and the ratified crate-root `pub fn` surface) already consume, forcing every painter to call `.css_name()`, in exchange for a compile-time exhaustiveness that the test already gives at the same strength. The enum stays the natural upgrade if a class ever needs to carry data beyond its name; it is a mechanical change from here.

**What it touches:** the three queued painter tasks consume this const (each iterates it in its own `refresh` and asserts its own stylesheet styles every member); nothing else. The two mobile edges paint native colours rather than CSS, so they consume the postures, not these names.

## D2 — grouped by mutually-exclusive FAMILY, not one flat list

**Chosen:** `CHROME_CSS_CLASS_SETS: &[&[&str]]` = the two families, with each family also exported by name.

A painter's toggle loop is per-WIDGET and must cover exactly one family: the trust badge turns one `trust-*` on and the other `trust-*` off, while the error banner does the same among the `error-banner*` names. A single flat list would make a painter iterate names belonging to another widget (or invite it back to a hand-written subset, the very bug this task closes), so the grouping is the useful unit. The "complete set" is then a one-line flatten (`CHROME_CSS_CLASS_SETS.iter().flat_map(...)`), which is what the no-unstyled-class guard iterates.

**Alternative considered:** a single flat `CHROME_CSS_CLASSES` const, with the families as sub-slices. Rejected: deriving the sub-slices in const context is awkward, and re-stating the names in both places would reintroduce a second list to keep in sync.

**What it touches:** the same three painter tasks. A test asserts no name appears in two families, so the grouping cannot become ambiguous.

## D3 — a source-shape guard for the painter's wiring, beside the two teeth

**Chosen:** the two teeth the task names (core exhaustiveness in `crates/werust-core/src/lib.rs`, no-unstyled-class in `crates/werust/src/main.rs`) plus a small third guard, `crates/werust-core/tests/chrome_css_class_set_edge_wiring_shape.rs`.

The exhaustiveness tooth only BITES while the painter really derives its toggle list from the exported set; a painter that drifted back to a literal list would keep a green suite and go stale on the next posture. Asserting the toggle at runtime needs a display (GTK widget classes), which the `verify` gate may not have, so the guard parses the desktop shell for the wiring, exactly as the sibling `debug_view_desktop_wiring_shape.rs` and `browser_menu_edge_wiring_shape.rs` do. It also pins the layering: the stylesheet stays in the edge and the core mentions no colour.

**What it touches:** each new painter should extend this guard (or add its sibling) so its own toggle lists are pinned the same way.

## Mutation-checked teeth

Each tooth was verified to actually red the gate, by mutating the source and re-running (all mutations reverted):

- a new class returned by `trust_indicator_css_class` without extending the set: core exhaustiveness test fails, naming the class and the `ChromeState` shape that produced it;
- a name added to the exported set that no rule can return: core test fails (the set carries no dead name, which would otherwise demand a stylesheet rule for nothing);
- a class renamed in core without updating `APP_CSS`: the edge's no-unstyled-class test fails, and the debug view's `trust-*` reuse test fails with it (ADR-0006 vocabulary, covered by the same guarantee);
- the painter reverted to a literal toggle list: the wiring-shape guard fails.
