---
title: review-gate non-blocking nits for 'configurable-rpc-endpoint-via-env' (Gate 2 approve)
date: 2026-07-29
status: open
reviewOf: configurable-rpc-endpoint-via-env
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'configurable-rpc-endpoint-via-env' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the WERUST_RPC_URL CI secret pass-through was built exactly as the task prescribed, but it is build-env-only and cannot affect any shipped artifact (the read is a runtime std::env::var at session construction; WERUST_VERSION only reaches artifacts because build.rs bakes it at compile time; Android/iOS have no user-settable env at all). The builder recorded this honestly in work/notes/observations/werust-rpc-url-ci-secret-is-build-env-only-2026-07-29.md. Decide whether a follow-up (compile-time bake via option_env!, noting it publishes the URL in a public artifact) is wanted, or whether the plumbing stays as inert forward-wiring.
  (crates/werust-core/src/ethereum.rs rpc_endpoint() reads at runtime; .github/workflows/release.yml exports the secret on all three legs; the observation note names the gap and the follow-up shape.)
- Ratify: the repo had NO README.md before this branch; the task assumed one existed (a short paragraph in the README dev section), so the builder created a minimal 16-line README from scratch containing the WERUST_RPC_URL workflow. Confirm the new README's scope/tone is acceptable as the repo's landing doc.
  (README.md is a new file in commit 12f1d6f; task acceptance criterion assumed an existing README dev section.)
- Ratify: the shape test adds an extra anti-hardcode assertion (the injected workflow expression must not contain a literal http URL) beyond what the task specified. Benign hardening, but it is a self-made test-policy decision.
  (crates/werust-core/tests/release_plumbing_shape.rs, every_rust_compiling_leg_passes_the_rpc_endpoint_secret_through.)
