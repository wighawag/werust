---
title: "Gate-3 conductor review: configurable-rpc-endpoint-via-env (APPROVE)"
date: 2026-07-29
status: open
reviewOf: configurable-rpc-endpoint-via-env
verdict: approve
---

## Verdict: APPROVE

Merged as `c0e463c`, first dispatch on kimi-k3 (landed before the kill). 14 `ethereum` unit tests + 15 `release_plumbing_shape` tests re-run locally green.

## Acceptance criteria, ticked against the merged tree

- [x] **`DEFAULT_RPC_ENDPOINT` switched to `https://1rpc.io/eth`.** `crates/werust-core/src/ethereum.rs:87` now carries the new constant; the doc comment at `:77` names the new default and notes the previous one (`publicnode.com`). The constant's name and `pub` visibility are unchanged.
- [x] **`WERUST_RPC_URL` env var overrides the default when set and non-empty.** `fn rpc_endpoint() -> String` at `:108` reads `std::env::var("WERUST_RPC_URL")`, trims, falls back to `DEFAULT_RPC_ENDPOINT` on empty/unset. `RpcProvider::new()` uses `Self::with_endpoint(&rpc_endpoint())`. Tests: `rpc_endpoint_prefers_a_non_empty_env_value_whitespace_trimmed`, `rpc_endpoint_falls_back_to_the_default_when_the_env_is_empty`, `rpc_endpoint_falls_back_to_the_default_when_the_env_is_unset`, `the_labelled_default_endpoint_is_1rpc`.
- [x] **`.gitignore` excludes `.env*`.** Confirmed (chore `f064bef` landed before this task).
- [x] **`.env.example` committed.** Documents the shape with no real URL; warns never to commit a real endpoint.
- [x] **`release.yml` passes the secret through all three Rust-compiling legs.** 5 injection sites (3 `WERUST_RPC_URL` env vars + 2 comments). Shape test: `every_rust_compiling_leg_passes_the_rpc_endpoint_secret_through`.
- [x] **`release_plumbing_shape.rs` covers both with-secret and without-secret paths.** The test passes when the secret is configured AND when it is not.
- [x] **README dev section.** The repo had no README; the builder created a minimal 16-line one with the `WERUST_RPC_URL` workflow.

## Nit triage (3 non-blocking findings)

All ratifications:
1. **The CI secret is build-env-only** — the `WERUST_RPC_URL` read is `std::env::var` at runtime, NOT `option_env!` at compile time. On desktop local builds (`source .env && cargo run`) the override works; on Android/iOS APKs the env is not settable, so the shipped default is `1rpc.io/eth`. A compile-time bake via `option_env!` is the named follow-up if a private endpoint in mobile builds is wanted (noting it would publish the URL in a public artifact). Honestly recorded in `werust-rpc-url-ci-secret-is-build-env-only-2026-07-29.md`.
2. **New README** — the repo had no README before; the builder created a minimal one. Confirm the scope/tone is acceptable.
3. **Extra anti-hardcode assertion** in the shape test — benign hardening beyond what the task specified.

## For the human

The default RPC is now `https://1rpc.io/eth` (replacing the DNS-block-prone `publicnode.com`). For local desktop builds, `source .env && cargo run` with `WERUST_RPC_URL=https://your-private-rpc.example.com/` in `.env` works. For mobile, the compiled default is `1rpc.io/eth` — if you need `your-private-rpc.example.com` in the mobile APK, the follow-up is a compile-time `option_env!` bake (noting the URL would be in the public binary).
