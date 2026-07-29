---
title: "Configurable RPC endpoint via WERUST_RPC_URL env (so private endpoints like https://your-private-rpc.example.com/ can be used without committing the URL) — default stays publicnode.com"
slug: configurable-rpc-endpoint-via-env
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: []
---

## What to build

HUMAN REQUEST (v0.2.7): the default Ethereum RPC endpoint (`https://ethereum-rpc.publicnode.com` in `crates/werust-core/src/ethereum.rs:82`) is a single point of failure and has been observed to fail in practice (LAN routers DNS-block it, some TLS endpoints return `io: received fatal alert: InternalError`, the home network occasionally blocks it as a captive portal). The human wants to use a **private** endpoint (`https://your-private-rpc.example.com/`) on CI and locally — and the URL must NEVER be committed.

Goal: a single opt-in env var `WERUST_RPC_URL` that overrides `DEFAULT_RPC_ENDPOINT` when set and non-empty, falls back to `publicnode.com` otherwise. The default in the source stays the public endpoint (so a fresh build still works without configuration); the private URL lives only in `.env` (local) or in GitHub Actions repository / environment secrets (CI), never in the repo.

## READ-FIRST / drift check

- `crates/werust-core/src/ethereum.rs:82` — `DEFAULT_RPC_ENDPOINT = "https://ethereum-rpc.publicnode.com"`, consumed by `RpcProvider::new()` via `Self::with_endpoint(DEFAULT_RPC_ENDPOINT)` (line 359). `with_endpoint(&str)` is the public override today; it just has no caller-supplied data path.
- `RpcProvider` is constructed ONCE at `CoreSession::new` (e.g. `crates/werust-android/rust/src/lib.rs:416`, the desktop equivalent); the env read belongs there (it is the same boundary at which the existing `WERUST_VERSION` build-script read happens — a one-shot read at session construction).
- The retriever already has a const-only override on the gateway side (`pub const DEFAULT_TRUSTLESS_GATEWAY = "https://dweb.link";`); this task does NOT add an env lever for the gateway (that is a separate concern, with its own retrieval-backend-settings task). Scope = RPC endpoint ONLY.
- There is already a captured observation `work/notes/observations/retrieval-backend-setting-cannot-take-effect-on-mobile-2026-07-28.md` documenting that the in-app settings mechanism does not persist on mobile (no settings dir, requires a restart). Out of scope for this task; do NOT try to fix it here.
- `.gitignore` already covers the project's secret-leak surface; confirm it excludes `.env` (and any variant like `.env.local`, `.env.production`).

## Mechanism (prescribed; small surface)

In `crates/werust-core/src/ethereum.rs`:

