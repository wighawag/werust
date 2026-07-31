# The `verify` gate now lints test targets, and its lints now bite

Task `verify-lints-test-targets-and-clears-the-existing-debt`. Origin: `work/notes/observations/verify-clippy-does-not-lint-test-targets-2026-07-30.md`, ratified by the human on 2026-07-31.

Measured on 2026-07-31 with `cargo 1.91.1` / `clippy 0.1.91` on Linux, against the full workspace.

## What the gate says now

```
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test
```

Declared once in `dorfl.json` and mirrored verbatim in `.github/workflows/verify.yml` and `.github/workflows/release.yml`. `crates/werust-core/tests/verify_gate_shape.rs` reds if any of the three drifts from the others, or if `--all-targets` or the deny flag is dropped.

## The re-taken inventory

The 2026-07-30 observation predicted a `debug.rs` `unnecessary use of copied` plus nine `field_reassign_with_default` in `werust-macos`'s paint tests. The `debug.rs` one survived; the paint tests had MOVED (into `crates/desktop-paint` by the `windows-win32-window-and-chrome` extraction) and GROWN to twelve, and three more lints existed that the observation never saw. What `cargo clippy --all-targets` actually reported, and what was done:

| Lint | Site | Count | Fix |
| --- | --- | --- | --- |
| `clippy::field_reassign_with_default` | `crates/desktop-paint/src/lib.rs` tests | 12 | Struct-literal initialisation with `..Default::default()`, and the nine ad-hoc `ChromeState`s that fed one loop collapsed into the `vec![]` they were being pushed into |
| `clippy::unnecessary_to_owned` | `crates/werust-core/src/debug.rs` tests | 1 | Iterate the `&[&str]` const directly instead of `.iter().copied()` |
| `clippy::manual_contains` | `crates/werust-core/tests/windows_renderer_leg_shape.rs` | 1 | `push.contains(&dir)` |
| `clippy::doc_lazy_continuation` + `clippy::doc_overindented_list_items` | `crates/macos-renderer/tests/macos_backend_shape.rs` | 2 | The module doc numbered criteria 1..6 and then wrote `7/8.`, which is not a markdown list marker, so the rest of the list ran into item 6. Split into real items 7 and 8 |
| `deprecated` (rustc, not clippy) | `crates/fetcher/src/lib.rs` (lib target) | 2 | `GenericArray::as_slice` is deprecated pending generic-array 1.x; slice through `Deref` (`&x[..]`) instead |

Every one is a real fix. **No `#[allow]` was added anywhere**, at any scope, so no lint is silenced and there is nothing to audit later. No lint turned out to be pervasive-but-unhelpful for test code, so there was nothing to propose configuring in `[lints]` or a `clippy.toml`.

Note the last row: those two `deprecated` warnings were on a LIB target, i.e. the old bare `cargo clippy` printed them on every single run and the gate stayed green anyway. That is the clearest evidence for the decision below.

## Decisions

### The gate denies warnings, not just lints all targets

**Chosen:** `cargo clippy --all-targets -- -D warnings`.

**Why:** `cargo clippy` EXITS 0 on warnings. The task asked for `--all-targets` and, separately, for proof that "a deliberate test-only lint reds the gate", and the second is impossible without a deny flag, because `--all-targets` alone only widens what gets PRINTED. Three runs over one deliberately-injected `field_reassign_with_default` in `desktop-paint`'s tests, on the same tree, say it plainly:

| Command | Result |
| --- | --- |
| `cargo clippy` (the old gate) | not even printed, exit **0** |
| `cargo clippy --all-targets` (the flag flip alone) | 1 warning printed, exit **0** |
| `cargo clippy --all-targets -- -D warnings` (landed) | `error:`, exit **101** |

So the flag flip alone would have left the lint half of the gate advisory: strictly wider, still toothless. This is also why the pre-existing `fetcher` deprecations had sat in a LIB target unnoticed.

**Alternatives considered.** `-D clippy::all` would deny clippy's lints but leave rustc's own (`deprecated`, `dead_code`, `unused_*`) as warnings. Rejected, because unused/dead test scaffolding is exactly the debt this task is about and it is a rustc lint, and because the criterion said "clean". Leaving the deny off entirely was rejected for the reason above.

