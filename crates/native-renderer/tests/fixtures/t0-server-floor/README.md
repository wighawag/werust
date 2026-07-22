# T0 server-web floor — golden fixtures

These are the committed golden fixtures for the conformance ladder's **T0
server-web floor** (`docs/conformance-tiers.md` T0; user story 11 of the ship
spec). Each fixture is an authored, self-contained static HTML *fragment* that
uses **only** the fixed v0 subset:

- elements on [`native_renderer::tree::ELEMENT_ALLOWLIST`],
- CSS properties on [`native_renderer::css::SUPPORTED_PROPERTIES`],
- and the T0 selector set (type, `.class`, `#id`, `*`).

Layout is deterministic (a fixed monospace text metric, software text — no font
backend, no GPU), so the native T0 path renders each fixture to an **exact,
reproducible** software-text transcript. That transcript is the fixture's golden
reference: it is the software equivalent of a golden image, without a font
dependency.

## Files

For each fixture `<name>`:

- `<name>.html` — the authored v0-subset fragment (the input).
- `<name>.golden.txt` — the expected render reference: the painted software-text
  transcript (flow order + style marks `[b]`/`[i]`/`[u]` and a non-black colour
  mark `#rrggbb`) the native path must reproduce, byte for byte, at the pinned
  viewport width. The transcript records styled WORDS + marks, not px positions, so
  it is font-independent and stable across the T0 monospace metric and the T1 real
  shaping metric alike.

> **Colour re-baseline (T1 `t1-core-css-stylo-and-latin-shaping-parley`).** These
> goldens were consciously regenerated when the T1 core-CSS task added a colour
> mark to the paint transcript. The T0-server-floor Gate-3 note flagged that the
> transcript did NOT assert colour while `paint::transcribe()` overclaimed that it
> did; the T1 change closes that gap, so a colour-cascade regression now turns a
> golden red. The diff is colour-only: every fixture's words + `[b]/[i]/[u]` marks
> are byte-identical to the pre-T1 goldens; only `#rrggbb` colour marks were added.
> See `docs/spikes/t1-core-css-stylo-and-latin-shaping-parley/README.md`.

The fixture viewport width is pinned in the golden test
(`tests/t0_server_floor_goldens.rs`) so wrapping is stable.

## The two guards (the T0 regression guard)

`tests/t0_server_floor_goldens.rs` enforces both halves of the T0 regression
guard (there is no WPT bar at T0 — a fixed private subset has no meaningful
public pass-rate):

1. **Golden-image guard.** The native T0 path renders each `<name>.html` and the
   transcript is asserted **equal** to `<name>.golden.txt`. A rendering
   regression (tokenizer, cascade, layout, or paint) makes a golden mismatch, and
   the test — run under the `verify` gate (`cargo test`) — goes red.
2. **Subset-doc-drift guard.** Every `<name>.html` is checked to use **only** the
   documented v0 allowlist (elements + CSS properties + selectors). A fixture that
   drifts outside the subset fails the guard, so the golden suite can never quietly
   start covering constructs T0 does not actually define.

## Regenerating the goldens

The goldens are intentionally committed (they are the reference). If an
*intended* change to the T0 render path shifts them, regenerate with:

```sh
cargo test -p native-renderer --test t0_server_floor_goldens -- --ignored regenerate_goldens
```

then review the diff before committing (a golden change is a rendering change).
