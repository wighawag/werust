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

## D4 — the drive is exhaustive BY CONSTRUCTION, via `Enum::ALL` + a compile-time completeness check

**Chosen:** each enum AXIS the chrome rules branch on gains an `ALL` list beside the enum — `TrustPosture::ALL` and `LoadState::ALL` (`crates/renderer/src/lib.rs`), `LoadStep::ALL` and `FailureKind::ALL` (`crates/werust-core/src/lib.rs`) — and the exhaustiveness test's drive (`every_chrome_state_shape`) iterates those lists instead of hand-written array literals. Each list is kept complete by an anonymous `const` check right below it.

This is the Gate-2 block from the first attempt (2026-07-30), and it is the whole point of tooth 1: with the drive written as a literal `for posture in […]`, adding a FIFTH `TrustPosture` did not force that list to grow, so an author could add the posture, add its `trust_indicator_css_class` arm, forget `TRUST_INDICATOR_CSS_CLASSES`, and keep a GREEN suite while the painter cleared all five classes and added none (an unstyled badge). A Phase-2 name-verified posture is anticipated in the `TrustPosture` docs, so that is a likely path, not a hypothetical.

The check is deliberately two-layered, because Rust cannot enumerate an enum's variants on its own:

```rust
const _TRUST_POSTURE_ALL_IS_EVERY_POSTURE_IN_SLOT_ORDER: () = {
    const fn listed(posture: TrustPosture) -> TrustPosture {
        match posture {
            TrustPosture::UnverifiedOrigin => TrustPosture::ALL[0],
            …
            TrustPosture::MutableName => TrustPosture::ALL[3],
        }
    }
    // … assert every entry maps back to its own slot
};
```

1. the `match` has no wildcard arm, so a new variant does not COMPILE until it is named here (`E0004`, at the list);
2. the arm an author then writes (`… => TrustPosture::ALL[4]`, the next slot) does not compile EITHER unless the variant also joins `ALL` — `index out of bounds`, the deny-by-default `unconditional_panic` lint;
3. the loop asserts each listed value maps back to its own slot, so a reordered or duplicated entry is a const-eval error too.

So the fifth posture reaches a green build only by being in `ALL`, hence in the drive, hence in the exported class set. `ALL` is the repo's existing vocabulary for this (`TrustHook::ALL`, `Candidate::ALL`), so no new concept is introduced.

**Alternatives considered:** (a) keeping the literal drive and only fixing the doc — rejected, that is the block; (b) a `strum`-style `EnumIter` derive — airtight, but `renderer` is a deliberately DEPENDENCY-FREE seam crate and this would be its first dependency; (c) a `macro_rules!` that defines the enum AND its list together — also airtight, but it would bury `TrustPosture`'s long per-variant docs inside a macro invocation and make the seam's central type un-greppable; (d) putting the exhaustive match only in the test helper — same strength for the test, but the compile error would land in another crate's test module instead of one line from the list an author is editing.

**What it touches:** four public seam/core consts that every future axis-covering test (and the three queued painter tasks) can reuse; and every future variant of those four enums, which now cannot land without visiting the list. What it does NOT catch is a posture added with NO new class branch: it falls into `trust_indicator_css_class`'s `else` and paints `trust-unverified`, which is the honest fail-closed default (a state werust cannot describe is not "verified"), so there is nothing to red.

## Mutation-checked teeth

Each tooth was verified to actually red the gate, by mutating the source and re-running (all mutations reverted):

- **the NEW-VARIANT path, end to end** — a fifth `TrustPosture::NameVerified` added to the enum, walked exactly as an author would:
  1. variant alone → `error[E0004]: non-exhaustive patterns: TrustPosture::NameVerified not covered` at the completeness check beside `TrustPosture::ALL` (the workspace does not build);
  2. the arm written as `TrustPosture::NameVerified => TrustPosture::ALL[4]` but `ALL` left at four entries → `error: this operation will panic at runtime … index out of bounds: the length is 4 but the index is 4` (`unconditional_panic`), so the forgotten-list path does not build either;
  3. `ALL` extended (plus the arms the compiler demands in `trust_posture_wire_name` and both mobile `ffi_json`s) and a `trust-name-verified` branch added to `trust_indicator_css_class` WITHOUT extending `TRUST_INDICATOR_CSS_CLASSES` → the core exhaustiveness test FAILS: ``‘trust-name-verified’ is returned for ChromeState { … trust_posture: NameVerified … } but is not in the exported set``. That is acceptance criterion 3, on the scenario that actually happens;
- a new class returned by `trust_indicator_css_class` on an EXISTING branch without extending the set: same core test fails, naming the class and the `ChromeState` shape that produced it;
- a name added to the exported set that no rule can return: core test fails (the set carries no dead name, which would otherwise demand a stylesheet rule for nothing);
- a class renamed in core without updating `APP_CSS`: the edge's no-unstyled-class test fails, and the debug view's `trust-*` reuse test fails with it (ADR-0006 vocabulary, covered by the same guarantee — that test now drives `TrustPosture::ALL` too, so a fifth posture's Network-tab class is covered by the same tooth);
- the painter reverted to a literal toggle list: the wiring-shape guard fails;
- an entry of `TrustPosture::ALL` reordered / duplicated: const-eval error at the completeness check.
