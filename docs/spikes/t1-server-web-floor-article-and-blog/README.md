# Spike: T1 server-web floor — real article/doc page + independent blog post

Durable evidence + decisions for task `t1-server-web-floor-article-and-blog`
(spec story 15, `docs/conformance-tiers.md` T1). This establishes the **T1
server-web floor**: two independently-authored real static pages that render
correctly via the native T1 path, pinned to committed snapshots and asserted
against stable goldens. It pairs with the content-addressed floor
(`t1-content-addressed-floor-ipfs-static-site`) — T1 needs BOTH — and is guarded
objectively by the WPT meter (`t1-wpt-subset-regression-meter`); the page checklist
here is what DRIVES the tier.

## What was built

In `crates/native-renderer/`:

- **Two pinned real-page fixtures** under
  `tests/fixtures/t1-server-floor/` (see its `README.md` + `SOURCE.md`):
  - `article.html` — a `motherfuckingwebsite.com`-class minimal semantic-HTML
    article/doc page (a full `<!doctype>`, a `<header>`, headings, paragraphs, an
    unordered list, links, inline emphasis, named entities, a core-CSS stylesheet).
  - `blog-post.html` — a second, INDEPENDENTLY-authored static-site-generator
    (Hugo-class) blog post (site header, `<article>` + post metadata, `<h2>`,
    `<blockquote>`, an ordered list, inline emphasis) — a different author's
    document structure, so the tier is not tuned to one exemplar.
  Each page has a committed `<name>.golden.txt`: the painted software-text
  transcript (flow order + `[b]`/`[i]`/`[u]` style marks + a non-black `#rrggbb`
  colour mark) the native T1 path must reproduce byte-for-byte.
- **The T1 server-floor golden suite** (`tests/t1_server_floor_goldens.rs`): renders
  each pinned page through the native T1 path (html5ever parse + core-CSS cascade +
  parley Latin/LTR shaping), driven THROUGH the `Renderer` seam as the shell would,
  and asserts (1) byte-equality to the committed golden, (2) real shaped runs with
  positive proportional advances and per-font-size line heights, (3) author colour
  cascaded onto the SURFACE (not just the transcript), and (4) that the fixtures
  stay strictly inside the T1 static scope (no floats/flex/grid/tables, no JS).

## Reproducing

```sh
cargo test -p native-renderer --test t1_server_floor_goldens
```

Regenerate the goldens after an intended render change (then review the diff):

```sh
cargo test -p native-renderer --test t1_server_floor_goldens -- --ignored regenerate_goldens
```

## Decisions

### D1 — Pin the pages as committed local snapshots, not live fetches

The task requires each page "pinned to a specific snapshot/commit … stable and
reproducible" AND "isolated from the live network (use captured snapshots)". T1 has
no networking yet (the `Fetcher` seam + `ipfs://` resolution are separate tasks; the
native backend `navigate`s only self-contained `data:` documents today), so the
pin is a COMMITTED SNAPSHOT: each `<name>.html` is frozen bytes the golden test
hands to the native path through a `data:text/html,…` URL. The snapshot IS the pin.

- **Alternative considered (rejected):** fetch the exemplar URLs at test time and
  pin by upstream commit/etag. Rejected because there is no fetch path at T1, it
  would couple the `verify` gate to network access, and the upstream page can change
  under us (non-reproducible). Re-pinning to a captured copy of a live page later
  (once a `Fetcher` exists) is documented in `SOURCE.md`; the golden is always
  asserted against committed bytes, never a live fetch.
- **Touches:** the content-addressed floor (`t1-content-addressed-floor-ipfs-static-site`)
  will render a page fetched BY CID; this server floor deliberately renders from
  committed bytes, so the two floors differ only in how the bytes arrive.

### D2 — The golden is the paint transcript (words + style + `#rrggbb` colour), not a pixel image

The floor reuses the crate's existing house style — the software-text transcript
the T0 server-floor goldens (`tests/t0_server_floor_goldens.rs`) and the core-CSS
task established — rather than introducing a pixel golden. The transcript records
styled WORDS in flow order with `[b]/[i]/[u]` marks and a non-black `#rrggbb` colour
mark, so it is legible, font-metric-stable, and free of a brittle pixel dependency,
while the colour mark makes a colour-cascade regression turn a golden red (closing
the T0 colour gap the core-CSS task already addressed in `paint::transcribe`). Real
pixels ARE still asserted (the native-path + colour-to-surface guards), just not as
the byte-for-byte golden.

- **Coherence:** this reuses the `paint::Surface::transcript()` concept + the T0
  golden-fixture pattern (`<name>.html` + `<name>.golden.txt` + a `#[ignore]`d
  `regenerate_goldens` helper) verbatim, so it does not fork a new golden concept —
  it extends the existing one to real pages.

### D3 — Fixtures leave `line-height` unset (avoid a cascade limitation, do not fix it here)

While authoring, I found the T1 cascade resolves a unitless `line-height` (e.g.
`body { line-height: 1.5 }`) to an ABSOLUTE px against the element's own font-size
and then inherits that absolute value, so every descendant gets the same line height
regardless of its font-size (per CSS a unitless line-height should inherit the
multiplier). That is a cascade limitation OUTSIDE this task's scope (this task pins
pages + goldens, it does not change the cascade), so the fixtures deliberately leave
`line-height` unset — lines then use the font-size-relative `normal`, and heading vs
body lines differ in height honestly. The observation is captured (not fixed) in
`work/notes/observations/t1-unitless-line-height-inherits-as-absolute-px.md`.

## Notes

- **Reproducibility depends on the one bundled font.** Shaping is pinned to
  `assets/DejaVuSans.ttf` (core-CSS spike D2), so the goldens are stable ONLY
  against that font (real proportional advances + synthesised bold/italic, not
  system fonts). Regenerating on a machine with different fonts would NOT change
  them — the shaper registers only the bundled font.
- **Scope line (core-CSS spike D4) respected:** the fixtures do not rely on a
  filled block-container `background-color` box (layout emits runs, not box rects at
  T1); colour is asserted on text runs, which the cascade + paint do deliver.
