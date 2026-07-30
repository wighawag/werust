---
title: review-gate non-blocking nits for 'windows-webview2-renderer-backend' (Gate 2 approve)
date: 2026-07-30
status: open
reviewOf: windows-webview2-renderer-backend
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'windows-webview2-renderer-backend' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify or fix: every WebView2 environment-creation refusal is reported as 'the Microsoft Edge WebView2 Runtime is not available ... Install the Evergreen Runtime', even though the constructor already proved the runtime IS present via GetAvailableCoreWebView2BrowserVersionString. A non-writable/corrupt user-data folder, a policy block or a version refusal therefore hands the user advice that cannot help (the real HRESULT survives only in the trailing parenthetical). Should create_environment map its refusal to a plain RendererError::Backend instead, keeping missing_runtime_error for the presence check?
  (crates/windows-renderer/src/backend.rs, create_environment: .map_err(|e| missing_runtime_error(&e.to_string())) on both CreateCoreWebView2EnvironmentWithOptions and the awaited result; with_user_data_folder already calls runtime_version()? first.)
- Ratify the CI-filter decision, and fix the now-stale header prose: the pull_request path filter was WIDENED with crates/windows-renderer/**, but the workflow's 'WHY THE pull_request FILTER IS NARROW' paragraph was not updated and still says windows-origin-probe is 'the only Windows-specific code in the tree' and lists the old trigger set. The task asked explicitly that a widening be justified in the workflow header. Also unrecorded: docs/spikes/windows-webview2-renderer-backend/** was added to the push filter, so a docs-only edit burns a windows-latest run.
  (.github/workflows/windows-renderer.yml lines ~50-88; task forward-pointer: 'If you widen the PR filter, say why in the workflow header'. Not in DECISIONS.md.)
- Ratify an in-scope decision not recorded in DECISIONS.md: the engine crate gained a public os_color_scheme() plus an HKCU registry read (and the Win32_System_Registry feature) purely for the sibling chrome task's benefit, while the engine itself follows the OS via PREFERRED_COLOR_SCHEME_AUTO and needs none of it. Keep it here, or let windows-win32-window-and-chrome own the reader?
  (crates/windows-renderer/src/backend.rs os_color_scheme/apps_use_light_theme; crates/windows-renderer/src/pure.rs os_color_scheme_from_apps_use_light_theme; Cargo.toml Win32_System_Registry.)
- Ratify DECISIONS.md 4 (default profile folder = %TEMP%\werust-webview2): nothing enforces the stated hand-off that windows-win32-window-and-chrome must pass a durable %LOCALAPPDATA% path, so the sibling can silently inherit a temp profile. Should that requirement be added to the sibling task's acceptance criteria now?
  (crates/windows-renderer/src/backend.rs default_user_data_folder / with_user_data_folder; work/tasks/backlog/windows-win32-window-and-chrome.md has no clause about it.)
