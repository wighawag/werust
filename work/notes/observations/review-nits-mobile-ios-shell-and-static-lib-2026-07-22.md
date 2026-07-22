---
title: review-gate non-blocking nits for 'mobile-ios-shell-and-static-lib' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: mobile-ios-shell-and-static-lib
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'mobile-ios-shell-and-static-lib' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Swift normalizeURL() prepends https:// to a bare host at the OS edge before calling core.navigate. Is edge-side URL normalization intended, or should the bare-host->https rule live in the Rust core so both mobile edges share it? Ratify.
  (WKWebViewShellController.swift normalizeURL; core still validates+rejects, so no logic is hidden, but the Android edge may or may not do the same normalization.)
- Ratify the 4 DECISIONS entries: (1) Rust Renderer backend over WKWebView driven from Swift, (2) staticlib + -force_load, (3) single-arch pin copied from wezig, (4) Xcode build as a separate macos-14 CI leg not the pure-Rust verify gate. All look correct and reversible; recorded for the human.
  (docs/spikes/mobile-ios-shell-and-static-lib/DECISIONS.md)
