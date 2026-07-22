---
title: review-gate non-blocking nits for 'fix-gtk4-feature-pin-to-v4-14-for-ci' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: fix-gtk4-feature-pin-to-v4-14-for-ci
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'fix-gtk4-feature-pin-to-v4-14-for-ci' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the in-scope deviation from the task text: the task said set webkit6 features to ['v2_50','gtk_v4_14'], but webkit6 0.5.0 has NO gtk_v4_14 feature (only gtk_v4_18, which forwards gtk/v4_18). The agent instead used ['v2_50'] and documented why via an inline NOTE. This is the correct fix (dropping the gtk-forward keeps gtk4 at the v4_14 pin) and the task pre-authorized surfacing such 4.18-only gaps, but it was not recorded in a ## Decisions block (commit msg is a single line). Confirm you accept omitting the gtk-forward feature rather than pinning it.
  (crates/webview-renderer/Cargo.toml:13-16; verified against ~/.cargo/registry/.../webkit6-0.5.0/Cargo.toml features: only gtk_v4_18 exists)
