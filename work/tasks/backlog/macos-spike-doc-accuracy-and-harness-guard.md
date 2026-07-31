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
