# T1 WPT-subset fixtures — provenance + pinning

These are the pinned, committed fixtures the **T1 WPT-subset regression meter**
(`docs/conformance-tiers.md` T1; task `t1-wpt-subset-regression-meter`) runs against
the native T1 path. The meter must run **hermetically under `verify`** (`cargo test`,
offline, no reference browser, no JS engine — that is T3), and give a
comparable-over-time number, so the subsets are pinned as committed local fixtures
rather than fetched from the live upstream WPT tree at test time.

Two subsets, matching the two halves of the T1 bar:

## `tree-construction/` — `html/syntax/parsing/` (>= 90 %)

The html5lib-derived **tree-construction** tests: the WPT `html/syntax/parsing/`
area is populated from the upstream `html5lib-tests` `tree-construction/*.dat`
suite. These are a **self-contained plain-text format** (`#data` / `#errors` /
`#document`) that asserts the PARSE TREE, not pixels — so they need neither JS nor a
reference browser, and run directly against the native T1 parse path
(`Html5everParser::parse`, the real WHATWG parser behind the `Parser` seam).

- **Format:** the exact upstream html5lib `.dat` format (documented in
  `crates/native-renderer/src/wpt_meter/tree_construction.rs`). `#errors` is present
  (upstream shape) but IGNORED by the meter — werust asserts the resulting tree; the
  parser's own error recovery is exercised by its own tests
  (`t1-whatwg-parser-html5ever-behind-tokenizer-seam`).
- **Pinning:** the cases in `tests1.dat` / `tests2.dat` are a pinned subset authored
  in the upstream format and frozen 2026-07-22, chosen to cover the T1-relevant
  tree-construction behaviours a real static document exercises: nested
  block/inline, implied `<head>`/`<body>`, void elements (`<br>`), attributes,
  entity decoding, tables (implied `<tbody>`), and two classic error-recovery cases
  (the adoption agency `<b>1<p>2</b>3` and table foster-parenting `<table>x<td>`).
  They are NOT a byte copy of an upstream `.dat` file — each `#data` is authored to a
  pinned behaviour and its `#document` is the WHATWG-correct tree.
- **Normalisation:** werust's `Dom` is a static, script-free render tree that drops
  the doctype and comments (documented in `html5ever_parser.rs`). The meter strips
  `<!DOCTYPE …>` / `<!-- … -->` lines from the EXPECTED tree before comparing, so a
  deliberate, documented drop is not miscounted as a parse regression. Everything
  else (elements, text, attributes, nesting) is compared exactly.

### Re-pinning to the real upstream `.dat` files

To later run the actual upstream corpus: vendor
`html5lib-tests/tree-construction/*.dat` (or the WPT mirror of it) into this
directory (or fetch+cache it once a build step exists), pin the exact upstream
commit here, and the meter runs unchanged — it already parses the standard `.dat`
format. Expect the pass-rate to move (the full corpus has scripting-mode and
fragment-context cases outside the T1 static scope); the >= 90 % floor is on the
tree-construction behaviours T1 targets.

## `core-css/cases.txt` — the five core-CSS areas (>= 70 %)

The core-CSS areas the T1 bar names — `css/CSS2/normal-flow/`, `css/css-box/`,
`css/css-color/`, `css/css-fonts/`, `css/css-text/`.

**Why these are computed-value cases and not the raw upstream files:** the raw
upstream WPT tests for these areas are **testharness.js** (they need a JS runtime to
run their assertions — JS is T3, not T1) or **reftests** (they need a
reference-browser pixel diff — werust has no reference renderer at T1). Running the
raw files at T1 would mean either fabricating pass/fail (dishonest) or standing up a
JS engine / reference browser (out of T1 scope, non-hermetic). So the pinned set is
a **computed-value subset** MODELLED on those five areas: each case pins a small
fragment + author CSS and asserts the value the native cascade
(`Stylesheet::parse` + `cascade` + `ComputedStyle`) resolves — the exact surface the
core-CSS engine exposes at T1. This measures the cascade's conformance on the named
areas objectively and hermetically. The decision (and the rejected alternatives) is
recorded in `docs/spikes/t1-wpt-subset-regression-meter/README.md` (decision D2).

- **Format + property vocabulary:** documented at the top of `cases.txt` and in
  `crates/native-renderer/src/wpt_meter/core_css.rs`.
- **Pinning:** authored + frozen 2026-07-22 to exercise each of the five areas
  (colour parsing + inheritance, `em`/`%` font-size resolution, font weight/style,
  the box-model shorthands + longhands, `display`/normal-flow defaults + combinator
  specificity, and `text-decoration` / `line-height`). One case
  (`line-height-unitless-inherits-as-multiplier`) documents a KNOWN cascade defect
  (unitless `line-height` inherits as resolved px, not the multiplier — see
  `work/notes/observations/t1-unitless-line-height-inherits-as-absolute-px.md`); it
  legitimately FAILS the meter today, which is the point: the WPT bar is meant to
  EXPOSE such regressions, not tune them away. The meter is well above its 70 % floor
  with that failure counted honestly.

### Re-pinning to the real upstream files

Once a JS/`ScriptEngine` path exists (T3) or a reference-render pixel path lands, the
core-CSS half can be re-pinned to run the raw upstream testharness/reftest files. The
runner (`core_css.rs`) is the swap point; the threshold + area names stay as the T1
bar defines them.
