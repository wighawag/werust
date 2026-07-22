# T1 server-floor fixtures — provenance + pinning

The T1 server-web floor requires two INDEPENDENTLY-authored real static pages,
each **pinned to a specific snapshot so the fixture is stable and reproducible**
(`docs/conformance-tiers.md` T1; task `t1-server-web-floor-article-and-blog`,
acceptance criteria 1-2 + 4). The tests must be **isolated from the live network**
(criterion 4), so the pages are pinned as **committed local snapshots** rather than
fetched at test time: each `<name>.html` in this directory is the frozen bytes the
native path renders. Nothing is fetched — the golden test hands the committed bytes
to the native T1 path through a `data:text/html,…` URL.

Why snapshots and not a live fetch of the exemplar URLs: T1 has no networking yet
(the `Fetcher` seam + `ipfs://` resolution are separate tasks — the native backend
`navigate`s only self-contained `data:` documents today), and pinning to live URLs
would make the fixture non-reproducible (the upstream page can change under us) and
couple the T1 floor to network access under `verify`. So each page is authored to
its pinned CLASS from the checklist and frozen here; the snapshot IS the pin.

## The two pinned pages

### `article.html` — a `motherfuckingwebsite.com`-class minimal semantic page

- **Class (pinned by the checklist):** a content-first minimal semantic-HTML
  article/doc page, as exemplified by
  <https://motherfuckingwebsite.com/> (checklist: "an MDN article / a
  `motherfuckingwebsite.com`-class minimal semantic-HTML page").
- **Snapshot:** hand-authored to that class and frozen 2026-07-22. It is an
  ORIGINAL authored page in the spirit + shape of the exemplar (a single
  content-first document: a `<header>` with an `<h1>` + tagline, `<h2>` sections,
  paragraphs, an unordered list, inline emphasis, links, named entities, and a
  small core-CSS stylesheet) — not a byte copy of the upstream page. It stays
  strictly inside the T1 static scope (no floats/flex/grid/tables, no JS).

### `blog-post.html` — an independent static-site-generator blog post

- **Class (pinned by the checklist):** a second, independently-authored static
  page — "a static-site-generator blog post" — so the tier is not tuned to one
  exemplar. Modelled on a Hugo/Jekyll-class rendered post (a `<meta name="generator"
  content="Hugo">`, a site header, an `<article>` with post metadata, a
  `<blockquote>`, an ordered list).
- **Snapshot:** hand-authored to that class and frozen 2026-07-22. A DIFFERENT
  author's document structure from `article.html` (site chrome + article + metadata
  + blockquote + ordered list) so the two exemplars are genuinely independent. Also
  strictly inside the T1 static scope.

## Re-pinning

To replace a snapshot with a captured copy of a live upstream page later (once a
`Fetcher` exists), overwrite `<name>.html` with the captured bytes, record the
exact source URL + capture date/commit here, then regenerate the golden
(`cargo test -p native-renderer --test t1_server_floor_goldens -- --ignored
regenerate_goldens`) and review the diff. The snapshot is always the pin — the
golden is asserted against the committed bytes, never a live fetch.
