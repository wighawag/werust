# Spike: the native-renderer benchmark harness (capability + trust-hooks + vs-wezig)

Durable evidence + decisions for task
`native-renderer-benchmark-harness-capability-and-trust-hooks` (spec stories 20 + 21
of `rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack`;
`docs/conformance-tiers.md`; the exploration spec
`rust-successor-native-renderer-architecture-benchmark`, `docs/adr/0001`).

This is the EVIDENCE GENERATOR for the deferred native-renderer architecture
decision. It scores a candidate native-renderer path on THREE axes and emits ONE
structured, comparable, reproducible report the follow-on exploration spec consumes to
DECIDE the architecture. **It does NOT decide the architecture** — it lays the
candidates side by side on the same pinned ladder and lets the human-resolved decision
(open question 1 of the exploration spec) choose.

## What was built

In `crates/native-renderer/`:

- **`src/benchmark.rs` — the harness** (public `benchmark` module, re-exported at the
  crate root):
  - **Capability axis** (`CapabilityScore`): the pinned page checklist
    (`score_page_checklist` renders each pinned T1 page through the `Renderer` seam and
    marks it rendered iff it reached `Finished` with painted runs — the primary,
    human-legible capability driver) PLUS the WPT subsets (`run_wpt_subsets` reuses the
    `wpt_meter` engine: `html/syntax/parsing/` tree-construction and the five core-CSS
    areas), compared against the T1 bars (>= 90 % / >= 70 %).
  - **Trust-hook axis** (`TrustHookScore`): a PASS/FAIL qualification reusing the
    seam's own `renderer::qualify` gate (provider injection + `ipfs://` scheme), naming
    the missing hooks — NOT a graded score.
  - **vs-wezig meter** (`VsWezigMeter` / `ArmSignals`): the reversible experiment's
    measurement on the SHARED ladder — effort, code volume, and DOM object-graph
    friction, per arm, with `dom_friction_delta()` surfacing the central "does Rust
    drown in DOM object-graph friction?" signal.
  - **Candidates** (`Candidate`): the three paths the exploration spec compares —
    `OwnEngine`, `Servo`, `BlitzStyloAssembly` — each scored either `Measured` (a real
    backend driven through the ladder now) or `Declared` (an honest not-yet-built slot),
    tagged by `CandidateScoring` so measured evidence is never confused with a
    placeholder. `score_measured_candidate` / `declared_candidate` build the rows.
  - **Report** (`BenchmarkReport`): every candidate row on the same axes, with
    `to_json()` serialising it in a stable, diffable shape (hand-serialised, no serde
    dependency) so a captured run is byte-stable committed evidence.
- **`tests/fixtures/benchmark/`** — the pinned inputs (provenance in `SOURCE.md`):
  `vs-wezig.txt` (the recorded per-arm effort/code-volume/friction signals). The page
  checklist reuses the committed `tests/fixtures/t1-server-floor/` snapshots and the WPT
  subsets reuse `tests/fixtures/t1-wpt/` — one source of truth, not re-pinned copies.
- **`tests/benchmark_harness.rs`** — the end-to-end acceptance test: runs the whole
  harness against the pinned ladder and asserts all four acceptance criteria, plus a
  `prints_the_report_for_capture` helper that emits the report for capture.

## The captured report

