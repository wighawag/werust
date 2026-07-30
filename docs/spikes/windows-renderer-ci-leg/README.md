# The `windows-renderer` CI leg: what was measured, what was chosen, and what is still unproven

**Headline: the leg is GREEN on a real `windows-latest` runner, measured twice on 2026-07-30 ([run 30581522002](https://github.com/wighawag/werust/actions/runs/30581522002) on `main`, [run 30581549437](https://github.com/wighawag/werust/actions/runs/30581549437) dispatched by ref against this task's branch). 448 tests built and RAN on `x86_64-pc-windows-msvc`, zero failed. The recording is below; the `cargo xwin` sweep further down is the METHOD that chose the crate set, not the proof of green.**

Task `windows-renderer-ci-leg` (sub-task 1 of [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md)'s Windows split). Deliverables: [`.github/workflows/windows-renderer.yml`](../../../.github/workflows/windows-renderer.yml), the shared [`.github/actions/webview2-runtime-version`](../../../.github/actions/webview2-runtime-version/action.yml) composite action, and the shape guard [`crates/werust-core/tests/windows_renderer_leg_shape.rs`](../../../crates/werust-core/tests/windows_renderer_leg_shape.rs).

## Why the leg lands before the code

`gh workflow run <wf> --ref <branch>` is only legal once `<wf>` exists on the DEFAULT branch. A build agent therefore cannot dispatch a workflow it is itself inventing on its own work branch, which is precisely why both macOS tasks shipped a PREDICTION where an acceptance criterion demanded a MEASUREMENT. Putting this leg on `main` first is what makes `windows-webview2-renderer-backend` and `windows-win32-window-and-chrome` measurable at all: whoever drives them can dispatch this workflow against their branch, exactly as `macos-renderer.yml` made the AppKit window provable ([run 30572253620](https://github.com/wighawag/werust/actions/runs/30572253620)).

`workflow_dispatch` is consequently not a convenience on this leg, it is the deliverable. `windows_renderer_leg_shape.rs` pins it for that reason. Run 30581549437 below is that mechanic exercised for real, on this very branch, before the branch landed.

## The result: GREEN on a real `windows-latest` runner (2026-07-30)

Both runs SUCCESS, both on GitHub's `windows-latest` image `windows-2025-vs2026`:

| Run | Tree | Trigger | Result |
| --- | --- | --- | --- |
| [30581522002](https://github.com/wighawag/werust/actions/runs/30581522002) | `main`, the moment the workflow landed (commit `c9e7430`) | `push` | SUCCESS, 441 tests passed |
| [30581549437](https://github.com/wighawag/werust/actions/runs/30581549437) | this task's branch `work/task-windows-renderer-ci-leg` | `workflow_dispatch --ref` | SUCCESS, 448 tests passed |

The branch run is the one acceptance turns on: it carries the whole deliverable, including the shape guard that is the 7-test difference between the two totals, and it is simultaneously the first live proof that `gh workflow run … --ref <branch>` works on this leg, which is the entire reason the leg lands ahead of the Windows shell code.

The runner's WebView2 Runtime version, recorded by the shared composite action, verbatim from the branch run (identical on `main`'s run):

```
WebView2 Runtime (registry HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}): 150.0.4078.65
```

Every crate in the leg's set BUILT, and its tests RAN and PASSED, on `x86_64-pc-windows-msvc`: 448 passed, 0 failed, 0 ignored (branch run):

| Test binary | Passed |
| --- | --- |
| `fetcher` (lib) | 36 |
| `renderer` (lib) | 20 |
| `webview-shared` (lib) | 5 |
| `werust-core` (lib) | 276 |
| `werust-core` `browser_menu_edge_wiring_shape` | 6 |
| `werust-core` `chrome_css_class_set_edge_wiring_shape` | 4 |
| `werust-core` `debug_capture_edge_wiring_shape` | 9 |
| `werust-core` `debug_view_desktop_wiring_shape` | 5 |
| `werust-core` `debug_view_mobile_wiring_shape` | 5 |
| `werust-core` `ipfs_redirects_fixture` | 18 |
| `werust-core` `platform_capability_parity` | 9 |
| `werust-core` `redirect_navigation_edge_shape` | 5 |
| `werust-core` `release_plumbing_shape` | 20 |
| `werust-core` `windows_renderer_leg_shape` | 7 |
| `windows-origin-probe` (lib) | 23 |

### What the run settles that a cross `cargo check` never could

These were exactly the runtime-only risks the sweep below could not see, and each is now closed by a real execution rather than an argument:

- **The CRLF workaround (D3) works.** Every `*_shape.rs` test parses committed source and matches multi-line patterns; all of them passed, so `core.autocrlf false` before checkout really does give the runner the repo's bytes.
- **The loopback-TCP and sleep-based tests in `fetcher` and `werust-core` pass on Windows**, the ones that depend on real sockets and real timing, which no type-check exercises.
- **The temp-dir scratch handling in `retrieval.rs` passes** on Windows' filesystem semantics.
- **`windows-origin-probe`'s 23 host-independent tests RAN on Windows for the first time.** Until this run, the gate-0 leg only built that crate and ran the probe measurement; its decision rules had never executed on the platform they describe.

## How the crate set was CHOSEN: cross-checking every workspace member

This sweep is the METHOD behind the set the leg builds, and the honest provenance of every exclusion named below. It is deliberately NOT presented as the proof of green: a cross `cargo check` type-checks and runs build scripts, but it does not link and it runs zero tests. The proof of green is the run recorded above.

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
| `windows-origin-probe` | green | **yes** | The only Windows-specific code in the tree. On Windows its `cfg(windows)` WebView2 half compiles for real; its unit tests had never been RUN on Windows before this leg (the gate-0 leg only builds it and runs the measurement). The leg's first runs closed that: 23 passed. |
| `native-renderer` | green | no | Green for real, excluded on COST not redness: it is the future native stack, not the Windows-shell reuse path, it is fully covered by the Ubuntu gate, and it carries the workspace's heaviest dependency tree (html5ever, parley/fontique/harfrust). A later task may add it. |
| `script-engine` | green | no | Same reasoning; today it is a stub seam with no Windows-specific surface. |
| `macos-renderer`, `werust-macos`, `macos-origin-probe` | green | no | Green only because their platform halves are `cfg(target_os = "macos")`-gated to nearly nothing here. Building them on Windows would assert nothing that `macos-renderer.yml` does not already assert on a real Mac runner. |
| `werust-android-core`, `werust-ios-core` | green | no | Same `cfg`-gated emptiness; covered by the Android and iOS legs. |
| `werust` | **red** | no | Binds gtk4/glib/cairo/pango through pkg-config (`gobject-sys` build script fails). Nothing on a Windows runner can satisfy those `.pc` files. The Linux desktop binary; the Ubuntu `verify` gate builds it. |
| `webview-renderer` | **red** | no | Same: `gio-sys`/`pango-sys` build scripts fail. The WebKitGTK backend. |

## What the run still does NOT prove

Honesty, in the shape [ADR-0011 Amendment 1](../../adr/0011-webview2-for-windows.md) asks each platform to land with:

- **Nothing in this leg drives WebView2 itself.** The runtime version is READ off the registry; no `CoreWebView2` is created, no window opens, nothing navigates, renders, takes input or scales for HiDPI. Those belong to the two Windows code tasks this leg exists to make measurable, and the backend task is where WebView2 stops being a version string and becomes a behaviour.
- **Only the toolkit-free crate set is covered.** `native-renderer` and `script-engine` are measured green for this target but are not in the leg (cost, see the table); `werust` and `webview-renderer` are measured red and never will be. A Windows regression inside those is not something this leg can see.
- **The `cargo xwin` sweep remains a cross-check, not a runner.** It chose the set; the runner proved it. Where the two could in principle diverge (xwin reproduces the MSVC headers/libs rather than being the runner's own toolchain), the runner is now the authority for the five crates in the leg, while the other rows in the table are still sweep-only evidence.
- **`windows-latest` is a moving image, and the WebView2 Runtime is evergreen.** `150.0.4078.65` on `windows-2025-vs2026` dates this result; it does not pin anything. That is exactly why every Windows leg records the version it measured on.

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

**Confirmed by the run:** all 61 `*_shape.rs` tests, every one of which parses committed source text, passed on the Windows runner, so the workaround is load-bearing and sufficient as written.

### D4 — the excluded-but-green crates

Naming this as a decision because "excluded" reads like "broken" otherwise: `native-renderer`, `script-engine`, the three macOS crates and the two mobile FFI crates are all measured GREEN for this target. They are out of the leg on cost and on relevance (see the table above), not on redness. The reasons are in the workflow header so a reader of the leg sees them without finding this file.
