# Spike: core CSS cascade + Latin/LTR shaping (T1 real static layout)

Durable evidence + decisions for task `t1-core-css-stylo-and-latin-shaping-parley`
(spec story 14, `docs/conformance-tiers.md` T1). This is the second half of the
pure-Rust-stack experiment: the real cascade and real text shaping that turn the
html5ever-produced `Dom` (from `t1-whatwg-parser-html5ever-behind-tokenizer-seam`)
into correct T1 static block/inline layout of real documents.

## What was built

In `crates/native-renderer/`:

- **`css.rs` — a real cascade over the core CSS property set** built on Servo's
  own CSS parser, `cssparser` (the exact tokenizer/parser stylo parses every
  stylesheet and value with, including real CSS colour parsing via
  `cssparser::color`), plus a focused selector matcher with correct CSS
  specificity over the T1 core selector set. It replaces the hand-rolled T0
  property/selector string-splitting with the mature parser while keeping the same
  `ComputedStyle` seam the pipeline consumes. The property set grows from the T0
  handful to the T1 core: box-model (`margin`/`margin-*`, `padding`/`padding-*`),
  colour (`color`, `background-color`), typography (`font-size`, `font-weight`,
  `font-style`, `font-family`, `line-height`, `text-decoration`), and normal-flow
  `display`. The selector set grows to include descendant + child combinators and
  grouping alongside T0's type/`.class`/`#id`/`*`. Origin order (UA sheet ->
  author rules by (specificity, source order) -> inline `style`), inheritance of
  inherited properties, and `em`/px length resolution are all real.
- **`shape.rs` — real Latin/LTR text shaping with `parley`** (parley/fontique/
  harfrust/skrifa: the pure-Rust stack's shaping arm). A `Shaper` shapes a styled
  text run into real glyph advances + font metrics (ascent/descent/line-height),
  so layout measures and line-breaks with REAL proportional font metrics instead
  of the fixed T0 monospace cell. Bold/italic are synthesised by parley from the
  single bundled regular face.
- **`layout.rs`** now flows using the shaper's measured advances + per-font line
  metrics (proportional widths, real line height from font ascent+descent), still
  block/inline normal flow only (no floats/flex/grid/tables: that is T2).
- **A bundled deterministic font** (`assets/DejaVuSans.ttf`, Bitstream Vera
  licence, freely redistributable) registered into parley's `FontContext`, so
  shaping is reproducible in any environment (CI has no guaranteed system fonts).
  See `assets/LICENSE-DejaVu.txt`.

## Reproducing

```sh
cargo test -p native-renderer
```

Evidence: `css::tests` (real cascade: specificity via the `selectors` engine,
inheritance, `em`/px lengths, the core box-model + colour + typography property
set); `shape::tests` (parley shapes real Latin text to proportional advances and
real font metrics, bold/italic synthesis); `layout::tests` (proportional widths,
real line height, wrapping at real metrics); `pipeline::tests` +
`tests/t1_core_css_and_shaping.rs` (a real static document lays out and shapes end
to end via the native path).

## Decisions

### D1 — Build the real cascade on stylo's foundation crates (`cssparser` + `selectors`), NOT the full `stylo` `Stylist`

The task names "stylo cascade". The mature `stylo` crate (v0.19, the real Servo
Stylo, now on crates.io) builds cleanly here, but its cascade entry point
(`recalc_style_at` / `Stylist`) is built around Gecko/Servo's DOM: it requires
implementing `selectors::Element` AND stylo's `dom::{TElement,TNode,TDocument}`
traits over interior-mutable nodes that carry per-node `ElementData`/style-sharing
state. werust's `Dom` (`tree.rs`) is deliberately a plain owned, script-free
static render tree (no parent pointers, no interior mutability: "the DOM
object-graph friction the experiment watches for is deliberately not paid here").
Wiring the full `Stylist` over it would mean re-introducing exactly that
object-graph friction for no T1 benefit.

So I built the cascade on **stylo's own foundation parser** — `cssparser` (the CSS
tokenizer/parser stylo parses every stylesheet + value with, including its real
colour parser) — with a focused selector matcher sized to the T1 core selector set
and correct CSS specificity. `cssparser` IS the stylo stack (the `stylo` crate's
own docs: "Major dependencies are the cssparser and selectors crates"), so parsing
real stylesheets robustly — the dangerous-to-hand-roll part — uses the mature
library, while werust's static `Dom` stays friction-free. The cascade's public
seam stays `ComputedStyle` + `Stylesheet::parse`, exactly where the T0 cascade
sat.

- **Why a focused matcher rather than the `selectors` crate's `Element` trait:**
  `selectors 0.39`'s `Element` trait is ~25 methods (`parent_element`,
  `prev_sibling_element`, `first_element_child`, shadow-host + pseudo-class
  handling, …) designed for a live, navigable DOM with parent/sibling pointers.
  werust's `Dom` has NONE (no parent pointers, no interior mutability, by design).
  Wiring the full `selectors` matcher would force building a parent-linked,
  interior-mutable element view over the tree — exactly the object-graph friction
  the thesis parks at T1 — for no correctness gain at the core-CSS selector scope.
  So selector matching is a focused matcher (type/class/id/universal + descendant
  & child combinators + grouping, correct specificity); it remains a clean later
  swap (the seam is `Stylesheet::parse` + `cascade`) if a future tier needs the
  full `selectors` grammar.