`report.json` in this directory is a captured run of the harness (regenerate with the
reproducing command below). Today the assembled pure-Rust stack behind `NativeRenderer`
(the Blitz/Stylo-assembly class) is the one MEASURED candidate: it renders both pinned
T1 pages, meets both WPT bars (tree-construction 100 %, core-CSS 96.3 %), and does NOT
yet qualify on the trust hooks (the T1 native backend honestly declares neither hook
yet — that is the provider/ipfs tasks' wiring). Own-engine and Servo are DECLARED slots
the exploration fills in when those paths are prototyped.

## Reproducing

```sh
cargo test -p native-renderer --test benchmark_harness -- --nocapture
```

Regenerate the captured `report.json`:

```sh
cargo test -p native-renderer --test benchmark_harness \
    prints_the_report_for_capture -- --nocapture \
  | sed -n '/^{/,/^}/p' \
  > docs/spikes/native-renderer-benchmark-harness-capability-and-trust-hooks/report.json
```

The report is deterministic: two runs are byte-equal (asserted by
`harness_is_re_runnable_and_its_report_is_reproducible`).

## How this satisfies "re-runnable + reproducible + tests cover the scoring logic"

The repo's `verify` gate is `cargo fmt --check && cargo clippy && cargo build && cargo
test`, run by `.github/workflows/verify.yml` on every push/PR. The harness ships as a
public library module (unit-tested in `src/benchmark.rs`) plus a `cargo test`
integration test (`tests/benchmark_harness.rs`), so it runs under `verify` already — no
new binary or CI job. The scoring logic (page-checklist gate, WPT-bar comparison, the
trust-hook pass/fail, the vs-wezig delta, the stable JSON) is covered by the module unit
tests and the end-to-end acceptance test.

## Decisions

### D1 — The harness is a library module + a `cargo test` acceptance test, not a standalone binary / new CI job

The task asks for a "re-runnable harness" whose "scores are reproducible; tests cover
the scoring logic". The repo already runs `cargo test` as its whole `verify` gate, and
the sibling `t1-wpt-subset-regression-meter` established the pattern (a public
measurement module + a `cargo test` integration test; its D1 explicitly forward-notes
THIS harness as the consumer of the public `wpt_meter` surface). So the harness follows
that house pattern.

- **Alternative considered (rejected):** a standalone `cargo run --bin benchmark` plus a
  new CI workflow step. Rejected as redundant — it would duplicate the CI entry point
  (`cargo test`) the repo already gates on, add a second thing to keep green, and
  diverge from the sibling meter's pattern. The report is still emitted for capture
  (`prints_the_report_for_capture -- --nocapture`) so a run can be committed as evidence
  (`report.json`), and the scoring is a public library surface any future driver can
  call.
- **Touches:** reuses `wpt_meter` (capability WPT) and `renderer::qualify` (trust hooks)
  — no new flag/command/CI-config. The exploration spec
  (`rust-successor-native-renderer-architecture-benchmark`) consumes this module's
  `BenchmarkReport` / `Candidate` surface.

### D2 — A candidate is scored MEASURED or DECLARED; the vs-wezig arm signals are pinned recorded evidence, not computed

The exploration spec names three candidate architectures (own-engine, Servo,
Blitz/Stylo assembly) but only the assembled pure-Rust stack is built in this repo
today. The harness must be HONEST and comparable about that: a `CandidateScoring`
(`Measured` vs `Declared`) tags every row, so a not-yet-built path is a visible slot the
exploration fills in — never fabricated zeros masquerading as measurement. Likewise the
vs-wezig meter STRUCTURES the comparison and reads pinned per-arm signals from
`tests/fixtures/benchmark/vs-wezig.txt`; it does NOT compute effort/code-volume/friction
from a source tree (wezig is a separate project; those figures are build-history
evidence). This is a user-visible modelling choice worth recording.

- **Alternative considered (rejected):** score only the one real candidate and omit the
  other two, or fabricate numbers for them. Rejected: the exploration spec compares
  THREE candidates on the SAME axes (its story 1), so a report that silently dropped two
  — or invented data for them — would let the architecture decision rest on fiction. A
  `Declared` slot says "not scored here" out loud.
- **Alternative considered (rejected):** compute/fetch the wezig-arm figures at test
  time. Rejected: it would make the meter non-hermetic under `verify` and
  non-reproducible (wezig source is not in this repo). Pinning as recorded evidence
  matches how the T1 page snapshots and WPT subsets are already pinned; updating a figure
  is a fixture edit, not a code change. Provenance/status of the pinned figures is in
  `tests/fixtures/benchmark/SOURCE.md`.
- **Touches:** the exploration spec's story 3 (fold the T1-climb effort/volume/friction
  vs wezig into the decision) reads this meter. No other command/flag.

## Coherence check

The new concepts reuse the project's existing language and sit at the right layer, so no
existing term is re-meant:

- **capability**, **trust hooks**, **vs-wezig meter**, **shared conformance ladder** are
  the spec's/`CONTEXT.md`'s own words for exactly these axes; the harness realises them.
- **`TrustHookScore`** reuses `renderer::qualify` / `TrustHooks` (the trust-hook
  qualification gate task) verbatim as a pass/fail — it does not fork a second gate. The
  gate task's own README forward-notes THIS harness as reusing `qualify` / `trust_hooks`.
- **Capability WPT** reuses `wpt_meter::{run_tree_construction, run_core_css}` — the same
  engine the ladder's regression meter uses — so capability numbers are comparable across
  candidates and against wezig.
- **`Candidate`** names exactly the exploration spec's three architectures; **`Measured`
  / `Declared`** is a new distinction, but it is a scoring-provenance tag scoped to the
  report (it does not re-mean the ladder's "reached" or the gate's "qualifies").
- The harness deliberately has NO "winner" / "decision" field: deciding is the
  exploration spec's human-resolved job (`docs/adr/0001`, the exploration spec's open
  question 1), and this harness only MEASURES.

## Cross-references

- `docs/conformance-tiers.md` — the pinned page checklists + WPT bars the capability axis
  scores against.
- `docs/adr/0001-general-browser-for-a-post-trusted-server-web.md` — the thesis + the
  reversible experiment the vs-wezig meter measures.
- `crates/renderer/src/lib.rs` + `docs/spikes/renderer-seam-trust-hook-qualification-gate/`
  — the `qualify` gate the trust-hook axis reuses.
- `crates/native-renderer/src/wpt_meter.rs` +
  `docs/spikes/t1-wpt-subset-regression-meter/` — the WPT engine the capability axis
  reuses.
- `crates/native-renderer/tests/fixtures/benchmark/SOURCE.md` — the pinned vs-wezig
  fixture's provenance.
- The consumer: `rust-successor-native-renderer-architecture-benchmark` (the exploration
  spec that decides the architecture FROM this report).