**What it touches.** Every future task in this repo, and the two CI legs. The known cost: the toolchain is not pinned (`rustup component add rustfmt clippy` takes whatever the runner has), so a new Rust release that adds a lint can red the gate for a task that did not cause it. That is the same exposure `cargo fmt --check` already carries, and it is cheaply reversed (one string in `dorfl.json`, plus the two mirrors the shape guard forces you to keep in step).

### The three copies of the gate are now guarded

`.github/workflows/verify.yml` already claimed in a comment to be "identical to dorfl.json's `verify`" and `release.yml` gates the tag build on the same claim, but nothing checked it. Making the local gate stricter without the workflows would have been exactly the drift that comment was there to prevent, so `crates/werust-core/tests/verify_gate_shape.rs` now asserts each `&&`-separated step of `dorfl.json`'s `verify` appears verbatim as a `run:` line in both workflows' `verify` job. It is a new GUARD over an existing claim, not a new concept.

## What the flipped gate covers, and what it cannot

The Ubuntu gate compiles all 18 workspace members, so `--all-targets` lints every lib, bin, integration test, `#[cfg(test)]` module and example that compiles on Linux:

- **Fully covered** (whole crate, tests included): `werust`, `werust-core`, `renderer`, `fetcher`, `script-engine`, `webview-renderer`, `webview-shared`, `desktop-paint`, `native-renderer`.
- **Fully covered too, despite being "mobile"**: `werust-android-core` and `werust-ios-core` are plain Rust on Linux; only android's `jni_exports` module (`crates/werust-android/rust/src/lib.rs` ~947-1449) is `#[cfg(target_os = "android")]`. Their Kotlin and Swift edges are of course not Rust and are outside clippy entirely.
- **Host-independent half only**: `macos-renderer`, `werust-macos`, `macos-origin-probe`, `windows-renderer`, `werust-windows`, `windows-origin-probe`. The gate lints their pure decision rules (`pure.rs`, `facts.rs`, `cli.rs`, `profile.rs`, `page.rs`), their unit tests and their source-shape guards, which is genuinely most of what those crates are ASSERTED on. It does not, and cannot, lint the platform half:

  | Unlinted on Ubuntu | Lines |
  | --- | --- |
  | `crates/windows-renderer/src/backend.rs` | 1565 |
  | `crates/werust-macos/src/window.rs` | 1304 |
  | `crates/werust-windows/src/window.rs` | 1247 |
  | `crates/macos-renderer/src/backend.rs` | 1170 |
  | `crates/windows-origin-probe/src/win.rs` | 503 |
  | `crates/werust-windows/src/chrome.rs` | 362 |
  | `crates/werust-windows/src/debugview.rs` | 336 |
  | `crates/macos-origin-probe/src/mac.rs` | 320 |
  | `crates/werust-windows/src/win32.rs` | 207 |
  | plus each `main.rs`'s platform arm and `werust-android`'s `jni_exports` | ~550 |

  Roughly 7.5k lines of platform Rust, still unlinted after this change.

- **Examples.** There are eight and no benches. `werust-core/examples/{print_version,chrome_json_cost}.rs`, `native-renderer/examples/render_subset.rs` and `webview-renderer/examples/navigate_and_show.rs` are linted in full. The four load-bearing CI smokes (`macos-renderer/examples/trust_hooks_smoke.rs`, `werust-macos/examples/window_smoke.rs`, `windows-renderer/examples/trust_hooks_smoke.rs`, `werust-windows/examples/window_smoke.rs`) are `#[cfg]`-gated to their platform, so `--all-targets` on Linux lints only their `#[cfg(not(...))]` stub `main`. Their real bodies are in the unlinted set above.

Read `--all-targets` as "every TARGET on this host", never as "every crate".

## The follow-on this deliberately does not do

`.github/workflows/macos-renderer.yml` and `.github/workflows/windows-renderer.yml` already build, test and RUN those crates on native runners, but neither runs clippy at all. Adding the same `cargo clippy --all-targets -- -D warnings` (scoped with `-p`) to those two legs is the cheap way to cover the ~7.5k lines above, and it needs no cross-target trick: the platform half already compiles there. Out of scope here because it lands lint debt discovery on legs this task cannot see the output of, and because clearing whatever it finds is its own inventory. Captured in `work/notes/observations/platform-ci-legs-never-run-clippy-2026-07-31.md`.
