---
title: review-gate non-blocking nits for 'fix-ci-verify-missing-webkitgtk-system-deps' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: fix-ci-verify-missing-webkitgtk-system-deps
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'fix-ci-verify-missing-webkitgtk-system-deps' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the un-recorded in-scope decision: the agent also rewrote the goreleaser DESKTOP leg from libwebkit2gtk-4.1-dev+libgtk-4-dev to libwebkitgtk-6.0-dev. The task only asked to add deps there if required; the agent instead CORRECTED a latent ABI mismatch (the crate binds webkit6 0.5 / gtk4 0.10, which need webkitgtk-6.0.pc, so the old webkit2gtk-4.1 package was the wrong ABI and that leg would have failed to link). Correct and reversible, but no Decisions block recorded it.
  (release.yml goreleaser job: -sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-4-dev  +sudo apt-get install -y --no-install-recommends pkg-config libwebkitgtk-6.0-dev. Confirmed vs crates/webview-renderer/Cargo.toml: webkit6 =0.5.0, gtk4 =0.10.0.)
