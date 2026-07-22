# T1 server-web floor — real-page golden fixtures

These are the committed golden fixtures for the conformance ladder's **T1
server-web floor** (`docs/conformance-tiers.md` T1; user story 15 of the ship
spec, task `t1-server-web-floor-article-and-blog`). Where the T0 floor pins
authored *subset* fragments, T1 pins **two independently-authored REAL static
pages** — so the tier is proven on two exemplars, not tuned to one:

- `article` — a [`motherfuckingwebsite.com`](https://motherfuckingwebsite.com/)-class
  minimal semantic-HTML article/doc page: a full `<!doctype>`, a `<header>`,
  headings, paragraphs, an unordered list, links, inline emphasis
  (`<strong>`/`<em>`/`<b>`/`<i>`), named entities, and a small core-CSS stylesheet.
- `blog-post` — a second, independently-authored static-site-generator
  (Hugo/Jekyll-class) blog post: a site header, an `<article>` with post metadata,
  an `<h2>`, a `<blockquote>`, an ordered list, and inline emphasis — a different
  author's document structure.

Both are **pinned local snapshots**: their provenance (the class of page each
captures, and the snapshot date) is recorded in [`SOURCE.md`]. The golden tests are
**isolated from the live network** — nothing is fetched; each page is rendered from
its committed bytes handed to the native path through a `data:text/html,…` URL.

## The native T1 path these prove

Each page renders through the **native T1 path** — html5ever parse behind the
`Parser` seam (`t1-whatwg-parser-html5ever-behind-tokenizer-seam`) + the core-CSS
cascade + parley Latin/LTR shaping (`t1-core-css-stylo-and-latin-shaping-parley`) —
driven THROUGH the `Renderer` seam exactly as the browser shell would.

Shaping is reproducible because it is pinned to the crate's one bundled font
(`assets/DejaVuSans.ttf`); the goldens are stable **only** against that font (real
proportional advances + synthesised bold/italic metrics, not system fonts — see the
core-CSS spike, decision D2).

## Files

For each fixture `<name>`:

- `<name>.html` — the pinned page snapshot (the input).
- `<name>.golden.txt` — the expected render reference: the painted software-text
  transcript (flow order + style marks `[b]`/`[i]`/`[u]` and a non-black colour
  mark `#rrggbb`) the native T1 path must reproduce, byte for byte, at the pinned
  viewport width. The transcript records styled WORDS + marks, not px positions, so
  it is font-metric-legible and stable without a brittle pixel dependency, while the
  `#rrggbb` colour mark makes a colour-cascade regression turn a golden red.

The fixture viewport width is pinned in the golden test
(`tests/t1_server_floor_goldens.rs`) so inline wrapping is stable.

## The T1 floor guards (in `tests/t1_server_floor_goldens.rs`)

1. **Golden-image guard** (`renders_each_real_page_at_golden_parity`): the native
   T1 path renders each `<name>.html` and the transcript is asserted **equal** to
   `<name>.golden.txt`. A regression anywhere in parse / cascade / shaping / layout
   / paint makes a mismatch, and the `verify` gate (`cargo test`) goes red.
2. **Native-path + shaping guard** (`each_page_renders_via_the_native_t1_path_with_shaped_text`):
   every run has a positive proportional shaped advance and a real font line height,
   and heading vs body lines differ in height — proof the real shaper (not a
   monospace cell) drove geometry.
3. **Colour-cascade-to-surface guard** (`colour_cascade_reaches_the_surface`):
   author colour rules over the core-CSS colour set are painted onto the surface,
   not merely recorded in the transcript.
4. **T1-scope guard** (`fixtures_stay_within_the_t1_static_scope`): the pages use no
   T2 constructs (tables/floats/flex/grid) and no T3 constructs (`<script>`), so the
   floor can never quietly claim more than T1 defines.

## A note on `line-height`

The fixtures deliberately leave `line-height` unset (lines use the font-size-relative
`normal`), so heading and body lines differ in height by font size. A unitless
`line-height` on `body` used to inherit as an absolute px in the T1 cascade (so it
would flatten all line heights), see
`work/notes/observations/t1-unitless-line-height-inherits-as-absolute-px.md`. Task
`fix-unitless-line-height-inherits-as-multiplier` fixed that (a unitless value now
inherits as the multiplier and re-resolves per element's own font-size); the
fixtures still leave `line-height` unset, so these goldens are unaffected.

## Regenerating the goldens

The goldens are intentionally committed (they are the reference). If an *intended*
change to the T1 render path shifts them, regenerate with:

```sh
cargo test -p native-renderer --test t1_server_floor_goldens -- --ignored regenerate_goldens
```

then review the diff before committing (a golden change is a rendering change).
