# The Rust toolchain pin, and the 1.97 clippy lint that made it urgent

Task: `pin-the-rust-toolchain-and-fix-the-197-clippy-red-on-main`. Written on 2026-07-31, the day `main` went red.

## The incident

[Run 30622910777](https://github.com/wighawag/werust/actions/runs/30622910777) failed `verify` and took every downstream release job (`goreleaser`, `android-apk`, `ios-simulator-app`, `macos-desktop-app`, `windows-desktop-app`) to `skipped`:

```
error: this block may be rewritten with the `?` operator
   --> crates/native-renderer/src/tokenizer.rs:377:20
    = note: `-D clippy::question-mark` implied by `-D warnings`
    = help: for further information visit .../rust-clippy/rust-1.97.0/index.html#question_mark
```

The lint is correct and the code really was needlessly long. The interesting part is not the lint, it is that **the acceptance gate could not see it**: CI installed clippy with a bare `rustup component add rustfmt clippy` and got whatever the runner shipped (**1.97.0**), while this development machine, and therefore every local `dorfl` run of `verify` (this repo's actual pass/fail bar), was on **1.91.1**. `clippy::question_mark` does not fire on 1.91 for that code. Two runners of the same `-D warnings` gate, six minor versions apart: the local one waves changes through and `main` catches them afterwards. That is a gate that cannot decide, which is the defect this task fixes.

This was predicted. `verify-lints-test-targets-and-clears-the-existing-debt` landed `-D warnings` on an unpinned toolchain and [recorded the exposure](../verify-lints-test-targets-and-clears-the-existing-debt/README.md) ("the toolchain is not pinned … so a new Rust release that adds a lint can red the gate for a task that did not cause it"). It was realised within the hour.

## What landed

1. **The lint, fixed with `?` and not with an `#[allow]`** (`crates/native-renderer/src/tokenizer.rs`). The `else if let Some(dec) = other.strip_prefix('#') { … } else { return None }` tail became `let dec = other.strip_prefix('#')?;` inside a plain `else`. A characterization test written BEFORE the rewrite (`decodes_numeric_entities_and_leaves_the_rest_verbatim`) pins all four branches the block distinguishes: `&#x41;`, `&#X42;`, `&#67;`, an unknown NAME (`&copy;`) and an unparseable decimal (`&#zz;`), the last two passing through verbatim. The rewrite is therefore provably a simplification and not a behaviour change.

2. **`rust-toolchain.toml` at the workspace root**, pinning `channel = "1.97.0"` plus the `rustfmt` and `clippy` components and `profile = "minimal"`.

3. **No workflow selects a toolchain any more.** `verify.yml` and `release.yml`'s `verify` job traded `rustup component add rustfmt clippy` for a step that only RESOLVES and RECORDS the toolchain (`rustup show active-toolchain`, `cargo --version`, `cargo clippy --version`, `cargo fmt --version`), so a run's log now carries the compiler that reached its verdict.

4. **A shape guard**, `crates/werust-core/tests/toolchain_pin_shape.rs`, in the same family as `verify_gate_shape.rs` / `release_plumbing_shape.rs`: the channel must be an exact `major.minor.patch`, the components must include the two the gate runs, no YAML under `.github/` may contain `rustup component add` / `rustup toolchain install` / `rustup default` / `rustup override` / `rustup update` / `cargo +…` or a setup-toolchain action, and this file must name the version actually pinned.

## Why 1.97.0

Because it is the bar `main` was already being judged against. Pinning DOWN to 1.91.1 (what this laptop had) would have turned CI green in one line, and was rejected: it makes the repo's standard "whatever the developer's machine happens to have", and it silently discards a lint that is right. Pinning to `stable` is not a pin at all: a floating channel is exactly what produced the failure, since two runners resolved it to two different compilers.

The expected cost was that 1.97's clippy, six releases newer, would surface more lints than 1.91 did across `--all-targets`. **Measured: it did not.** After the tokenizer fix, `cargo clippy --all-targets -- -D warnings` under 1.97.0 is clean across the whole workspace, and `cargo fmt --check` (rustfmt 1.9.0-stable) leaves the tree byte-identical. The `question_mark` lint was the only debt six minor versions had accumulated.

## Why the pin makes `-D warnings` safe

A bump is now a **deliberate, reviewable, one-file change**: edit `channel` in `rust-toolchain.toml`, run the gate, clear whatever the new clippy reports, land it together. Nothing about a Rust release day can red the gate for a task that did not cause it, on anyone's machine or on CI, until someone chooses to raise the pin.

That matters most on the leg that is easy to forget: **the same `-D warnings` gate runs inside `release.yml`, blocking the tag build**. Before this pin, a Rust release that added one lint could have failed the release of code nobody had touched, leaving no artifacts on the Release page and no obvious cause. The pin closes that too.

**Raise the pin when** someone wants a newer compiler or a newer dependency needs one, not on a schedule. One related constraint is now unblocked but deliberately NOT taken here: `docs/spikes/renderer-seam-trait-and-webview-backend-navigate/README.md` pins `gtk4 = "=0.10.0"` / `webkit6 = "=0.5.0"` because the `0.11`/`0.6` line needs rustc >= 1.92 "and this repo's toolchain is 1.91.x". That sentence is stale as of this change; whether to move up the gtk4-rs line is its own task with its own risk, and bundling it into a red-main fix would have hidden it.

## How the pin reaches every leg

`rustup` walks up from the current directory to find `rust-toolchain.toml`, and honours it for every proxied `cargo`/`rustc`/`clippy`/`rustfmt` call — laptop, `dorfl` gate and all five CI legs alike. Two behaviours were MEASURED on rustup 1.28.2 rather than assumed, because the whole design rests on them:

- a proxy call (`cargo --version`) in a directory pinning an **uninstalled** toolchain downloads it, including the `components` the file declares (observed installing `clippy` and `rustfmt` for a pinned 1.96.0);
- `rustup target add <triple>` does the same before adding the target.

That second one is what makes the cross-compiling legs correct with NO change: `macos-renderer.yml`, `windows-renderer.yml`, `windows-origin-probe.yml` and `mobile-ios.yml` never selected a toolchain in the first place (they only `rustup target add` or call `cargo`), and a target is ADDITIVE — rustup installs it INTO the pinned toolchain. The same is true of the `rustup target add` steps in `release.yml`'s Android, iOS and Windows jobs, and of GoReleaser's rust builder, which runs `rustup target add` itself.

## What this does NOT cover

- **The WebView2 Runtime is still evergreen and still unpinnable** (`.github/actions/webview2-runtime-version` exists precisely because it cannot be pinned). This pin is about the Rust compiler only.
- **The platform SDKs are not pinned either** — Xcode on `macos-14`, the Android NDK, the MSVC toolchain all still float with the runner image.
- **Clippy coverage is unchanged.** The Ubuntu gate still cannot compile the `#[cfg(target_os = "macos")]` / `#[cfg(windows)]` halves; the honest inventory stays in [`../verify-lints-test-targets-and-clears-the-existing-debt/README.md`](../verify-lints-test-targets-and-clears-the-existing-debt/README.md).
- **A pin can go stale**, and nothing here nags about it. That is the deliberate trade: a stale-but-known compiler beats a floating one, because only the second can red a branch that changed nothing.
