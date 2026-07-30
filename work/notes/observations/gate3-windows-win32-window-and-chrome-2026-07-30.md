---
title: "Gate-3 verdict: windows-win32-window-and-chrome (APPROVE) — a fourth shell that paints, and a THIRD painter that finally shares"
date: 2026-07-30
status: open
reviewOf: windows-win32-window-and-chrome
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main` as `7b0ffe0` (after one deadline checkpoint, `1a59e15`, which resumed cleanly from the branch tip). 4,465 lines: `crates/werust-windows` (the Win32 shell), a new shared `crates/desktop-paint`, a 639-line window shape guard, the extended CI leg, the spike, and two authored follow-on tasks.

## Independent verification of the evidence

Three `workflow_dispatch` runs on `ci/windows-win32-window-and-chrome`: [30588443862](https://github.com/wighawag/werust/actions/runs/30588443862) FAILURE, then [30588844677](https://github.com/wighawag/werust/actions/runs/30588844677) and [30589670305](https://github.com/wighawag/werust/actions/runs/30589670305) SUCCESS. The visible red-then-green is a good sign, not a bad one: the leg can fail and did.

The strongest check available, and it is clean: diffing the MEASURED tree (`b312f31`, the head of the last green run) against what merged, restricted to `crates/` and `.github/`, yields **nothing at all**. The code that was measured is byte-for-byte the code that landed. The smoke's own output corroborates it from the inside — the ⋮ menu printed `werust b312f31`, the version string of that very commit.

`main`'s post-merge push run was in flight at the time of writing; the branch runs are the load-bearing evidence and they are green on the identical tree.

## Criteria, ticked against the merged diff

1. **A native Windows window with every surface present, driven by the shared `BrowserShell`.** MET. The smoke drives the real window and asserts what the real widgets hold: URL bar, trust indicator, status line, error banner, invalid-entry badge, ⋮ menu, debug view, load progress.
2. **Every surface reads the SHARED derivation; anything missing is EXTRACTED and consumed by GTK and macOS too.** MET, and this is the criterion the task was really about. `crates/desktop-paint` is new: the painter half shared by the native-widget edges, with `werust-macos` converted to consume it (`pub use desktop_paint as paint`) in the same change. That is the honest version of "extract and have both edges consume it" — the alternative (Windows quietly copying macOS's painter) is precisely the fourth-copy failure ADR-0011 warned about, and it did not happen. The smoke asserts the trust badge's TOOLTIP is the core's explanation, which is the exact string that shipped desktop-only for months.
3. **⋮ menu from `BrowserMenu`; devtools is `OpenDevToolsWindow`.** MET, both asserted live (`menu: ["werust b312f31", "Debug"]`, and the shell opens Edge's real DevTools window over the live page).
4. **ADR-0009, ADR-0010 and the URL-bar-progress rule honoured, not re-decided.** MET with one recorded partial gap (the comctl32 manifest, below). The progress rule is asserted twice and well: "a successful load raises NO banner", and "the page did NOT move or resize across a whole load". The negative control proves the converse — a FAILURE is the one state allowed to displace the page.
5. **The leg extended with a real off-screen window smoke + failing negative control, run recorded.** MET, verified above. The control's banner carries the core's protocol-named reason (`block hash mismatch: bytes do not match cid bafkrei…`), not a generic string.
6. **Manual steps recorded; CI-versus-hardware stated.** MET (11 numbered manual steps, including resize/maximise/restore, which is the right place for the resize question raised in the nits).
7. **Follow-on parity-column and packaging tasks AUTHORED, not built.** MET: `windows-parity-column-and-stub-tasks` and `windows-release-packaging-leg` are in `tasks/backlog/`.
8. **Ubuntu `verify` green.** MET.

**The forward-note I planted at the previous Gate-3 was honoured, and then some.** The `%LOCALAPPDATA%` profile requirement is not merely implemented, it is ASSERTED IN THE SMOKE on a real runner: `profile folder: C:\Users\runneradmin\AppData\Local\werust\WebView2`, with an explicit check that it exists and is outside `%TEMP%`. A one-line acceptance criterion turned a silently-inherited temp profile into a proven durable one.

## Review-nit triage (8 raised at Gate 2, all non-blocking)

**Acted on (routed into existing backlog tasks rather than left in a nits file):**

- **The macOS-from-Linux type-check harness is BROKEN by this extraction.** The real one. `typecheck-macos-from-linux.sh` still symlinks `crates/werust-macos/src/paint.rs`, which this change DELETES, and its scratch manifest has no `desktop-paint` dependency, so `pub use desktop_paint as paint` cannot resolve. The Windows sibling harness was updated; the macOS one was missed. This repo writes ALL Apple and Windows code blind from Linux, so a broken cross-check harness is not cosmetic — it is the next macOS task's first five minutes, spent on a confusing error. Planted as a first-class item in `macos-spike-doc-accuracy-and-harness-guard`, which already owns that exact script (it adds the `rm -rf` guard), so the fix lands where the file is already open.
- **The repo README has no Windows section**, while macOS got one. A visitor cannot discover `cargo run -p werust-windows`. Folded into `windows-backend-error-mapping-and-leg-header-accuracy`.
- **The spike README claims `cargo xwin clippy` but the committed harness ends in `cargo xwin check`.** Same class as the macOS doc-accuracy nits, same treatment: folded into the same task.
- **`crates/desktop-paint/**` was added to the `pull_request` filter of BOTH the Windows and macOS legs, and the narrow-filter guard does not pin it.** So the shared painter now gates PRs on two cross-platform runners, and a later broadening has no test to change. That is the deliberately-narrow filter widening by accretion, twice in two tasks, which is exactly the drift the guard existed to prevent. Folded into the same task, which already owns the filter question.

**Ratified, no action:**

- **`SetAreDevToolsEnabled(cfg!(debug_assertions))`.** Not a new decision at all: the `web-inspector` capability row and the 2026-07-23 gating note make the debug-build gate the repo-wide RULE ("a release build is not silently inspectable"). Windows conforming to it is the correct default, and a release build can opt in later by flipping the same gate.
- **`WM_SIZE` handled in the container's window proc with a controller borrowed through `GWLP_USERDATA`.** The right layer (no seam widening), recorded in `DECISIONS.md` §3a, and the teardown ordering is guarded in `Drop`. Only the initial layout is exercised on CI; a real resize is on the manual-verification list, which is where it belongs given this repo has no Windows hardware.
- **No comctl32 v6 manifest**, so the chrome draws classic-styled on Windows 11 and system-drawn buttons do not follow dark mode. A genuine partial ADR-0009 gap, but recorded twice and OWNED by `windows-release-packaging-leg`, which is the task that must add a manifest anyway. Nothing reaches a user before that leg lands. Deferral ratified; flagged to the human as a ratify item since it is a visible product default.
- **The `desktop-paint` NAME.** The nit's argument is sound (`webview-shared` names its layer, not its form factor, and the GTK "desktop" edge is deliberately NOT a consumer). But renaming a crate that landed an hour ago, in the same week the macOS task explicitly REFUSED a rename for the same reason ("renaming it here would bury the chrome work in a refactor"), trades a real churn cost for a hypothetical future misreading. Kept, and raised to the human as a cheap-now / expensive-later naming call rather than decided unilaterally.

## An off-path finding the build filed correctly

`settings-dir-has-no-windows-branch-2026-07-30.md`: `werust_core::retrieval::settings_dir()` has no Windows branch, so on Windows it returns `None` and the user's chosen retrieval backend is silently forgotten at exit. Found while looking for a per-user path, correctly NOT fixed in scope, and the window's own profile deliberately names the same `%LOCALAPPDATA%\werust\` vendor directory a core Windows branch would want, so the two converge rather than collide. That is the capture-don't-fix rule working as intended. It needs a task; it is in the human batch because it is a settings-concept decision (where Windows state lives), not a mechanical port.

## What this unlocks

Windows is a complete desktop shell: engine, window, chrome, debug view, devtools, durable profile. `desktop-paint` now has two consumers and is the natural home for any third native-widget edge. Newly authored and buildable: `windows-parity-column-and-stub-tasks`, `windows-release-packaging-leg`. The macOS parity column and packaging leg (next in this drive) now have a Windows sibling to be consistent with.
