---
title: review-gate non-blocking nits for 'ipfs-retrieval-off-main-thread-no-ui-freeze' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: ipfs-retrieval-off-main-thread-no-ui-freeze
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'ipfs-retrieval-off-main-thread-no-ui-freeze' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The commit/PR body carries no Decisions block. Ratify: using gio spawn_blocking shared pool (vs a dedicated worker+queue) for off-thread retrieval. ADR-0008 documents+justifies it (integrates with the GTK main context, transport timeout still bounds each fetch). Reasonable; human to confirm.
  (backend.rs install_ipfs uses gio::spawn_blocking; ADR-0008 Considered options rejects the dedicated-worker alternative.)
- Ratify: a panicked retrieval worker is surfaced as a fail-closed load with RendererError::Backend (retrieval worker panicked), not rendered. Correct fail-closed choice; recording it for the human.
  (backend.rs: blocking.await.unwrap_or_else(|_| Err(Backend(...panicked))).)
- Ratify: the werust://settings scheme handler in install_ipfs is left SYNCHRONOUS on the main thread while only ipfs:// went off-thread. Justified because apply_settings_request is local file I/O (no network CAR fetch), so no freeze; in scope for the task which targets ipfs:// network retrieval.
  (backend.rs register_uri_scheme(WERUST_SCHEME,...) still calls apply_settings_request synchronously.)
