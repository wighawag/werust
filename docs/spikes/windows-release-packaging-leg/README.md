# Windows packaging: the unsigned `werust-windows.exe` zip, and the manifest the chrome was waiting for

Task: `windows-release-packaging-leg`. Decision it executes: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md), the last sub-task of its Windows breakdown ("CI and packaging … a zip attached to the tagged Release beside the existing artifacts"), funded by [Amendment 1](../../adr/0011-webview2-for-windows.md#amendment-1-2026-07-30--the-defer-is-overturned-windows-and-macos-are-funded-now). Window it ships: [`windows-win32-window-and-chrome`](../windows-win32-window-and-chrome/README.md). Sibling it is modelled on: [`macos-release-packaging-leg`](../macos-release-packaging-leg/README.md).

**Read this first.** This was WRITTEN on Linux. Nothing below has been run on Windows yet — not on a runner and not on hardware. The leg's own proof is the `windows-desktop-app` job on the `windows-latest` runner, which nobody has dispatched against this branch; the human-judgement items are under [What still awaits Windows](#what-still-awaits-windows-stated-plainly). Where a claim is checked by the Ubuntu `verify` gate, or by the Linux-hosted cross-target type-check, it says so.

## What landed

- **`crates/werust-windows/app.manifest`** — werust's Win32 application manifest, carrying exactly two declarations: the **comctl32 v6** dependency (visual styles) and **per-monitor-v2 DPI awareness** (with the pre-1607 `<dpiAware>true/pm</dpiAware>` fallback Microsoft's own documentation pairs it with). The file's own comment says what each one costs.
- **`crates/werust-windows/build.rs`** — embeds it, by handing the MSVC linker `/MANIFEST:EMBED` + `/MANIFESTINPUT:`, for the crate's BINS **and** its EXAMPLES (decision 1 and decision 2 below).
- **One line in `crates/werust-windows/src/window.rs`** — `SetWindowTheme(progress, "", "")`, because `PBM_SETBARCOLOR` "has no effect" once visual styles are enabled, and without it the manifest would silently have taken the shared palette's blue off the URL bar's progress strip (decision 3).
- **`.github/workflows/release.yml`** — a fifth leg, `windows-desktop-app`: `needs: verify` only, `runs-on: windows-latest`, `WERUST_VERSION` + `WERUST_RPC_URL` injected exactly as every other leg injects them, `cargo build -p werust-windows --release --target x86_64-pc-windows-msvc` → artifact check → `Compress-Archive` → zip check, then an idempotent `gh release create` + `gh release upload` on a tag, or an `actions/upload-artifact` on the `workflow_dispatch` dry-run.
- **`check-windows-artifact.sh`** (this directory) — the BUILD-leg acceptance check, the Windows twin of the Android `check-apk-abis.sh`, the iOS `check-app-bundle.sh` and the macOS `check-macos-app-bundle.sh`: a PE image, the manifest really embedded (comctl32 v6 by its full identity, and `PerMonitorV2`), the exe carrying the exact version the compiled core reports, and the zip really carrying the exe under a name that says UNSIGNED.
- **`crates/werust-core/tests/release_plumbing_shape.rs`**, criterion 11 — the leg's shape pinned from Linux: job key, runner, decoupling in both directions, the target and the `--release` flag, both manifest declarations, the embedding mechanism and its two scopes, the ABSENCE of any signing or installer tool, the zip's honest name, and the README's unsigned / SmartScreen / WebView2 / follow-on statements. Network-isolated: it only reads files in this repo.
- **Documentation the artifact needs to be honest**: the top-level `README.md` gains the Windows release-artifact section (unsigned, what SmartScreen does and the click-path through it, the WebView2 Runtime, the signing follow-on), and the window spike's ["what awaits real Windows hardware"](../windows-win32-window-and-chrome/README.md#what-still-awaits-real-windows-hardware-stated-plainly) list is re-stated for both manifest items.

Not in scope, deliberately: no code signing, no installer, no auto-update, no `winget`/Store packaging, and no DPI-scaling of the chrome's layout (see decision 4 and the "awaits" list).

## Decisions

**1. The manifest is embedded by the LINKER, not by a resource compiler.** `build.rs` emits `/MANIFEST:EMBED` and `/MANIFESTINPUT:<abs path>` for the MSVC target, and nothing else. *Alternative considered and rejected:* `embed-resource` (or `winres`), the usual crate answer, which drives `rc.exe`. Rejected for two reasons: it adds a build dependency to the crate that carries werust's trust-reporting chrome, which is the same instinct `docs/spikes/windows-win32-window-and-chrome/DECISIONS.md` §1 records for the window itself; and it needs a resource compiler wherever the crate is built, whereas the linker route needs one only where the crate is LINKED. That second half is load-bearing here: the Linux-hosted [cross-target type-check harness](../windows-webview2-renderer-backend/typecheck-windows-from-linux.sh) runs `cargo xwin clippy`, which never links, so the flags are simply inert there — re-run clean on this tree (2026-07-31, `-p windows-renderer -p werust-windows --tests --examples`, no errors and no warnings). *What it touches:* only `x86_64-pc-windows-msvc` gets a manifest at all; a `*-pc-windows-gnu` build gets none rather than a broken link, which is acceptable because the MSVC target is the only one werust ships (`docs/adr/0011` finding 6: it statically links the WebView2 loader, which is what makes a single-exe zip possible).

