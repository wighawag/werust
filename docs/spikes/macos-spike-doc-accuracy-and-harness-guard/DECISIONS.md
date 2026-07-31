# Judgement calls made tidying the macOS spike's docs and harness

The choices in `macos-spike-doc-accuracy-and-harness-guard` that another task, a user or a reviewer could be surprised were decided here. Each says what was chosen, why, what else was considered, and what it touches. The task was "docs accuracy plus one shell guard", so anything below that is more than a wording fix is here on purpose.

## 1. The harness's refusal: strictly BELOW a temp root, and nowhere else

**Chosen:** `typecheck-macos-from-linux.sh` resolves `SCRATCH_DIR` to an absolute, symlink-free path and deletes it only when that path is strictly below one of `$TMPDIR`, `/tmp` or `/var/tmp`. Anything else prints a message naming the path and the allowed roots, and exits 1.

**Why:** the script rebuilds its scratch workspace from nothing on every run, so it must `rm -rf` first; the directory is caller-supplied, so a stale exported `SCRATCH_DIR` or a typo used to eat a working directory. A temp-root allowlist is the smallest rule that keeps the default working and makes the destructive case impossible. "Strictly below" matters: `SCRATCH_DIR=/tmp` is refused, because deleting a temp root itself is the same accident one level up. The path is resolved BEFORE the check (walking up to the deepest existing ancestor, since the directory need not exist yet), so `../..`-style relative paths and symlinked temp dirs cannot smuggle a working directory past a string comparison.

**Alternatives considered:** (a) a marker file the script writes and refuses to delete a directory without — safer still, but it cannot protect the FIRST run against a hostile `SCRATCH_DIR`, which is the actual reported footgun; (b) prompting for confirmation — this script is meant to be runnable in a loop and from a pre-push hook, so a prompt is a regression; (c) never deleting, only `mkdir -p` over the top — stale symlinks from a previous layout are exactly what broke this harness in the first place (see 3), so a rebuild-from-nothing is the property worth keeping.

**What it touches:** it introduces a NEW REFUSAL, i.e. a user-visible error that did not exist before. An operator who deliberately pointed `SCRATCH_DIR` at, say, a fast scratch disk outside `/tmp` will now be refused and must either unset it or point it under a temp root. That trade was made knowingly: the harness is a developer convenience, the directory it deletes is disposable by construction, and no CI leg sets `SCRATCH_DIR`. The refusal is exercised by `crates/macos-renderer/tests/typecheck_harness_guard.rs` on the ordinary Ubuntu gate.

## 2. The `pull_request` path filter was made IDENTICAL to the `push` filter

**Chosen:** in `.github/workflows/macos-renderer.yml`, the `pull_request` filter now lists exactly what the `push` filter lists — which adds `crates/renderer/**`, `crates/fetcher/**` and both `docs/spikes/macos-*/**` directories.

**Why:** the task asked only that the doc's claim ("it runs on pull requests when the backend, the probe or the recorded verdict changes") stop being false, which the docs path alone would have fixed. But the two filters diverging is its own latent trap: any path in `push` but not in `pull_request` is a change that merges green and only then reds `main`, which is the worst place to learn that WebKit moved under the recorded verdict. Keeping one list, duplicated (GitHub Actions has no YAML anchors) with a comment saying they are deliberately identical, removes the whole class.

**Alternatives considered:** correcting the README sentence instead — rejected, because the recorded verdict genuinely is the thing most worth re-measuring on a PR: re-recording `expected.json` after a deliberate re-decision is exactly the change that must be checked against a real WebKit before it lands.

**What it touches:** CI trigger surface. A PR touching only `crates/fetcher` or `crates/renderer` now also runs the `macos-14` leg (it already did on `main`). That is more runner minutes on a shared, already-used runner, for a leg that is decoupled from the ordinary `verify` gate. The claim/trigger agreement is pinned by `the_readme_claim_about_when_the_leg_runs_matches_the_pull_request_trigger` in `crates/macos-renderer/tests/macos_backend_shape.rs`, so the doc and the YAML cannot drift apart again silently.

## 3. The harness checks `desktop-paint`'s REAL source, re-pointed at the stand-in core

**Chosen:** the scratch workspace gained a `fake-paint` member whose `Cargo.toml` is a stand-in (it names the package `desktop-paint` and points `werust-core` at `fake-core`) but whose `src` is a symlink to the REAL `crates/desktop-paint/src`. The dangling `paint.rs` symlink was dropped, since `windows-win32-window-and-chrome` deleted that file.

**Why:** `crates/desktop-paint` depends on the real `werust-core`, which reaches `fetcher -> ureq -> rustls -> ring`, whose build script cannot cross-compile to an Apple target from Linux — the single obstacle this whole harness is built around. Depending on the repo crate directly would therefore have re-broken the check it was fixing. `webview-shared` was already handled exactly this way, so the painter's shared carrier now gets the painter's version of the same treatment, and the window is type-checked against the REAL painter source rather than a hand-written fake of it.

**What it touches:** the stand-in `werust-core` in that script must now carry the core API `desktop-paint` uses as well as the API the window uses; this change added `load_progress_tooltip` and `STOP_AFFORDANCE_LABEL`. That is the known, accepted maintenance cost of the stand-in (`macos-wkwebview-renderer-backend`'s DECISIONS.md, choice 5's spirit): the harness reds legibly when the core moves, which is the point. Proven by RUNNING it, not by reading it — the run is recorded in `docs/spikes/macos-wkwebview-renderer-backend/README.md`.

## 4. Four `LoadLifecycle` tests MOVED crates, rather than the sentence being corrected

**Chosen:** the four pure state-machine tests (`a_same_document_url_change_...`, `an_ens_resolved_load_reports_...`, `the_ens_origin_flag_redirects_...`, `an_ens_flagged_load_that_fails_verification_...`) moved verbatim from `crates/webview-renderer/src/lib.rs` into `crates/webview-shared/src/lifecycle.rs`. `webview-shared` goes from 5 tests to 9.

**Why:** the task named this preference and the reason is real. `LoadLifecycle` MOVED to the shared crate; its tests did not, so the shared crate's central guarantee was exercised only inside the GTK-bound crate that cannot compile anywhere but Linux — and the macOS leg's `cargo test -p webview-shared` step, which exists to prove the moved code passes on the other desktop platform, covered the off-thread boundary and the URL rule and NOTHING about the lifecycle. Moving them makes that step honest and gives the future WebView2/other consumers the guarantees with the code.

**What it touches:** `cargo test -p webview-renderer` no longer runs those four (it keeps every test that is about the GTK backend, the seam harness and the trust hooks); `cargo test -p webview-shared`, including on the `macos-14` runner, now does. No test was rewritten, weakened or deleted: only the wording of comments that named GTK-only signals was generalised to name both platforms' equivalents.
