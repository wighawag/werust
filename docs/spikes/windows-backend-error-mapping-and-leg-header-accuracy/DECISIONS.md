# Judgement calls made correcting the Windows backend's error mapping and the leg's header

Task: `windows-backend-error-mapping-and-leg-header-accuracy`, the Windows sibling of [`macos-spike-doc-accuracy-and-harness-guard`](../macos-spike-doc-accuracy-and-harness-guard/DECISIONS.md). Its brief was two corrections plus three doc/filter residues, so anything below that is more than a wording fix is here on purpose: what was chosen, why, what was rejected, and what else it touches.

The engine-crate decision this task was also asked to record after the fact (the `os_color_scheme()` reader and its HKCU read) belongs with the code it governs and is [`windows-webview2-renderer-backend/DECISIONS.md` §7](../windows-webview2-renderer-backend/DECISIONS.md), not here.

## 1. A refusal after the presence check gets its OWN message, not a second meaning for the runtime-missing one

**Chosen:** a new pure function `windows_renderer::pure::environment_creation_error(detail)` alongside the existing `missing_runtime_error(detail)`. Both are plain `RendererError::Backend`s. The new one names the runtime as INSTALLED, says the environment was refused, and LEADS with the platform's own text; it never mentions the download. `create_environment`'s two failure paths now map through it, and `missing_runtime_error` is left with the one call site where the runtime may genuinely be absent: `runtime_version()`, the `GetAvailableCoreWebView2BrowserVersionString` presence check.

**Why:** `with_user_data_folder` calls `runtime_version()?` before anything else, so by the time environment creation runs the runtime is PROVEN present. A corrupt or non-writable user-data folder, a group-policy block or a version refusal was therefore being answered with "install the Evergreen Runtime", advice that cannot help, while the `HRESULT` that could help trailed in a parenthetical. Honest failure is a product value in this repo (`docs/adr/0005` on silent no-ops, the fail-closed load path), and an error that confidently misdiagnoses itself is worse than a generic one.

**Rejected:** (a) keeping ONE function and passing it a flag or a stage string, which keeps two user-visible meanings in one message and makes the caller responsible for a distinction the call SITE already encodes; (b) a bare `RendererError::Backend(format!("…: {e}"))` at each site, which loses the fact that the runtime is present, which is the single most useful thing werust knows at that moment, and it would have left nothing pure to unit-test on the Ubuntu gate, which is where this whole path can be tested at all.

