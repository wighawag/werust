# The macOS and Windows CI legs never run clippy (the platform halves are linted only by the two hand-run harnesses)

2026-07-31, noticed while building `verify-lints-test-targets-and-clears-the-existing-debt`. Corrected the same day after review: the first version of this note claimed the platform halves were unlinted EVERYWHERE, which is false (see below).

`.github/workflows/macos-renderer.yml` and `.github/workflows/windows-renderer.yml` build, test and RUN the platform crates on native runners but have no clippy step. The only clippy invocations in CI are `verify.yml` and `release.yml`, both Ubuntu, and `verify` now lints all targets with `-D warnings` for the crates the Ubuntu gate can build. So no GATE anywhere lints the ~7.5k lines behind `#[cfg(target_os = "macos")]` / `#[cfg(windows)]` (the two WebView backends, both native windows, both origin probes, and the four load-bearing example smokes).

They are not unlinted, though. Two cross-target developer harnesses already run clippy over most of that code from Linux:

- `docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh` ends in `cargo xwin clippy -p windows-renderer -p werust-windows --target x86_64-pc-windows-msvc --tests --examples`, and its header says it runs clippy rather than a bare `check` deliberately.
- `docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh` runs three clippy invocations against `aarch64-apple-darwin`: the engine (`--all-targets`, in a scratch workspace against stand-in `werust-core`/`fetcher`), `-p werust-macos --lib --examples`, and `-p macos-origin-probe --all-targets` in the repo.

Both are DEVELOPER TOOLS a human runs by hand, not gates, and they run at a LOWER bar than `verify` now does: the Windows one uses `--tests --examples` rather than `--all-targets`, `werust-macos` is only `--lib --examples` (its bin arm and its unit tests are excluded on purpose, the latter because the stand-in core cannot judge them), and none of the four invocations passes `-D warnings`, so clippy prints and still exits 0. `crates/windows-origin-probe`'s `#[cfg(windows)]` half and `werust-android`'s `jni_exports` are in no harness at all.

Two levers, in rising cost, both out of scope for the task that noticed this (its scope was the Ubuntu gate):

1. Raise the two harnesses to the gate's bar (`--all-targets` plus `-D warnings`). Measured on 2026-07-31: three of the four legs are ALREADY clean at that bar (Windows both crates, the macOS engine, `macos-origin-probe`), so the change is one flag per invocation plus whatever the fourth leg turns up. The fourth (`-p werust-macos`) could not be measured because the harness's stand-in core has drifted and no longer compiles: `work/notes/observations/macos-typecheck-harness-standin-core-drifted-2026-07-31.md`.
2. Add `cargo clippy --all-targets -- -D warnings` (scoped with the same `-p` sets those legs already use) to the two platform workflows. That is what makes it a GATE rather than a habit, needs no cross-target trick since the platform half already compiles there, and is the only way `windows-origin-probe`'s Win32 half and the smokes' real bodies get covered.

Inventory and the full covers/cannot-cover breakdown: `docs/spikes/verify-lints-test-targets-and-clears-the-existing-debt/README.md`.
