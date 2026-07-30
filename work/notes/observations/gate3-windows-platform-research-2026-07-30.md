---
title: "Gate-3 conductor review: windows-platform-research (APPROVE)"
date: 2026-07-30
status: open
reviewOf: windows-platform-research
verdict: approve
---

## Verdict: APPROVE

Merged as `d225a2b` on `origin/main` (drive-tasks, `--allow-backlog --review --merge`, `etherplay/opus-5`). Research only, no code changed. Gate-1 and Gate-2 green, 4 non-blocking nits.

## Acceptance criteria, ticked

- [x] **A committed ADR with the findings.** `docs/adr/0011-webview2-for-windows.md` (the number I corrected in the pre-drive cleanup; `0005` was already the parity guard), plus a spike README carrying the evidence and a DECISIONS.md carrying the judgement calls.
- [x] **A clear go / no-go / defer plus a rough breakdown.** DEFER, with named revisit triggers and a 6-step breakdown (22 to 39 person-days) if funded.
- [x] **Self-contained.** A reader can decide funding from the ADR alone.
- [x] **Key technical questions answered with references.** Custom-scheme interception, SvelteKit compatibility and CI strategy all answered against primary sources.
- [x] **The forward-pointer's generic-desktop-seam question answered in a form that decides the macOS split.** Answered explicitly, and answered AGAINST the repo's own precedent rather than from first principles.

## Why this was worth running first (the human's instinct, validated but redirected)

The reason for running Windows before splitting macOS was that the two might share one answer. They do, but not the answer either of us assumed. The research declined to invent a `DesktopShell` seam (it would duplicate `Renderer` + `BrowserShell` at the wrong layer and re-mean "seam", which in this codebase means a hot-swappable backend interface). What IS shared is the chrome PRESENTATION: `status_line`, `trust_indicator*`, `error_banner_*`, `invalid_entry_badge_*`, `load_progress_*` are pure functions of `ChromeState` that today live in the GTK edge in Rust and are re-derived a second time in Kotlin and a third in Swift. Extracting the Rust copy into the toolkit-free core leaves GTK as a pure painter and makes every later window (Win32, AppKit, eventually the mobile twins) paint from ONE derivation. That is a behaviour-preserving refactor worth doing with zero new platforms, and it turns "a fourth shell" into "a fourth window".

The honest finding on Windows itself: WebView2 has real scheme REGISTRATION with documented tuple origins (architecturally closer to WebKitGTK than to Android's interception hook), but the exact sub-behaviour werust needs, a same-origin `fetch` plus `pushState` from a custom-scheme document, sits on an open WebView2 bug that regressed in stable runtime 144 in January 2026 and cannot be pinned because the runtime is evergreen. Verdict recorded as UNDETERMINED, not broken, with a Windows-only probe designed as gate 0. Notably, the fallback if the probe fails is not new work: it is promoting `crates/werust-android/rust/src/origin_map.rs`, which is the same mechanism Tauri's `wry` ships for Windows.

## Nit triage (4 non-blocking findings)

**Acted on by me (conductor): `macos-desktop-build` was split-brain.** The task still proposed a 3-way cut with the WKWebView backend first, while the ADR prescribes a 4-way cut with the shared presentation extraction FIRST, and the task pointed at `crates/webview-renderer/src/lib.rs` for the `Renderer` trait when it lives in `crates/renderer/src/lib.rs`. The next builder reads the task, not the ADR. Reconciled by replacing that task with the four sub-tasks the ADR prescribes.

**For the human: ratify the ADR's two-part call.** It is `Status: accepted`, which both defers Windows AND pre-decides the presentation extraction plus the macOS split shape, none of which a task funded at the time it was written. Ratify as-is, or downgrade the extraction to a recommendation.

**Worth folding into the future Windows step 2:** `crates/webview-renderer` depends on `gtk4` and `webkit6` UNCONDITIONALLY, so nothing in it compiles on Windows. The ADR hedges with "or a sibling crate", but the sizing should say plainly that a Windows backend needs its own crate and that `offthread.rs` (genuinely toolkit-free) must move to a shared home first.

**For the human: two side-effects to ratify.** A new observation was minted (`service-worker-registration-differs-by-ipfs-serving-origin-2026-07-30.md`: a site's service worker registers on Android's internal-`https` origin but NOT on a real custom-scheme origin, so the same site gets a different execution model per platform, unverified on werust and covered by no parity row), and the ADR pre-specifies a user-visible Windows default (a machine without the WebView2 Runtime fails honestly naming the runtime rather than crashing).

The service-worker finding is the more interesting of the two: it means the Android origin workaround is not merely a workaround, it is a behavioural FORK, and whichever mechanism Windows picks inherits one side of it.
