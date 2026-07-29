---
title: "DEFAULT_RPC_ENDPOINT switches to https://1rpc.io/eth AND is configurable via WERUST_RPC_URL env (so private endpoints like https://your-private-rpc.example.com/ can be used without committing the URL)"
slug: configurable-rpc-endpoint-via-env
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: []
---

## What to build

HUMAN REQUEST (v0.2.7, follow-up): the default Ethereum RPC endpoint (`https://ethereum-rpc.publicnode.com` in `crates/werust-core/src/ethereum.rs:82`) is a single point of failure and has been observed to fail in practice (LAN routers DNS-block it, some TLS endpoints return `io: received fatal alert: InternalError`, the home network occasionally blocks it as a captive portal). Two coordinated changes:

1. **Change the default from `publicnode.com` to `https://1rpc.io/eth`** — `1rpc.io` is a public, keyless, broadly-deploying Ethereum RPC that has not been observed to be DNS-blocked / TLS-blocked the way publicnode.com has. The privacy posture is similar (a public RPC still sees every ENS lookup) but the failure shape is better.
2. **A single opt-in env var `WERUST_RPC_URL` that overrides the default** when set and non-empty, falls back to the new default otherwise. The default in the source stays public (`1rpc.io/eth`) so a fresh build still works without configuration; the human's private endpoint (`https://your-private-rpc.example.com/`) lives only in `.env` (local) or in GitHub Actions repository / environment secrets (CI), never in the repo.

The rationale for combining the two: a privacy-focused browser's default egress is a human-nod item (the prior drive's review-nits file captured exactly that); switching the default AND adding the env override addresses both the failure-shape and the privacy-customisation concerns in one task. The capability-matrix description and the `ethereum-provider-seam-and-trusted-rpc-backend` spike README also need to name the new default.

## READ-FIRST / drift check

- `crates/werust-core/src/ethereum.rs:82` — `DEFAULT_RPC_ENDPOINT = "https://ethereum-rpc.publicnode.com"`, consumed by `RpcProvider::new()` via `Self::with_endpoint(DEFAULT_RPC_ENDPOINT)` (line 359). `with_endpoint(&str)` is the public override today; it just has no caller-supplied data path. **Both the constant AND the doc comment on line 77 (which names the URL literally) need to be updated to `https://1rpc.io/eth`.**
- `RpcProvider` is constructed ONCE at `CoreSession::new` (e.g. `crates/werust-android/rust/src/lib.rs:416`, the desktop equivalent); the env read belongs there (it is the same boundary at which the existing `WERUST_VERSION` build-script read happens — a one-shot read at session construction).
- The retriever already has a const-only override on the gateway side (`pub const DEFAULT_TRUSTLESS_GATEWAY = "https://dweb.link";`); this task does NOT add an env lever for the gateway (that is a separate concern, with its own retrieval-backend-settings task). Scope = RPC endpoint ONLY.
- There is already a captured observation `work/notes/observations/retrieval-backend-setting-cannot-take-effect-on-mobile-2026-07-28.md` documenting that the in-app settings mechanism does not persist on mobile (no settings dir, requires a restart). Out of scope for this task; do NOT try to fix it here.
- `.gitignore` already covers the project's secret-leak surface; confirm it excludes `.env` (and any variant like `.env.local`, `.env.production`).

## Mechanism (prescribed; small surface)

In `crates/werust-core/src/ethereum.rs`:

0. Update the `DEFAULT_RPC_ENDPOINT` constant and its doc comment on line 77 from `https://ethereum-rpc.publicnode.com` to `https://1rpc.io/eth`. The constant's name and public visibility stay unchanged; the `/// swap)` doc just notes that the RPC backend is a runtime-swappable seam with `with_endpoint`, the labelled default is `1rpc.io/eth`, and the user's `WERUST_RPC_URL` env (when set) takes precedence over both.

1. Add a small private helper `fn rpc_endpoint() -> String` that:
   - Calls `std::env::var("WERUST_RPC_URL")`.
   - If the result is `Ok(s)` and `s` is non-empty (after trimming whitespace), returns `s.trim().to_string()`.
   - Otherwise returns `DEFAULT_RPC_ENDPOINT.to_string()`.
   The return type is `String` (NOT `&'static str`) because `std::env::var` owns its data. `RpcProvider::with_endpoint(&str)` already takes `&str`, so this composes without a leak.

2. `RpcProvider::new()` uses `Self::with_endpoint(&rpc_endpoint())` instead of `Self::with_endpoint(DEFAULT_RPC_ENDPOINT)`.

3. Leave the `pub const DEFAULT_RPC_ENDPOINT` in place (now pointing at `1rpc.io/eth`) — it stays the documented default and is what `rpc_endpoint()` falls back to. It is NOT renamed and NOT made `pub(crate)` — the constant's public visibility is part of the contract for downstream / test consumers.

