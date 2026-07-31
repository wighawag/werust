---
title: review-gate non-blocking nits for 'macos-harness-guard-teeth-and-paint-path-residue' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: macos-harness-guard-teeth-and-paint-path-residue
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'macos-harness-guard-teeth-and-paint-path-residue' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify decision 1: the allow test now also asserts the assembled scratch workspace contains no dangling symlink, which the task did not ask for. It is the assertion that makes the corrected prose honest (the stubbed cargo would otherwise hide exactly the desktop-paint breakage), and it only touches the test, but it does add a new red the gate can produce for harness maintenance.
  (crates/macos-renderer/tests/typecheck_harness_guard.rs dangling_symlinks + the final assert; docs/spikes/macos-harness-guard-teeth-and-paint-path-residue/DECISIONS.md section 1)
- Ratify decision 5: on a host where no candidate location is outside a temp root, the refusal test now prints a note to stderr and returns instead of standing in for the allow assertion. That is what the acceptance criterion about test independence asked for, but it does mean the refusal half is silently unexercised on such a host (and the note is invisible without --nocapture).
  (typecheck_harness_guard.rs, the let Some(victim) else arm)
- Ratify decision 4: the criterion says no committed doc, comment or script names crates/werust-macos/src/paint.rs, but five mentions remain by design (two historical records in one-derivation DECISIONS, annotated by a dated path note; the windows_window_shape.rs assertion that the path does NOT exist; two doc comments that narrate the extraction). The spirit is met; the literal wording is not.
  (docs/spikes/macos-harness-guard-teeth-and-paint-path-residue/DECISIONS.md section 4)
- The sweep missed a stale relative pointer: crates/werust-macos/tests/macos_window_shape.rs line 20 tells the reader that everything assembling a display value lives in src/paint.rs, and the comments around lines 206-208 say the same, but that crate's src/ now holds only lib.rs, main.rs and window.rs. It escaped because it names the bare file, not the full path the criterion greps for. Repoint or annotate in a follow-up?
  (crates/werust-macos/tests/macos_window_shape.rs:20 and :206-208 vs ls crates/werust-macos/src)
- Small overclaim in the corrected prose this task exists to make accurate: the spike README says every ordinary Ubuntu verify run EXECUTES the script twice, once with a SCRATCH_DIR outside every temp root which it must REFUSE. On a host where nothing qualifies (TMPDIR or HOME under a temp root, some containers) that first run does not happen at all. Worth a clause saying the refusal half needs a location outside every temp root.
  (docs/spikes/macos-wkwebview-renderer-backend/README.md, the local-type-check bullet)
- Side effect of the pid suffix: an abnormally terminated run (ctrl-C, gate timeout) now leaves a distinct hidden probe directory per run under the real HOME, where the previous fixed name was reused and overwritten. Cleanup is unconditional on the normal path, so this is only crash residue, but it accumulates rather than being self-limiting.
  (a_probe_dir_outside_every_temp_root builds the name with std::process::id())
- Ratify decision 2: the macOS PR-filter pin lives in crates/macos-renderer/tests/macos_backend_shape.rs and reads the workflow by whole list items, while the Windows sibling pin lives in crates/werust-core/tests/windows_renderer_leg_shape.rs and parses YAML with serde_yaml. The concept and const names match exactly, so the language is coherent, but the two pins now have different homes and different readers; the stated reason is avoiding a dev-dependency added for symmetry.
  (macos_backend_shape.rs PULL_REQUEST_FILTER + trigger_paths vs windows_renderer_leg_shape.rs)