1. Add a small private helper `fn rpc_endpoint() -> &'static str` that:
   - Calls `std::env::var("WERUST_RPC_URL")`.
   - If the result is `Ok(s)` and `s` is non-empty (after trimming whitespace), returns `s.trim()` (leak-free — do NOT expose the variable into a leaked Box; return the env var's storage directly, or copy into a thread-local + leaky if a `&'static str` is required; prefer the latter if `RpcProvider::with_endpoint` truly needs `&'static str`).
   - Otherwise returns `DEFAULT_RPC_ENDPOINT`.
2. `RpcProvider::new()` uses `Self::with_endpoint(rpc_endpoint())` instead of `Self::with_endpoint(DEFAULT_RPC_ENDPOINT)`.
3. Leave the `pub const DEFAULT_RPC_ENDPOINT` in place — it stays the documented default and is what `rpc_endpoint()` falls back to. It is NOT renamed and NOT made `pub(crate)` — the constant's public visibility is part of the contract for downstream / test consumers.
4. Add a doc comment on `rpc_endpoint()` explaining the precedence: `WERUST_RPC_URL` env (non-empty, trimmed) > `DEFAULT_RPC_ENDPOINT`. Note that the env is read ONCE at session construction (consistent with `WERUST_VERSION`'s build-time read), and a live change requires a relaunch — same constraint as the existing in-app settings.
5. Add a unit test that:
   - With no env var set, `RpcProvider::default().endpoint()` (or equivalent) returns `DEFAULT_RPC_ENDPOINT`.
   - With `WERUST_RPC_URL=https://example.test/rpc` set, it returns that URL (whitespace-trimmed).
   - With `WERUST_RPC_URL=""` (empty) set, it falls back to `DEFAULT_RPC_ENDPOINT`.
   - The test does NOT touch the live network (network-isolated).
   - The test does NOT mutate the env (use a thread-local override helper or restructure the read so it is testable; `std::env::set_var` is `unsafe` in recent Rust editions — be careful).

## GitHub Actions CI integration

- `.github/workflows/release.yml` and the verify gate: pass `WERUST_RPC_URL` from a repository secret to every Rust-compiling leg (desktop, android-apk, ios-simulator), exactly as `WERUST_VERSION` is passed today. Use the SAME secret name pattern (`secrets.WERUST_RPC_URL`).
- Add a documentation note in the workflow's comment block ("`WERUST_RPC_URL` is read at session construction; if absent, falls back to the public `DEFAULT_RPC_ENDPOINT`") so the secret's role is discoverable.
- Add the same precedence assertion to the existing `release_plumbing_shape.rs` test (it already asserts every Rust-compiling leg injects `WERUST_VERSION`); extend it to assert the RPC env is injected OR absent (the test should pass when the secret is configured, AND when it is not, because the secret is optional and the default works).

## Local `.env` support

- Add a `.env.example` (committed) that lists `WERUST_RPC_URL=` with a comment naming the public default and pointing to the your-private-rpc.example.com endpoint shape (no real URL).
- Confirm `.gitignore` excludes `.env`, `.env.local`, `.env.*.local`. Add a one-line comment in `.env.example` warning the developer NEVER to commit a real RPC URL and NEVER to put one in any tracked file.
- The Rust code does NOT need to load `.env` itself (the existing code reads `std::env::var` directly); the dev-time `cargo run` invokes the binary with the user's shell env, so a `direnv`-style loader or a `source .env && cargo run` workflow is the developer's choice — record it in the README's dev section as the documented workflow, do not add a runtime `.env` loader (one more dependency for a small win).

## Coherence

- The `WERUST_*` env-var namespace is established by `WERUST_VERSION` (build.rs) and `WERUST_SETTINGS_DIR` (retrieval.rs); `WERUST_RPC_URL` fits the convention.
- The endpoint stays a public constant so existing code that pins `DEFAULT_RPC_ENDPOINT` (tests, the FFI boundary) keeps compiling and passing. The override is additive at the read site.
- The mechanism is generic over the env lever and the constant default; the same shape can be reused if/when a similar need appears for the trustless gateway (separate task).

## Acceptance criteria

- [ ] `WERUST_RPC_URL` env var, when set and non-empty (whitespace-trimmed), overrides `DEFAULT_RPC_ENDPOINT` at session construction on desktop, Android, and iOS. When unset OR empty, the public `publicnode.com` default is used.
- [ ] The override is read once per session (not per request), consistent with how `WERUST_VERSION` is read at build time.
- [ ] `.gitignore` excludes `.env`, `.env.local`, `.env.*.local`. `.env.example` is committed, real `.env` files are not.
- [ ] `.github/workflows/release.yml` passes the secret through to every Rust-compiling leg.
- [ ] `release_plumbing_shape.rs` covers both the WITH-secret and WITHOUT-secret paths so the test does not fail in PRs without the secret configured.
- [ ] Unit tests for the precedence (no env / non-empty env / empty env) — network-isolated, no live network, no env mutation.
- [ ] A short paragraph in the README's dev section explaining `source .env && cargo run` and `.env.example` as the workflow.

## Blocked by

- None.

## Prompt

> Goal: add a `WERUST_RPC_URL` env var that overrides `DEFAULT_RPC_ENDPOINT` (publicnode.com) at session construction on desktop, Android, iOS. The default in the source stays publicnode.com so a fresh build still works; the private URL (e.g. `https://your-private-rpc.example.com/`) lives only in `.env` locally or in GitHub Actions secrets on CI — NEVER in the repo.
>
> Where to look: `crates/werust-core/src/ethereum.rs:82` (`DEFAULT_RPC_ENDPOINT`), `:359` (`RpcProvider::new` -> `Self::with_endpoint(DEFAULT_RPC_ENDPOINT)`), `with_endpoint(&str)` is the existing public override — add a small private helper `fn rpc_endpoint() -> &'static str` that reads `WERUST_RPC_URL` and falls back to the constant. `.gitignore` for `.env*` exclusion (already covers it; confirm). `.github/workflows/release.yml` to wire the secret (mirror the `WERUST_VERSION` injection). `.env.example` (new, committed). `crates/werust-core/tests/release_plumbing_shape.rs` (existing test) — extend to cover both the with-secret and without-secret paths. Unit tests on the precedence: no env / non-empty env / empty env — network-isolated, do not mutate the env.
>
> Out of scope: do not add an env lever for the trustless gateway (different task), do not fix the settings-dir persistence gap (separate observation), do not add a runtime `.env` loader (use the `source .env && cargo run` shell workflow, document it).