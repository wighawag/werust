---
title: review-gate non-blocking nits for 'windows-platform-research' (Gate 2 approve)
date: 2026-07-30
status: open
reviewOf: windows-platform-research
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'windows-platform-research' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the ADR's two-part call: DEFER the Windows build, AND a new go on extracting the desktop chrome presentation into werust-core, recorded as Status accepted. The defer is well-argued, but the accepted status also pre-decides a piece of work no task funds yet and pre-decides how macos-desktop-build is split. Ratify as-is, or downgrade the extraction to a recommendation and cut it as a task?
  (docs/adr/0011-webview2-for-windows.md, Status accepted plus the Decision block; DECISIONS.md entry 1 records the choice and its rejected alternatives (go now / no-go).)
- work/tasks/backlog/macos-desktop-build.md is now split-brain with ADR-0011 and was not touched: it still proposes a 3-way cut with the WKWebView backend first, while the ADR prescribes a 4-way cut with the shared presentation extraction FIRST, and the task still asserts the Renderer trait lives in crates/webview-renderer/src/lib.rs when it is crates/renderer/src/lib.rs:695. Who reconciles the task before it is dispatched?
  (Research-only scope correctly stopped the agent editing another task, but the next builder reads the task, not the ADR. Same wrong path premise was in this task and survives in the macos one, so it is a second instance.)
- The ADR says the WebView2 backend can live in the existing crates/webview-renderer and that offthread.rs is reusable AS IS. crates/webview-renderer/Cargo.toml depends on gtk4 and webkit6 UNCONDITIONALLY, so nothing in that crate compiles on Windows: the future task needs a sibling crate and offthread.rs must be moved to a shared home. Should the ADR's step 2 sizing carry that extraction explicitly?
  (crates/webview-renderer/Cargo.toml lines 9-16; offthread.rs imports are genuinely toolkit-free (fetcher, renderer, werust_core, crate::SharedLifecycle), so only the crate boundary is the blocker. ADR hedges with or a sibling crate.)
- Two side-effects are not in the DECISIONS block: a new observation note was minted (a site's service worker registers on Android's internal-https ipfs origin but not on a real custom-scheme origin, unverified on werust and covered by no parity row), and the ADR pre-specifies a user-visible Windows default (a machine without the WebView2 Runtime must fail honestly naming the runtime rather than crash). Ratify both?
  (work/notes/observations/service-worker-registration-differs-by-ipfs-serving-origin-2026-07-30.md (no frontmatter, matching ~20 other observations here); ADR Consequences, last bullet.)

## Human/conductor triage (2026-07-30, via drive-tasks)

- **macos-desktop-build split-brain: FIXED by the conductor.** The task was replaced by the four sub-tasks ADR-0011 prescribes: `desktop-chrome-presentation-into-core` (first, platform-neutral), `macos-wkwebview-backend-and-window`, `macos-parity-column-and-stub-tasks`, `macos-release-packaging-leg`. The wrong `Renderer` path premise (`crates/webview-renderer/src/lib.rs`, actually `crates/renderer/src/lib.rs:695`) is corrected in the new tasks.
- **The gtk4/webkit6 unconditional dependency: FOLDED IN.** `macos-wkwebview-backend-and-window` states plainly that `crates/webview-renderer` cannot host the new backend and that `offthread.rs` must move to a shared toolkit-free home rather than be copied.
- Awaiting the human: ratifying ADR-0011's `Status: accepted` (it defers Windows AND pre-decides the presentation extraction + the macOS split shape), and ratifying the two side-effects (the service-worker observation, and the Windows runtime-missing honest-failure default).
