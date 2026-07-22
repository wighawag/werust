# Spike: the T1 WPT-subset regression meter

Durable evidence + decisions for task `t1-wpt-subset-regression-meter` (spec story
17, `docs/conformance-tiers.md` T1). This wires the T1 WPT-subset bar as the
**objective secondary regression meter** for the native T1 path: it runs the two
named subsets against the native path, produces a comparable-over-time pass-rate,
and enforces the thresholds (>= 90 % tree-construction, >= 70 % core-CSS) as a CI
guard. It is the SECONDARY meter, NOT the roadmap driver — the page checklists
(`t1-server-web-floor-*`) define "reached"; this catches regressions.

## What was built

In `crates/native-renderer/`:

- **`src/wpt_meter/` — the measurement engine** (public `wpt_meter` module):
  - `tree_construction.rs` runs the `html/syntax/parsing/` html5lib-derived
    tree-construction `.dat` cases against the native T1 parser
    (`Html5everParser::parse`), serialising the resulting render `Dom` in the
    html5lib `#document` format and comparing to the expected tree (normalised for
    the doctype/comments werust's static tree deliberately drops).
  - `core_css.rs` runs pinned computed-value cases for the five core-CSS areas
    (`css/CSS2/normal-flow/`, `css/css-box/`, `css/css-color/`, `css/css-fonts/`,
    `css/css-text/`) against the native cascade surface (`Stylesheet::parse` +
    `cascade` + `ComputedStyle`).
  - `mod.rs` defines `MeterReport` (the pass-rate + failures, the comparable
    number) and re-exports `run_tree_construction` / `run_core_css`.
- **`tests/fixtures/t1-wpt/` — the pinned subsets** (provenance in `SOURCE.md`):
  `tree-construction/*.dat` (upstream `.dat` format) and `core-css/cases.txt`
  (computed-value cases modelled on the five areas).
- **`tests/t1_wpt_subset_meter.rs` — the enforcement + CI guard:** runs both
  subsets, ENFORCES the thresholds (a drop below fails the test, hence `verify`,
  hence CI), guards that every named area is represented and that no
  bidi/complex-script area leaks into the T1 bar, and prints the comparable number.

## Reproducing

```sh
cargo test -p native-renderer --test t1_wpt_subset_meter -- --nocapture
```

The `--nocapture` prints the comparable number, e.g.:

```
[T1 WPT meter] tree-construction: 17/17 = 100.0% (floor 90%)
[T1 WPT meter] core-CSS: 26/27 = 96.3% (floor 70%)
```

The one core-CSS failure is the KNOWN unitless-`line-height` inheritance defect
(see the note linked below) — counted honestly, and well above the 70 % floor.

## How this satisfies "runnable in CI + enforces the thresholds"

The repo's `verify` gate is `cargo fmt --check && cargo clippy && cargo build &&
cargo test`, and `.github/workflows/verify.yml` runs exactly that on every push/PR.
The meter is a normal `cargo test` integration test, so it runs under `verify`
already — no new CI job or workflow edit is needed. A regression that drops either
subset below its floor turns the meter's assertion red, which fails `cargo test`,
which fails `verify` on the PR. That IS the CI regression guard the task asks for
(the `verify` gate is the single acceptance bar in this repo; see `CONTEXT.md`).

## Decisions

### D1 — Ship the meter as a `cargo test` integration test, not a separate binary / CI job

The task says "runnable in CI" and "enforce the thresholds". The repo already runs
`cargo test` as its whole `verify` gate in CI (`dorfl.json`,
`.github/workflows/verify.yml`), and the sibling floor tasks
(`t1-server-web-floor-article-and-blog`) already ship their objective regression
guard as a `cargo test` golden test. So the meter follows that established house
pattern: an integration test that computes the pass-rate and asserts the threshold.

- **Alternative considered (rejected):** a standalone `cargo run --bin wpt-meter`
  plus a new CI workflow step. Rejected as redundant — it would DUPLICATE the CI
  entry point (`cargo test`) the repo already gates on, add a second thing to keep
  green, and diverge from the sibling floor tasks' pattern. The measurement engine
  is still a public `wpt_meter` module, so a future benchmark harness
  (`native-renderer-benchmark-harness-capability-and-trust-hooks`, which scores a
  candidate path on "page checklists + WPT subsets") can call `run_tree_construction`
  / `run_core_css` directly without going through the test.
- **Touches:** `native-renderer-benchmark-harness-capability-and-trust-hooks`
  consumes the same public `wpt_meter` surface. No new flag/command/CI-config.

### D2 — Pin LOCAL fixtures; core-CSS is a computed-value subset, not the raw upstream reftests

The meter runs hermetically under `verify` (offline, no reference browser, no JS
engine — that is T3). The two subsets differ in how runnable their upstream form is:

- **Tree-construction IS runnable as-is:** the html5lib `.dat` format is
  self-contained plain text asserting the parse tree (no JS, no pixels). The pinned
  `.dat` cases use the EXACT upstream format, so the runner already parses the real
  corpus; re-pinning to the vendored upstream files is a fixture swap, not a code
  change (see `SOURCE.md`).
- **Core-CSS is NOT runnable as-is at T1:** the raw upstream `css/*` files are
  `testharness.js` (need a JS runtime — T3) or reftests (need a reference-browser
  pixel diff — werust has no reference renderer at T1). Running them raw would mean
  fabricating results or standing up a JS engine / reference browser, both out of T1
  scope and non-hermetic. So the core-CSS half is a **computed-value subset**: each
  case pins a fragment + author CSS and asserts the value the native cascade
  resolves — the exact core-CSS surface T1 exposes. This measures the named areas
  objectively and hermetically.
- **Alternative considered (rejected):** vendor the raw upstream `css/*` tests and
  run them through a headless testharness/reftest harness. Rejected for T1: it needs
  a JS engine (T3) or a reference browser (neither exists here), and would make the
  meter non-hermetic under `verify`. Recorded here (linked from the done record) so
  a reviewer expecting the raw WPT reftest corpus is not surprised: at T1 the
  core-CSS bar measures the cascade's computed values, and re-pins to the raw files
  when a JS/reference-render path lands (the runner is the swap point).
- **Coherence:** `docs/conformance-tiers.md` already frames the WPT bar as the
  "objective secondary regression meter … NOT how we decide what to build next", and
  the parser/core-CSS tasks' forward-notes name `Html5everParser::parse` and
  `Stylesheet::parse`+`cascade`+`ComputedStyle` as the surfaces "a WPT harness
  drives". This meter drives exactly those, so it realises the pinned concept at the
  layer T1 supports, without re-meaning "WPT bar".

### D3 — The bar counts a KNOWN defect honestly rather than excluding it

The core-CSS set includes `line-height-unitless-inherits-as-multiplier`, which fails
today because the landed cascade inherits a unitless `line-height` as resolved px,
not the multiplier (`work/notes/observations/t1-unitless-line-height-inherits-as-absolute-px.md`,
an orphaned defect flagged by the t1-server-floor gate). It is left IN the set and
counted as a failure: the WPT bar exists to EXPOSE such regressions objectively, so
hiding it would defeat the meter. The rate stays well above the 70 % floor with it
counted, and when the defect is fixed the meter ticks up — exactly the
comparable-over-time signal the bar is for.

## Cross-references

- `docs/conformance-tiers.md` — T1 bar definition (the thresholds + the named
  subsets, and the "objective secondary meter, not the roadmap" framing).
- `crates/native-renderer/tests/fixtures/t1-wpt/SOURCE.md` — fixture provenance +
  re-pinning instructions.
- `work/notes/observations/t1-unitless-line-height-inherits-as-absolute-px.md` — the
  known defect the core-CSS bar counts honestly (D3).
- The consumers: `native-renderer-benchmark-harness-capability-and-trust-hooks`
  (scores a candidate path on page checklists + WPT subsets via the public
  `wpt_meter` surface).
