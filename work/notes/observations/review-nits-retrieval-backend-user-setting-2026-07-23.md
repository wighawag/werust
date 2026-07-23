---
title: review-gate non-blocking nits for 'retrieval-backend-user-setting' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: retrieval-backend-user-setting
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'retrieval-backend-user-setting' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- On iOS, CoreSession::resolve_ipfs and apply_settings both call the same generic resolve_scheme(uri) and dispatch by the URI scheme, so neither method name constrains which scheme it serves (resolve_ipfs would also route a werust:// URI and vice-versa). Correctness relies entirely on the Swift side registering IpfsSchemeHandler for ipfs and WerustSchemeHandler for werust so each edge only ever receives its own scheme. This is documented as a deliberate honest-naming split, not a bug. Ratify the two-distinct-edge-methods-over-one-generic-dispatch choice.
  (crates/werust-ios/rust/src/lib.rs:150,183 both delegate to self.backend.resolve_scheme; tests confirm non-matching schemes return None. Swift registers per-scheme at WKWebViewShellController.swift:107,119.)
- Ratify the recorded known limitation: the retriever is built once at session new()/install_ipfs, so a backend selection takes effect on the NEXT session/launch, not live mid-session. Criterion 2 (selection switches the load path) is met on next-launch; a live hot-swap is a named follow-on.
  (DECISIONS.md Decision 4 known-limitation; consistent across all three install_ipfs sites.)