**2. The manifest applies to the EXAMPLES too, not only to the shipped binary.** `examples/window_smoke.rs` is the only place this window is EXECUTED anywhere, and comctl32 v6 is a genuinely different DLL: it is precisely why the tooltip's `cbSize` became load-bearing (the window task's README records a CI run that failed on exactly that). Manifesting the product while smoking an unmanifested build would test a configuration nobody ships. *Alternative considered and rejected:* `rustc-link-arg-bins` alone, which is the lower-risk choice for the existing green `windows-renderer` leg — precisely because the smoke would then keep running under comctl32 5.82. Rejected: the risk is the POINT. If v6 breaks something in the window, the smoke is where that must surface, not a user's desktop. *What it touches:* the `windows-renderer.yml` leg now exercises the window under visual styles; if it goes red on this change, that is a finding about the window, not about the packaging.

**3. The URL bar's progress strip opts OUT of visual styles.** `PBM_SETBARCOLOR` is documented as having no effect when visual styles are enabled, so embedding the manifest would have silently replaced the shared `desktop-paint` palette's `LOAD_PROGRESS_COLOR` with the theme's own. `SetWindowTheme(progress, "", "")` detaches that ONE control from the theme, which is the documented way to keep a custom bar colour. *Why it matters more than it looks:* `crates/desktop-paint/tests/gtk_stylesheet_agreement.rs` exists because "the same blue on both desktops" must not be a promise kept by transcription — and this would have broken it through a PACKAGING change, on the one edge whose pixels no test can see. *Alternative considered:* accept the themed colour and record it. Rejected: the palette is a product rule, and a rule that a manifest can quietly repeal is not one. *What it touches:* only the progress bar; every other control keeps its visual style.

**4. `PerMonitorV2` ships even though the chrome's layout is still raw pixels.** The task's criterion asks for the declaration and the declaration is right — a DPI-unaware browser is bitmap-scaled *including its page*, and the page is the part that matters most. But `chrome.rs` lays out in raw pixels and `ui_font()` hardcodes a 96-DPI height, so on a 200% display the chrome should now be crisp-but-small where it used to be blurry-but-proportional. *Alternative considered and rejected:* DPI-scale the layout in this task. Rejected as scope: the window spike's manual step 10 already names "the DPI follow-on" as separate work, this is Win32 layout code that no gate here can execute, and the change would have forced an edit to the layout guard that protects the URL-bar-progress product rule. *What it touches:* the by-hand HiDPI check is now judging a DIFFERENT failure mode than the one that list described; the follow-on's shape is recorded at `work/notes/observations/windows-chrome-is-raw-pixels-under-per-monitor-v2-2026-07-31.md`.

**5. The manifest does NOT close the dark-mode parity gap, and the cell stays `stubbed`.** The task's premise was that comctl32 v6 is the fix for the chrome's light-in-dark-mode push buttons. It is not: a v6 dependency buys VISUAL STYLES, and dark mode for standard Win32 controls has no public API — it is uxtheme functions exported by ordinal plus per-class `SetWindowTheme(…, "DarkMode_Explorer")`, and even that is partial. Ground truth and sources: `work/notes/findings/win32-common-controls-dark-mode-needs-more-than-a-v6-manifest-2026-07-31.md`. So `docs/platform-capability-matrix.toml`'s `follow-os-color-scheme` `windows` cell keeps its `stubbed` state (the task's own criterion said not to flip it on the manifest alone) and is re-pointed at the new backlog task `windows-chrome-dark-mode-for-common-controls`, which has a real decision to make first. *What it touches:* the parity column's decision 1, which chose not to cut a second manifest task — that reasoning still holds; what is cut here is not a manifest task but the work the manifest turned out not to do.

**6. No packaging SCRIPT beside the crate; the leg's two commands are inline.** The macOS twin put `bundle-app.sh` in the crate because a human on a Mac needs `lipo` + an `Info.plist` written the same way CI writes them. The Windows packaging is `cargo build --release --target …` followed by a zip: a script would exist only to be a script, and the iOS leg's inline `zip` is the precedent for that shape. *What it touches:* the ACCEPTANCE check still lives here as a script, exactly like its three siblings, because a check is worth running by hand against a downloaded artifact.

