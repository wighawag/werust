---
title: "Tidy the macOS engine's committed docs and put a guard on the type-check harness's `rm -rf`"
slug: macos-spike-doc-accuracy-and-harness-guard
blockedBy: []
covers: []
needsAnswers: true
---

## What to build

Three small, unrelated-to-each-other corrections found at Gate-2 of `macos-wkwebview-renderer-backend`. They are grouped only because each is a few lines in the same spike.

**0. THE HARNESS IS NOW BROKEN, fix it first of all** (planted by the conductor at Gate-3 of `windows-win32-window-and-chrome`, 2026-07-30 — this item did not exist when the task was written). That task extracted the shared painter into a new crate `crates/desktop-paint` and converted `werust-macos` to consume it, DELETING `crates/werust-macos/src/paint.rs` in the process. `typecheck-macos-from-linux.sh` still does `ln -sf $REPO/crates/werust-macos/src/paint.rs`, so the symlink now dangles, and its scratch-workspace `Cargo.toml` lists only `renderer`, `werust-core` and `macos-renderer`, so the real `pub use desktop_paint as paint;` in `crates/werust-macos/src/lib.rs` cannot resolve and the scratch `cargo clippy -p werust-macos` fails. The Windows sibling harness WAS updated in that change; this one was missed. Add the `desktop-paint` path dependency to the scratch manifest and drop or repoint the `paint.rs` symlink, then RUN the harness to prove it works rather than assuming. This matters out of proportion to its size: every line of Apple and Windows code in this repo is written blind from Linux, so this script is the only pre-CI feedback a macOS task has, and a dangling symlink turns that into a confusing error in the next agent's first five minutes.

**1. The `rm -rf` footgun (do this one after 0).** `docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh` runs `rm -rf` on a caller-supplied `SCRATCH_DIR` with no guard that the path is under a temp root. The default is safe, but an operator who exports `SCRATCH_DIR` to a working directory loses it. Add a guard: refuse unless the path is under a temp root (or is a directory the script itself created), and fail with a legible message rather than deleting. A committed harness that eats a directory on a typo is not a harness anyone should keep running.

**2. Claim versus reality in the README.** It says the leg runs on pull requests when the backend, the probe or the recorded verdict changes, but the workflow's `pull_request` path filter lists only the three crates and the workflow file. A PR that changes ONLY `docs/spikes/macos-wkwebview-renderer-backend/**` (for example, re-recording `expected.json`) does NOT trigger it. Either add the docs path to the filter or correct the sentence; do not leave the doc claiming a trigger that does not exist.

**3. Coverage locality.** README step 2 describes the five `webview-shared` tests as the lifecycle and off-thread-boundary tests, but `crates/webview-shared/src/lifecycle.rs` carries ZERO tests: the five are three `offthread` plus two `validate_url`. Either move the `LoadLifecycle` state-machine tests next to the code they cover, or correct the sentence. Prefer moving the tests if they exist elsewhere, since the point of the shared crate is that its guarantees travel with it.

**4. The README's Gatekeeper instructions are wrong on a current macOS** (planted by the conductor at Gate-3 of `macos-release-packaging-leg`, 2026-07-31). `README.md`'s new "The macOS release artifact (`Werust.app`, UNSIGNED)" section leads with right-click then Open, which **macOS 15 (Sequoia) REMOVED** as a Gatekeeper bypass for unsigned apps; the path there is System Settings -> Privacy & Security -> Open Anyway. The second option given, `xattr -d com.apple.quarantine`, still works everywhere, so nobody is stranded, but the FIRST bullet is the one most people will try and it will not work on a current OS. Lead with what works today, keep the older path labelled as such, and do not promise a flow the platform withdrew. Same class as items 2 and 3: a doc claiming behaviour the system does not have.

**Scope:** documentation accuracy and one shell guard. No behaviour change to the backend, the probe or the workflow's actual coverage beyond a possible path-filter addition.

## Acceptance criteria

- [ ] `typecheck-macos-from-linux.sh` WORKS again after the `desktop-paint` extraction (the scratch manifest carries the new path dependency, no symlink dangles), proven by running it, not by reading it.
- [ ] `typecheck-macos-from-linux.sh` refuses to `rm -rf` a `SCRATCH_DIR` outside a temp root, with a legible message; the safe default still works.
- [ ] The README's statement about when the leg runs matches the workflow's actual triggers (adjust one or the other, and say which you chose).
- [ ] The README's description of the `webview-shared` tests matches what that crate actually contains, or the tests move to match the description.
- [ ] The README's macOS open-it instructions work on a current macOS (Sequoia removed the right-click -> Open bypass for unsigned apps); the still-valid `xattr` path is kept.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Prompt