4. Add a doc comment on `rpc_endpoint()` explaining the precedence: `WERUST_RPC_URL` env (non-empty, trimmed) > `DEFAULT_RPC_ENDPOINT`. Note that the env is read ONCE at session construction (consistent with `WERUST_VERSION`'s build-time read), and a live change requires a relaunch — same constraint as the existing in-app settings.

5. Add a unit test that:
   - With no env var set, `RpcProvider::default()` returns the new `DEFAULT_RPC_ENDPOINT` (`https://1rpc.io/eth`).
   - With `WERUST_RPC_URL=https://example.test/rpc` set, it returns that URL (whitespace-trimmed).
   - With `WERUST_RPC_URL=""` (empty) set, it falls back to `DEFAULT_RPC_ENDPOINT`.
   - The test does NOT touch the live network (network-isolated).
   - The test does NOT mutate the env (use `unsafe { std::env::set_var(...) }` in Rust 2024, or pull the read into a small trait/object the test can stub — DO NOT use the unsafe setter in test code that runs in parallel with other tests, which would race; the trait/object approach is safer).

Also update the following doc + observation sites to name the new default (the constant rename alone leaves stale prose):
- `docs/spikes/ethereum-provider-seam-and-trusted-rpc-backend/README.md:31` — the spike's "Labelled default endpoint" paragraph names `ethereum-rpc.publicnode.com` literally; update to `https://1rpc.io/eth`.
- `work/notes/observations/gate3-ethereum-provider-seam-and-trusted-rpc-backend-2026-07-22.md` — the ratification-nits summary mentions "the concrete default RPC host ethereum-rpc.publicnode.com"; leave as-is (it is a historical gate-3 note about the THEN-default) OR update the ratification to mention both the new default and the env override; choose whichever the builder's judgement prefers.

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

- [ ] `DEFAULT_RPC_ENDPOINT` (and its doc comment) is updated from `https://ethereum-rpc.publicnode.com` to `https://1rpc.io/eth` in `crates/werust-core/src/ethereum.rs`. The constant's name and public visibility stay unchanged.
- [ ] `docs/spikes/ethereum-provider-seam-and-trusted-rpc-backend/README.md:31` (and any other spike note that names the URL literally) is updated to the new default.
- [ ] `WERUST_RPC_URL` env var, when set and non-empty (whitespace-trimmed), overrides `DEFAULT_RPC_ENDPOINT` at session construction on desktop, Android, and iOS. When unset OR empty, the new default `https://1rpc.io/eth` is used.
- [ ] The override is read once per session (not per request), consistent with how `WERUST_VERSION` is read at build time.
- [ ] `.gitignore` excludes `.env`, `.env.local`, `.env.*.local`. `.env.example` is committed, real `.env` files are not.
- [ ] `.github/workflows/release.yml` passes the secret through to every Rust-compiling leg (desktop, android-apk, ios-simulator — three sites, NOT just one).
- [ ] `release_plumbing_shape.rs` covers both the WITH-secret and WITHOUT-secret paths so the test does not fail in PRs without the secret configured.
- [ ] Unit tests for the precedence (no env / non-empty env / empty env) — network-isolated, no live network, no env mutation.
- [ ] A short paragraph in the README's dev section explaining `source .env && cargo run` and `.env.example` as the workflow.

## Blocked by

- None.

## Prompt

> Goal: change `DEFAULT_RPC_ENDPOINT` from `https://ethereum-rpc.publicnode.com` to `https://1rpc.io/eth` AND add a `WERUST_RPC_URL` env var that overrides the default when set and non-empty. Two coordinated changes in one task: the privacy/egress-default review nit the prior drive captured is the human trigger; the failure-shape on `publicnode.com` (DNS-blocked by home routers, captive-portal TLS issues) is the technical trigger. A fresh build works without configuration (default = `1rpc.io/eth`); private endpoints like `https://your-private-rpc.example.com/` are opt-in via env / secrets, never committed.
>
> Where to look: `crates/werust-core/src/ethereum.rs:82` (`DEFAULT_RPC_ENDPOINT` constant) and `:77` (the doc comment that names the URL literally) — both update. `:359` (`RpcProvider::new` -> `Self::with_endpoint(DEFAULT_RPC_ENDPOINT)`) — wire `rpc_endpoint()` instead. `RpcProvider::with_endpoint(&str)` is the existing public override — add a small private helper `fn rpc_endpoint() -> String` that reads `WERUST_RPC_URL` and falls back to the constant. `.gitignore` for `.env*` exclusion (now closed by chore `f064bef`; confirm). `.github/workflows/release.yml` to wire the secret (mirror the `WERUST_VERSION` injection across all THREE Rust-compiling legs: desktop, android-apk, ios-simulator). `.env.example` (new, committed). `crates/werust-core/tests/release_plumbing_shape.rs` (existing test) — extend to cover both the with-secret and without-secret paths. Unit tests on the precedence: no env / non-empty env / empty env — network-isolated, do not mutate the env. Doc-comment refresh: `docs/spikes/ethereum-provider-seam-and-trusted-rpc-backend/README.md:31`.
>
> Out of scope: do not add an env lever for the trustless gateway (different task), do not fix the settings-dir persistence gap (separate observation), do not add a runtime `.env` loader (use the `source .env && cargo run` shell workflow, document it).