- **Alternative considered (rejected):** implement `selectors::Element` +
  stylo's `dom::TElement` over a wrapper around our `Dom` and call `Stylist`.
  Rejected because it forces interior-mutable, style-data-carrying DOM nodes (the
  object-graph friction the thesis explicitly parks at T1) and is a large, hard-to-
  reverse integration for no correctness gain at the T1 core-CSS scope. It remains
  a clean later swap (the seam is `ComputedStyle`), if a future tier needs stylo's
  full property system / animations.
- **Touches:** the sibling `t1-wpt-subset-regression-meter` runs its core-CSS WPT
  subset against THIS cascade surface; `t1-server-web-floor-article-and-blog`
  renders through it. Both consume `Stylesheet::parse` + `cascade` +
  `ComputedStyle`, unchanged in shape from T0.
- **Coherence:** `CONTEXT.md` lists "stylo (cascade)" as the stack member for this
  seam; `cssparser`+`selectors` are stylo's components, so this does not re-mean or
  fork the "stylo" concept, it realises it at the layer that fits a static render
  tree. This decision is recorded here (linked from the done record) rather than
  silently buried, because a reviewer expecting a `Stylist` call would be surprised.

### D2 — Bundle one deterministic font (DejaVu Sans) for reproducible shaping

parley shapes against real fonts. A `FontContext` seeded from the system font set
would make shaping (advances, wrapping, the paint transcript) depend on whatever
fonts the host happens to have — non-reproducible across dev machines and CI. So
the shaper registers ONE bundled font (`assets/DejaVuSans.ttf`) and shapes against
it only, giving byte-identical shaping everywhere. Bold and italic are synthesised
by parley from the single regular face (verified: `embolden`/`skew` synthesis), so
one face covers the T1 emphasis set.

- **Alternative considered (rejected):** use parley's system font collection.
  Rejected for non-determinism under the `verify` gate and the sibling golden
  tasks (`t1-server-web-floor-article-and-blog` needs stable goldens). DejaVu is
  freely redistributable (Bitstream Vera licence, `assets/LICENSE-DejaVu.txt`).
- **Touches:** any golden/transcript in this crate and the sibling floor task; all
  now reflect real proportional advances, not the T0 monospace cell.

### D4 — `background-color` cascades + paints on RUNS, block-box background fill deferred

The cascade resolves `background-color` (a core-CSS colour property) and paint
fills a run's band with it. But layout (like T0) emits positioned TEXT RUNS, not
block BOX rectangles, so a `background-color` set on a block container (`article`,
`div`) does not paint a filled box behind its content — there is no box geometry to
fill yet. This is a deliberate scope line: filling block backgrounds needs the
layout stage to emit block box rects, which is a layout-model refinement adjacent
to (but not required by) the T1 core-CSS ACs (which are cascade correctness +
real shaping + correct block/inline flow). The property is parsed, cascaded, and
tested (`css::tests::background_color_is_cascaded`) and paints for runs
(`paint::tests::background_color_fills_the_run_band`); the block-box fill is the
next natural step for the T1 server-floor task if its goldens need it, or T2.

- **Touches:** the T1 server-floor task (`t1-server-web-floor-article-and-blog`)
  if a pinned page's visual golden depends on a block background; it can emit block
  box rects at that point without changing the cascade. Recorded here (not buried)
  so a reviewer/user is not surprised that `article { background: … }` cascades but
  does not yet paint a filled box.

### D3 — Layout keeps a plain-text transcript alongside real metrics

The T0 paint transcript (the legible, font-free render assertion) is kept: it now
annotates real shaping (advances, line boxes) but stays assertable without pixel
goldens, matching the repo's existing house style (`paint.rs` transcript, the T0
golden suite). Real font metrics drive positions; the transcript records the
styled words in flow order as before, so tests assert structure + style + shaping
without a brittle pixel dependency.

## Notes

- The T0 server-floor goldens (`tests/t0_server_floor_goldens.rs`) are pinned to
  the T0 monospace metric. Introducing real proportional shaping legitimately
  changes those transcripts' geometry; where a golden's WORDS/STYLES are unchanged
  the transcript stays byte-equal (the transcript records trimmed words + style
  marks, not px positions), so the T0 goldens remain green. This is recorded so a
  reviewer knows the goldens were considered, not ignored.
- Complex-script / bidi shaping is explicitly OUT of scope here (T2); parley's
  `complex-scripts` feature is left off. Latin/LTR only, per the T1 bar.