> Goal: three small corrections in the macOS engine spike. FIRST, guard the `rm -rf` in `docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh`: it deletes a caller-supplied `SCRATCH_DIR` with no check that the path is under a temp root, so an operator export to a working directory is destructive; refuse with a legible message instead. SECOND, the README claims the leg runs on PRs that change the recorded verdict, but the workflow's `pull_request` path filter covers only the three crates and the workflow file, so a docs-only PR does not trigger it: fix the filter or the sentence. THIRD, the README calls the five `webview-shared` tests the lifecycle and off-thread tests, but `lifecycle.rs` has none (they are three `offthread` + two `validate_url`): move the tests or correct the text, preferring to move them so the shared crate's guarantees travel with it. Docs accuracy plus one shell guard; no backend behaviour changes.

## Requeue 2026-07-31

CONDUCTOR HANDOFF (2026-07-31, drive-tasks). The acceptance gate went RED on the REBASED tip with exit 101, and the failure is NOT in the guard you wrote. It is in the TEST you wrote to prove it, which is environment-dependent. Your guard is fine. Do not weaken it.

THE FAILING TEST: `crates/macos-renderer/tests/typecheck_harness_guard.rs::the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root`

    thread '...' panicked at crates/macos-renderer/tests/typecheck_harness_guard.rs:60:5:
    the harness deleted a SCRATCH_DIR outside a temp root: it must refuse instead

DIAGNOSED, with both halves reproduced by the conductor on your exact branch tip (37ed77c):

- Clone the repo to `/tmp/werust-gate-check`, run the test: FAILS.
- Clone the SAME commit to `~/scratch-gate/werust-gate-check`, run the test: PASSES (2 passed).

THE CAUSE. The test builds its victim directory as `repo_root().join("target/typecheck-harness-guard-probe")` and comments that it "must NOT be under a temp root". That is an assumption about where the REPOSITORY lives, and it is false in exactly the environment that matters here: dorfl's `freshWorktreeGate` (ON by default) runs `prepare`+`verify` in a CLEAN THROWAWAY worktree, cut under a temp root. So `repo_root()` IS under a temp root there, the victim path is legitimately inside it, and your guard CORRECTLY allows the deletion — which the test then reads as the guard failing. That is why it passed in your build worktree (under `~/.dorfl/work/...`) and failed on the rebased tip. It would fail on any CI runner that checks out under a temp path too.

WHAT TO DO. Make the test's victim provably outside a temp root REGARDLESS of where the repo lives, so the assertion tests the GUARD and not the checkout location. Options, in the order I would try them:

1. Create the victim in a directory the TEST owns and controls, chosen to be outside any temp root (for example under `$HOME`, cleaned up unconditionally), rather than under `repo_root()/target`.
2. If, and only if, no such location can be guaranteed on every host, detect that `repo_root()` is itself under a temp root and assert the CONVERSE there (the harness must ALLOW that delete), so the test still has teeth in both environments instead of being skipped.

Do NOT fix this by loosening the guard to refuse deletions inside temp roots as well: the default `SCRATCH_DIR` lives under a temp root on purpose, and `the_harnesss_default_scratch_dir_stays_under_a_temp_root` (which passes) depends on that. Do NOT `#[ignore]` it either: a guard whose teeth are ignored is the footgun it was meant to close, back again.

While you are in there, ONE MORE THING the conductor will otherwise raise at Gate 3, so it is cheaper to settle now. Your change makes `macos-renderer.yml`'s `pull_request` filter DELIBERATELY IDENTICAL to its `push` filter, which widens it further (it now adds `crates/renderer/**`, `crates/fetcher/**` and both spike docs paths to the PR trigger). Item 2 of the task asked you to make the README and the workflow AGREE, and adding the docs path to the trigger is a legitimate way to do that — but the human has explicitly flagged the OPPOSITE concern about this very leg: that it already triggers on `crates/werust-core/**`, so most core work spends `macos-14` minutes and can be gated by a red macOS leg, and they are asking whether that is wanted at all. The Windows sibling deliberately went the other way (narrow PR filter, wide push filter, `workflow_dispatch` for the deliberate case) and PINS that choice in `crates/werust-core/tests/windows_renderer_leg_shape.rs`.

So: keep the docs paths on the PR trigger if you can justify them (re-recording a verdict IS the PR that most needs re-measuring, which is a good argument), but do NOT silently widen the leg further than item 2 requires. Adding `crates/renderer/**` and `crates/fetcher/**` to the PR filter is a separate decision that item 2 did not ask for. Either drop those two, or state the case for them explicitly in the workflow header the way the Windows leg states its trade-off. Whichever you choose, say so in the spike DECISIONS block.
