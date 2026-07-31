# Decisions — harness-guard teeth on both branches, and the `paint.rs` residue

Task: `macos-harness-guard-teeth-and-paint-path-residue` (four residues of `macos-spike-doc-accuracy-and-harness-guard`, plus the PR-filter narrowing the conductor added at Gate-3). Parent decisions: [`../macos-spike-doc-accuracy-and-harness-guard/DECISIONS.md`](../macos-spike-doc-accuracy-and-harness-guard/DECISIONS.md).

The task was "test rigour plus doc accuracy, no behaviour change to the harness", so anything below that is more than a wording fix or the literal instruction is recorded here to be ratified or reversed.

## 1. The ALLOW test also asserts the assembled workspace has NO DANGLING SYMLINK

**Chosen:** `the_harness_deletes_a_scratch_dir_under_a_temp_root` walks the scratch workspace the harness just built and fails if any symlink in it points at something that does not exist. Not asked for by the task; added inside it.

**Why:** without it, the corrected prose (item 2) could not honestly say the gate would have caught the breakage the task keeps citing. `rustup` and `cargo` are STUBBED for this run, and `ln -s` is perfectly happy to point at a deleted file, so the exact failure that broke this harness — `windows-win32-window-and-chrome` deleting `crates/werust-macos/src/paint.rs` while the harness went on symlinking it — reds only at `cargo check` time, which the stub never reaches. Executing the script with a stub proves it ASSEMBLES; asserting the assembly RESOLVES is what makes "the gate runs the harness" evidence about the thing that actually broke. Verified by re-pointing the `fake-paint` symlink at the deleted path: the test reds naming the link.

**Alternatives considered:** (a) leave it out and weaken the README sentence to "the gate runs the prologue" — honest, but it leaves the parent's "prove it by running it" criterion backed by a local run nobody re-checks, which is what this task exists to close; (b) drop the stubs and run the real cross-target `cargo clippy` on the gate — that is minutes per run plus an installed `aarch64-apple-darwin` std, which is exactly why the harness is a LOCAL loop and the `macos-14` leg is the verdict.

**What it touches:** the harness's own maintenance. A future edit that adds a symlink to a source which is later moved now reds the ordinary Ubuntu gate rather than the next macOS agent's afternoon. It is not a harness behaviour change: the script is unmodified apart from one comment.

## 2. The macOS PR-filter pin lives in `macos_backend_shape.rs`, parsed by LINE, not by `serde_yaml`

**Chosen:** the exact-set pin (`PULL_REQUEST_FILTER` + `PUSH_ONLY_DEPENDENCY_SURFACE` + `the_pull_request_filter_is_the_pinned_exact_set_and_push_carries_the_rest`) went into `crates/macos-renderer/tests/macos_backend_shape.rs`, beside the macOS leg's existing README-claim pin, and reads the `paths:` entries as whole list ITEMS off the workflow text instead of parsing the file as YAML.

**Why:** the task said to follow `crates/werust-core/tests/windows_renderer_leg_shape.rs`'s SHAPE and not to invent a second one. Its shape is the named exact-set const plus the "push carries what the PR filter gives up" pair; its LOCATION and its `serde_yaml` parse are incidental to the crate it happens to live in (`werust-core` already had `serde_yaml` as a dev-dependency; `macos-renderer` does not, and adding it to satisfy a comment would be a new dependency for style). The macOS leg already pins a claim about this filter in `macos_backend_shape.rs`, and splitting one filter's pins across two crates is how a reader ends up believing there is only one. Item-wise line parsing is what the pre-existing assertion in that file already did, and it is deliberately not substring matching: both filters here sit next to prose comments that NAME paths deliberately kept OFF them.

**Alternatives considered:** (a) a new `crates/werust-core/tests/macos_renderer_leg_shape.rs` mirroring the Windows file — rejected as two homes for one filter's pins, and it would leave `macos_backend_shape.rs`'s comment about a widening needing a test edit pointing at a test in another crate; (b) adding `serde_yaml` to `macos-renderer`'s dev-dependencies to parse identically — rejected as a dependency added for symmetry, not for a check the line parse cannot do.

