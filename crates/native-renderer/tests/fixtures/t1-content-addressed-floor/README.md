# T1 content-addressed floor — the `ipfs://` static-site golden fixture

This is the committed golden fixture for the conformance ladder's **T1
content-addressed floor** (`docs/conformance-tiers.md` T1; user story 16 of the
ship spec, task `t1-content-addressed-floor-ipfs-static-site`). Where the T1
[server floor](../t1-server-floor/) pins pages served straight to the native path,
this floor pins a real static site **fetched by CID over the hash-verified
`ipfs://` path** and rendered **at parity with the server path** — this is where
the thesis lands FIRST: a verifiable, content-addressed document opened as a
first-class page.

- `site.html` — a **Jekyll/Hugo-class static docs/landing site** (a
  `<meta name="generator" content="Jekyll ...">`, a masthead, an `<h1>` + lede,
  `<h2>` sections, a `<blockquote>`, an unordered list, links, inline emphasis, a
  small core-CSS stylesheet, a site footer). It is a single self-contained HTML
  file so it fits in one verified content block. Its provenance (the class of site
  it captures, the snapshot date, and why the CID is derived) is recorded in
  [`SOURCE.md`].

## The path this proves

The site is **pinned to a CID derived from its bytes** ([`fetcher::cid_v1_raw_sha256`],
a single-block CIDv1 raw `sha2-256` — the scope the `fetcher` verifies), resolved
through the **hash-verified content-addressed `ipfs://` path**
(`werust_core::ipfs::resolve_ipfs_request` over a `VerifyingContentFetcher`, task
`ipfs-scheme-resolution-through-renderer-seam` +
`fetcher-hash-verified-content-addressed-path`), and rendered through the **native
T1 path** — html5ever parse + the core-CSS cascade + parley Latin/LTR shaping
(`t1-whatwg-parser-html5ever-behind-tokenizer-seam` +
`t1-core-css-stylo-and-latin-shaping-parley`) — driven THROUGH the `Renderer`
seam exactly as the browser shell would.

Everything is **off the live network**: the content source is an in-memory,
per-test map, and the CID is derived from the committed bytes so it verifies
deterministically. Shaping is reproducible because it is pinned to the crate's one
bundled font (`assets/DejaVuSans.ttf`); the golden is stable **only** against that
font.

## Files

- `site.html` — the pinned static-site snapshot (the input).
- `site.golden.txt` — the expected render reference: the painted software-text
  transcript (flow order + style marks `[b]`/`[i]`/`[u]` and a non-black colour
  mark `#rrggbb`) the native T1 path must reproduce, byte for byte, at the pinned
  viewport width. The `#rrggbb` colour mark makes a colour-cascade regression turn
  the golden red.

The fixture viewport width is pinned in the golden test
(`tests/t1_content_addressed_floor.rs`) so inline wrapping is stable.

## The T1 content-addressed floor guards (in `tests/t1_content_addressed_floor.rs`)

1. **Parity guard** (`content_addressed_site_renders_at_parity_with_the_server_path`):
   the exact site bytes are rendered BOTH directly (the served `data:text/html`
   path) AND through the verified `ipfs://` path, and the two painted transcripts
   are asserted byte-for-byte identical — the content-addressed path is at parity,
   not a second-class renderer. That shared render is also asserted equal to
   `site.golden.txt`, so a regression anywhere in the native T1 path turns the
   golden red under `verify`.
2. **Native-path + shaping guard** (`the_site_renders_via_the_native_t1_path_with_shaped_text`):
   every run has a positive proportional shaped advance and a real font line
   height, heading vs body lines differ in height, and the `.note` author colour
   cascades to the surface — proof the real T1 native path (not a stub) drove
   geometry and colour.
3. **Trust gate** (`a_hash_mismatch_fails_the_content_addressed_load_and_never_renders`):
   tampered bytes that do not hash to the pinned CID FAIL the load and never
   reach the renderer — the content is hash-verified on the way in.
4. **Determinism / isolation guard**
   (`the_pinned_cid_is_derived_from_the_site_and_verifies_deterministically`):
   the pinned CID is deterministic for the site bytes and resolves to exactly
   them, off the network.
5. **T1-scope guard** (`the_site_stays_within_the_t1_static_scope`): the site uses
   no T2 constructs (tables/floats/flex/grid) and no T3 constructs (`<script>`), so
   the floor can never quietly claim more than T1 defines.

## Regenerating the golden

The golden is intentionally committed (it is the reference). If an *intended*
change to the T1 render path shifts it, regenerate with:

```sh
cargo test -p native-renderer --test t1_content_addressed_floor -- --ignored regenerate_goldens
```

then review the diff before committing (a golden change is a rendering change).
