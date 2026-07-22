# T1 content-addressed-floor fixture — provenance + pinning

The T1 content-addressed floor requires a real `ipfs://` static site — a
Jekyll/Hugo-class static docs/landing site — **fetched by CID and rendered at
parity with the server path** (`docs/conformance-tiers.md` T1; task
`t1-content-addressed-floor-ipfs-static-site`). The tests must be **isolated from
the live network** (acceptance criterion 4) AND the site must be **pinned to a
specific CID** (criterion 1), so the site is pinned as a **committed local
snapshot** whose CID is **derived from its bytes** at test time, rather than
fetched from a live gateway. Nothing is fetched: the golden test derives the
pinned CID with [`fetcher::cid_v1_raw_sha256`], stores the committed bytes under
it in an in-memory source, and resolves that CID through the hash-verified
`ipfs://` path.

Why a derived CID and not a live-IPFS CID: pinning to a CID that only a live
gateway can serve would make the fixture network-dependent under `verify` and
non-reproducible (a gateway can go away). Deriving the CID from the committed
bytes gives the SAME pin — a specific CID the site is addressed by — while keeping
the whole path off the network and deterministic. The derived CID is a **single-
block CIDv1 (raw codec, `sha2-256` multihash)**, which is exactly the scope the
`fetcher` verifies (DAG-PB / UnixFS multi-block traversal is out of scope in the
fetcher — see the forward-pointer in
`work/tasks/done/ipfs-scheme-resolution-through-renderer-seam.md`), so the pinned
site is authored as a **single self-contained HTML file** (inline CSS, no external
sub-resources) that fits in one verified block.

## The pinned site

### `site.html` — a Jekyll/Hugo-class static docs/landing site

- **Class (pinned by the checklist):** a real content-addressed static site — "an
  IPFS-hosted static docs/landing page (a Jekyll/Hugo-class site pinned to a
  specific CID)". It is distinct from the server floor's `article` +
  `blog-post` pages: the content-addressed floor is its OWN pinned page class per
  the checklist, so the tier is proven on a genuinely different exemplar reached
  through the `ipfs://` path.
- **Snapshot:** hand-authored to that class and frozen 2026-07-22. An ORIGINAL
  authored page in the spirit + shape of a static-site-generator docs/landing site
  (a `<meta name="generator" content="Jekyll v4.3.3">`, a masthead, an `<h1>` +
  lede, `<h2>` sections, paragraphs, a `<blockquote>`, an unordered list, links,
  inline emphasis, named entities, and a small core-CSS stylesheet, plus a site
  footer) — self-contained so it is one verified block. It stays strictly inside
  the T1 static scope (no floats/flex/grid/tables, no JS).

## Re-pinning

To replace the snapshot with a captured copy of a real live IPFS site later (a
single-block raw-`sha2-256` CID the `fetcher` can verify), overwrite `site.html`
with the captured bytes, record the exact source CID + capture date here, then
regenerate the golden (`cargo test -p native-renderer --test
t1_content_addressed_floor -- --ignored regenerate_goldens`) and review the diff.
The snapshot is always the pin — the golden is asserted against the committed
bytes rendered through the verified `ipfs://` path, never a live fetch.