**What it touches:** every macOS-shaped pull request, and any later task that wants to widen this leg's trigger: it must now edit the const AND the workflow header. It also makes the existing comment in `macos_backend_shape.rs` about widening requiring a test edit TRUE rather than aspirational.

## 3. `crates/werust-core/**` off the PR filter is a CI-cost decision, ratified before it was built

**Chosen:** the macOS leg's `pull_request` filter no longer carries `crates/werust-core/**`; it stays on `push` to `main`, with `workflow_dispatch` for the deliberate case.

**Why:** it is item 5 of the task body, added by the conductor and ratified by the human in the same drive. It is recorded here anyway because it is the one change in this task a user can FEEL: a core PR that breaks macOS is now found minutes after it merges rather than before. The parent task parked this question on purpose ("answering it is a decision of its own, not a side effect of a docs fix"), the Windows sibling had already refused the same cost, and two desktop legs answering the same question differently is a difference nobody can act on.

**What it touches:** `.github/workflows/windows-renderer.yml`'s header and `crates/werust-core/tests/windows_renderer_leg_shape.rs`'s comments both described the macOS leg as the counter-example under review; that prose was false the moment this landed, so it was corrected in the same change (as was `docs/spikes/windows-renderer-ci-leg/README.md`'s D1, annotated rather than rewritten — see 4).

## 4. Historical records were ANNOTATED, not rewritten, so three mentions of the deleted `paint.rs` remain

**Chosen:** the live pointers to `crates/werust-macos/src/paint.rs` were repointed at `crates/desktop-paint` (`.github/workflows/macos-renderer.yml`'s header, `docs/spikes/macos-appkit-window-and-chrome/README.md` in three places, the harness's own comment). Three mentions were deliberately LEFT:

- `docs/spikes/one-derivation-close-the-aggregate-and-tooltip-gaps/DECISIONS.md` (two) — a recorded decision and a verbatim record of what a gate PRINTED when a fourth class family was trialled. A dated **Path note** at the top of that file repoints the reader; the record below it is left as it was written.
- `crates/werust-windows/tests/windows_window_shape.rs` — asserts the path does NOT exist. Rewriting it would delete the guard that the extraction left no copy behind.

Two further mentions (`crates/werust-macos/tests/macos_window_shape.rs`, `crates/werust-core/tests/chrome_css_class_set_edge_wiring_shape.rs`) are doc comments that narrate the extraction explicitly and already read `crates/desktop-paint`; they are accurate history, not stale pointers.

**Why:** the acceptance criterion reads "no committed doc, comment or script names `crates/werust-macos/src/paint.rs`", and taken literally it would have me falsify a transcript and delete a working assertion. The criterion's PURPOSE is that no reader is sent to a file that is gone, which annotation satisfies without rewriting history. This is flagged rather than buried because it is a deliberate deviation from the literal wording.

**Alternatives considered:** a guard test asserting no file under `docs/` or `.github/` contains that path. Rejected: it would force the historical records to be rewritten to stay green, which is the cost above with an enforcement mechanism attached. The sweep is a one-off residue of one extraction, not a recurring class.

## 5. The refusal test SKIPS (loudly) on a host where nothing is outside a temp root

**Chosen:** where no candidate location qualifies, `the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root` prints a note and returns instead of standing in for the allow assertion, as it used to.

**Why:** the acceptance criterion is that the two tests must not depend on each other's environment, and on such a host there is genuinely no path to offer the harness that it is required to refuse. The parent's objection to skipping ("a guard whose teeth are quietly ignored is the footgun it was meant to close") was about the guard as a WHOLE going untested; that no longer applies, because the allow half now runs unconditionally on every host. The note goes to stderr so a run on such a host says which half it could not provoke.