**7. The zip carries the exe and nothing else, and the two facts live in the README.** No README file, no license file, no loader DLL in the archive. The last is a property of the target (`*-pc-windows-msvc` links `WebView2LoaderStatic.lib`); the first two are a deliberate minimum, matching the macOS bundle. *Where "the artifact must say so" landed:* the repo's `README.md`, pinned by the shape test — not the Release NOTES, because every leg creates the Release with `gh release create --generate-notes` and the notes are therefore generated from conventional commits by whichever leg gets there first; a leg that rewrote the body to add its own paragraph would be racing the other four for it. *Reversible:* if the Release page turns out to be too far from the download, adding a `READ-ME-FIRST.txt` to the archive is a one-line change to the zip step.

## What is proven by what

| claim | proven by | where |
|---|---|---|
| the leg exists, is decoupled, and cannot block or be blocked | parsing `release.yml` | Ubuntu `verify` (`release_plumbing_shape.rs`) |
| it builds `werust-windows` in release for `x86_64-pc-windows-msvc` | parsing `release.yml` | Ubuntu `verify` |
| the manifest declares comctl32 v6 (full identity) and PerMonitorV2, with the legacy fallback | parsing `app.manifest` | Ubuntu `verify` |
| the manifest is embedded by the linker, MSVC-only, for bins and examples | parsing `build.rs` | Ubuntu `verify` |
| no signing tool and no installer crept into the leg | absence assertions over the job | Ubuntu `verify` |
| the version has no second source in the leg | `minting_values_of` over the job's run/with/env values | Ubuntu `verify` |
| the README says unsigned + SmartScreen + WebView2 + the follow-on | parsing `README.md` | Ubuntu `verify` |
| the progress bar still gets the shared palette's colour under visual styles | parsing `window.rs` | Ubuntu `verify` |
| the Win32 source (including the new `SetWindowTheme` call) type-checks against the real Windows SDK | `cargo xwin clippy … --tests --examples`, clean 2026-07-31 | local, Linux |
| the check script's assertions and its FAILURE paths behave | run against a synthetic PE + zip on Linux (missing file, no manifest, wrongly-named zip all fail) | local, Linux |
| the exe really carries the embedded manifest, and the released version | `check-windows-artifact.sh` on the built exe | `windows-desktop-app` job — **not yet run** |
| the zip really carries the exe | the same script, `--zip` | `windows-desktop-app` job — **not yet run** |
| the window still constructs and paints under comctl32 v6 | `cargo run -p werust-windows --example window_smoke` | `windows-renderer` leg — **not yet re-run under the manifest** |

## What still awaits Windows (stated plainly)

- **The leg has never run.** Neither the tag path nor the `workflow_dispatch` dry-run has been dispatched against this change, so "it builds, packages and uploads" is a claim about YAML, not a measurement. Dispatch is `gh workflow run release.yml --ref <branch>` (the dry-run path publishes nothing).
- **The window has never been smoked WITH the manifest.** Decision 2 puts comctl32 v6 under the existing `windows-renderer` leg's window smoke on purpose; that leg has not been re-run on this tree. If v6 changed a control's behaviour, that run is where it surfaces.
- **How it LOOKS is entirely unjudged.** That the chrome now draws in the modern visual style rather than the pre-Vista one is the manifest's whole purpose and no runner can see it. A human has to look.
- **HiDPI is a DIFFERENT unknown now, not a closed one.** See decision 4: expect crisp-but-small chrome and a correctly-scaled page on a 150%/200% display, and expect nothing in particular when dragging between monitors of different scale factors (there is no `WM_DPICHANGED` handler). Record what you actually see — that is the input the DPI follow-on needs.
- **Dark-mode buttons are NOT fixed** (decision 5), and now have a task of their own.
- **SmartScreen's exact wording and click-path are documented from the platform's rules, not from a run.** Nobody has downloaded this zip on a fresh machine, and Windows' reputation heuristics differ between a browser download, an `Invoke-WebRequest` and a copied file.
- **The console window** the packaged exe should open beside the browser is untested and unfixed: `work/notes/observations/windows-exe-opens-a-console-window-alongside-the-browser-2026-07-31.md`.
- **Nothing about install/upgrade/uninstall exists at all**, because nothing is installed: the artifact is a zip a user unpacks wherever they like, and the durable profile at `%LOCALAPPDATA%\werust\WebView2` survives independently of where the exe sits.

Please record what you saw, especially anything in this list, as a dated note in `work/notes/observations/`.

## Re-running the checks

```
# the ordinary gate (Linux, no Windows box needed)
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test

# the local cross-target type-check of every Windows source (NOT a build)
LLVM_BIN=/usr/lib/llvm-19/bin ./docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh

# on Windows: package exactly what the release packages, then check it
cargo build -p werust-windows --release --target x86_64-pc-windows-msvc
docs/spikes/windows-release-packaging-leg/check-windows-artifact.sh

# the whole release pipeline, publishing nothing
gh workflow run release.yml --ref <branch>
```