**Touches:** the USER-VISIBLE text a shell puts in its error banner on Windows (`werust-windows`'s banner and the GTK-shaped `RendererError` display path are unchanged in shape; only the words differ, and only for a failure that previously said something false). The wording is pinned by `a_refusal_after_the_runtime_is_proven_present_is_not_reported_as_a_missing_runtime` in `crates/windows-renderer/src/pure.rs`, and the WIRING (which message each site reaches for) by `a_machine_without_the_webview2_runtime_fails_honestly` in `crates/windows-renderer/tests/windows_backend_shape.rs`, since the Ubuntu gate cannot compile `backend.rs`.

**One thing fixed in passing:** that shape guard's `between(&backend, "fn runtime_version() -> Result<String", …)` anchor matched the `Webview2Renderer` METHOD of that name, not the free function, so its slice ran to the end of the whole `impl` block and its "the check uses `missing_runtime_error`" assertion was in fact being satisfied by `create_environment`'s use of it. The anchor is now the free function (column 0). Without this the guard would have gone green on a change that emptied the presence check entirely.

## 2. `docs/spikes/windows-webview2-renderer-backend/**` STAYS on the `push` filter

**Chosen:** keep it, on `push` only, with the reason written beside the entry.

**Why:** that directory is not just prose. It holds the RECORDED EVIDENCE this leg's claims are stamped from (the verbatim `trust_hooks_smoke` transcript) and the local cross-target harness both Windows crates are iterated with. Re-recording evidence is exactly the change worth re-measuring, which is the same logic `macos-renderer.yml` applies to its two spike paths. The cost the task flagged is real but small and lands where it hurts least: the entry is on `push` to `main`, where this leg GATES NOTHING, so a README typo burns one `windows-latest` run and blocks no one. It is deliberately NOT on the `pull_request` filter, so no docs edit is ever gated on a Windows runner.

**Rejected:** dropping it. That saves a few runner-minutes and loses the property that a re-recorded measurement is re-measured, the same property the macOS leg spent a whole task acquiring. If the cost ever becomes visible, the cheaper narrowing is a `paths-ignore` for `**.md` within that directory, which keeps the transcript and the harness covered; that was not done now because it adds a second filter mechanism for a cost nobody has yet felt.

**Touches:** `windows-latest` minutes on `main` only.

## 3. `crates/desktop-paint/**` STAYS on BOTH legs' `pull_request` filters, and is now PINNED on both

**Chosen:** keep it on `windows-renderer.yml` and on `macos-renderer.yml`, and pin it in each leg's guard: `crates/werust-core/tests/windows_renderer_leg_shape.rs` for Windows, `crates/macos-renderer/tests/macos_backend_shape.rs` for macOS.

**Why:** `desktop-paint` is not Windows-shaped or macOS-shaped, so it does not obviously belong on a narrow platform filter. But it is the ONE carrier both native desktop windows paint from, and each leg's `window_smoke` is what asserts what the real widgets hold, so a break in it is genuinely cross-platform and these are the only two legs that can see it. That is a different class from the `werust-core` / `fetcher` / `renderer` dependency surface the Windows leg deliberately refuses: those are wide, frequently-touched crates whose PRs would be gated on cross-platform runners as a matter of routine; `desktop-paint` is small, edge-side, and touched by exactly the work these legs exist to check.

**Rejected:** moving it to `push` only. It would make a painter change merge green and red `main` afterwards, on precisely the two legs that can catch it. The narrowness would be a hole rather than a trade.

**Touches:** the Windows guard now pins the `pull_request` filter as an EXACT set rather than a must-have/must-not-have pair. That is the real fix for what this task was cleaning up: the pair let two later tasks widen the trigger with nothing going red, which is how the workflow's header came to describe a filter the file no longer had. Any future addition or removal must now edit that list, and the header paragraph it describes in prose, in the same change. A task that widens either filter should expect a red test, and that is the point.

## 4. The harness became the clippy its README claimed, rather than the README being softened

**Chosen:** `docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh` now ends in `cargo xwin clippy` (it ended in `cargo xwin check`), and `docs/spikes/windows-win32-window-and-chrome/README.md` says plainly that the clippy line it recorded was run beside the harness rather than by it.

**Why:** the doc claimed a stronger check than the tool performed, which is the same class of drift as item 1 in this task's brief. Two ways to close it; strengthening the tool is the one that keeps the recorded evidence true, matches the macOS sibling harness (which has always run `cargo clippy` for its Apple target), and buys something real: these `cfg(windows)` halves are the only code in the repo that the Ubuntu gate's clippy never lints at all. The change was PROVEN by running it, not by reading it: `cargo xwin clippy -p windows-renderer -p werust-windows --target x86_64-pc-windows-msvc --tests --examples`, clean on 2026-07-31 against the tree this task lands (the only warnings are two pre-existing `sha2` deprecations from `crates/fetcher`, which are not Windows-specific and appear on the Ubuntu gate too).

**Rejected:** correcting the README to say "check". It is the cheaper edit and it would have left the weaker tool in place while a human went on believing the stronger claim had been made once.

**Touches:** anyone running that harness locally now gets clippy's lints on the Windows halves, so a lint that was invisible before can now fail their inner loop. No CI leg runs this script (it needs `cargo-xwin`, LLVM and a network fetch of the MSVC SDK), so nothing in the gate changes.

## 5. The README's Windows section says the shell EXITS without the runtime, rather than degrading in-window

**Chosen:** the new "The Windows shell (`werust-windows`)" section in the repo `README.md` states that without the WebView2 Runtime werust exits with a message naming the runtime and its download.

**Why:** that is what the code does. `Webview2Renderer::with_user_data_folder` fails the presence check before any window exists, and `crates/werust-windows/src/main.rs` prints `werust: <error>` and returns a failure exit code. A first draft of this section said the window still opens and shows the message, which reads better and is false, exactly the drift this task exists to remove. Whether a bare Windows box SHOULD get a window with an explanation in it is a product question for the shell, not something to imply in a README.

**Touches:** nothing in code. If a later task makes the Windows shell degrade in-window (the GTK shell's error banner is the obvious model), this sentence has to change with it.
