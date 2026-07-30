# The `windows-renderer` CI leg: what was measured, what was chosen, and what is still unproven

Task `windows-renderer-ci-leg` (sub-task 1 of [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md)'s Windows split). Deliverables: [`.github/workflows/windows-renderer.yml`](../../../.github/workflows/windows-renderer.yml), the shared [`.github/actions/webview2-runtime-version`](../../../.github/actions/webview2-runtime-version/action.yml) composite action, and the shape guard [`crates/werust-core/tests/windows_renderer_leg_shape.rs`](../../../crates/werust-core/tests/windows_renderer_leg_shape.rs).

## Why the leg lands before the code

`gh workflow run <wf> --ref <branch>` is only legal once `<wf>` exists on the DEFAULT branch. A build agent therefore cannot dispatch a workflow it is itself inventing on its own work branch, which is precisely why both macOS tasks shipped a PREDICTION where an acceptance criterion demanded a MEASUREMENT. Putting this leg on `main` first is what makes `windows-webview2-renderer-backend` and `windows-win32-window-and-chrome` measurable at all: whoever drives them can dispatch this workflow against their branch, exactly as `macos-renderer.yml` made the AppKit window provable ([run 30572253620](https://github.com/wighawag/werust/actions/runs/30572253620)).

`workflow_dispatch` is consequently not a convenience on this leg, it is the deliverable. `windows_renderer_leg_shape.rs` pins it for that reason.

## The measurement: which crates really compile for `x86_64-pc-windows-msvc`

The task's candidate set was a starting point, not an assumption to trust. Every workspace member was cross-checked for the MSVC target from this Linux development machine using [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin), which supplies the MSVC CRT/SDK headers and the `llvm-lib`/`lld-link` tooling that a plain `cargo check --target x86_64-pc-windows-msvc` lacks (without it, `ring`'s build script dies on a missing `lib.exe` long before any werust code is reached).

Reproduce (rustc 1.91.1, `cargo-xwin` 0.23.0, LLVM 19 for `llvm-lib`):

```sh
rustup target add x86_64-pc-windows-msvc
cargo install cargo-xwin --locked
PATH="/usr/lib/llvm-19/bin:$PATH" \
  cargo xwin check --target x86_64-pc-windows-msvc --tests -p <package>
```

Result, every workspace member, `--tests` included:

| Crate | `x86_64-pc-windows-msvc` | In the leg? | Why |
| --- | --- | --- | --- |
| `webview-shared` | green | **yes** | The toolkit-free half the WebView2 backend will reuse VERBATIM: `LoadLifecycle`/`SharedLifecycle`, the `navigate` URL rule, the ADR-0008 off-thread `ipfs://` boundary. Proving it on Windows is proving the backend's foundation. |
| `renderer` | green | **yes** | The `Renderer` seam the Windows backend will implement. |
| `werust-core` | green | **yes** | The one shared crate (chrome derivation, ENS/IPNS, debug capture) every shell links. |
| `fetcher` | green | **yes** | The hash-verify boundary the `ipfs://` scheme handler sits on. |
| `windows-origin-probe` | green | **yes** | The only Windows-specific code in the tree. On Windows its `cfg(windows)` WebView2 half compiles for real; its unit tests have never been RUN on Windows before this leg (the gate-0 leg only builds it and runs the measurement). |
| `native-renderer` | green | no | Green for real, excluded on COST not redness: it is the future native stack, not the Windows-shell reuse path, it is fully covered by the Ubuntu gate, and it carries the workspace's heaviest dependency tree (html5ever, parley/fontique/harfrust). A later task may add it. |
| `script-engine` | green | no | Same reasoning; today it is a stub seam with no Windows-specific surface. |
| `macos-renderer`, `werust-macos`, `macos-origin-probe` | green | no | Green only because their platform halves are `cfg(target_os = "macos")`-gated to nearly nothing here. Building them on Windows would assert nothing that `macos-renderer.yml` does not already assert on a real Mac runner. |
| `werust-android-core`, `werust-ios-core` | green | no | Same `cfg`-gated emptiness; covered by the Android and iOS legs. |
| `werust` | **red** | no | Binds gtk4/glib/cairo/pango through pkg-config (`gobject-sys` build script fails). Nothing on a Windows runner can satisfy those `.pc` files. The Linux desktop binary; the Ubuntu `verify` gate builds it. |
| `webview-renderer` | **red** | no | Same: `gio-sys`/`pango-sys` build scripts fail. The WebKitGTK backend. |

## What this measurement does NOT prove

Honesty, in the shape [ADR-0011 Amendment 1](../../adr/0011-webview2-for-windows.md) asks each platform to land with:

- A cross `cargo check` type-checks and runs build scripts; it does **not link** the final binaries and does **not RUN a single test**. The Windows-side proof of "green as landed" is the leg's own first run on `main`, which builds and tests for real.
- Nothing here touches WebView2, a window, rendering, input or HiDPI. Those belong to the two Windows code tasks this leg exists to make measurable.
- `cargo-xwin` reproduces the MSVC target headers/libs; it is not the runner's own toolchain. A divergence is possible in principle and would show up as a red first run.

## Decisions

Judgement calls made inside this task, recorded so a reviewer can ratify or reverse them.

### D1 — the `pull_request` path filter is NARROW, and deliberately unlike the macOS sibling

**Chosen:** the PR trigger fires only on `crates/webview-shared/**`, `crates/windows-origin-probe/**`, the shared version action, and the workflow file itself. The wider set the leg actually builds (`werust-core`, `fetcher`, `renderer`) is on the `push`-to-`main` filter only.

**Why:** `macos-renderer.yml` triggers on PRs touching `crates/werust-core/**`, so most core work now spends `macos-14` minutes and can be gated by a red cross-platform leg — a cost the human raised while this task was being written. Copying it reflexively would double that cost the day this lands. The narrow filter keeps PR-time coverage on what is genuinely Windows-shaped, and buys post-merge coverage for the rest.

**Alternatives considered:** (a) mirror the macOS filter — rejected as doubling a cost already under review; (b) PR-trigger on nothing, dispatch-only — rejected because a `webview-shared` change is exactly the kind that silently breaks the Windows foundation and is cheap to catch early.

**What it touches:** the macOS leg's own filter (this leg is now the counter-example in that review, not a second instance of the pattern), and every core PR's minute budget.

**The cost, stated plainly:** a `werust-core`/`fetcher`/`renderer` change that breaks the Windows build is found minutes AFTER it merges, on a leg that gates nothing, rather than before. `windows_renderer_leg_shape.rs` asserts both halves — the narrow PR list and the wider push list — so broadening is a decision that has to change the test and the header comment together.

### D2 — the WebView2 registry read was EXTRACTED into a shared composite action, which edits a second workflow

**Chosen:** `.github/actions/webview2-runtime-version/action.yml` holds the pwsh registry read verbatim; both `windows-origin-probe.yml` and `windows-renderer.yml` `uses:` it.

**Why:** the acceptance criterion says to reuse the probe workflow's step "rather than a second implementation", and GitHub Actions has no way to share a step between workflows except a composite action. Two hand-copied blocks would be two implementations free to drift to different registry keys, at which point two legs' logs stop being comparable — the very thing the version is recorded for.

**Alternatives considered:** copy the pwsh block into the new leg (rejected: literally the second implementation the criterion forbids); read the version some other way, e.g. from the runtime DLL (rejected: a different fact, and a second mechanism).

**What it touches:** `windows-origin-probe.yml`, a workflow this task otherwise has no business editing, and the scope line "one workflow file". The edit is a lift, not a rewrite; the probe leg's own path filter now includes the action directory, so a change to the shared read re-runs both legs. `windows_renderer_leg_shape.rs` asserts the GUID appears in the action and in NEITHER workflow, so a future copy-paste goes red.

### D3 — the leg disables git's Windows CRLF rewrite before checking out

**Chosen:** `git config --global core.autocrlf false` as the first step, before `actions/checkout`.

**Why:** Git for Windows defaults to `core.autocrlf=true`, so a Windows checkout rewrites every committed LF to CRLF. Several tests in this repo PARSE committed source text and match MULTI-LINE patterns (`crates/werust-core/tests/*_shape.rs`, e.g. the `"\n    }\n"` block terminator in `chrome_css_class_set_edge_wiring_shape.rs`), and those patterns do not survive the rewrite. Without this step the leg would go red on arrival for a reason that has nothing to do with Windows.

**Alternatives considered:** (a) commit a repo-wide `.gitattributes` forcing LF — rejected as a repo-wide change well outside this task, with consequences for every platform and every contributor's editor; (b) exclude the text-parsing tests from the Windows leg — rejected: it would hide the problem and shrink what the leg proves; (c) make the tests line-ending agnostic — a real option, but it edits several files this task has no mandate over, and it is the wrong layer: the leg should test the repo's actual bytes.

**What it touches:** only this leg's runner, one step, before checkout. If a future task DOES want CRLF-tolerant shape tests, this step becomes redundant rather than wrong.

### D4 — the excluded-but-green crates

Naming this as a decision because "excluded" reads like "broken" otherwise: `native-renderer`, `script-engine`, the three macOS crates and the two mobile FFI crates are all measured GREEN for this target. They are out of the leg on cost and on relevance (see the table above), not on redness. The reasons are in the workflow header so a reader of the leg sees them without finding this file.
