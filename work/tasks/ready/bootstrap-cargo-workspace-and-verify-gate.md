---
title: Bootstrap the Cargo workspace and make the verify gate green
slug: bootstrap-cargo-workspace-and-verify-gate
blockedBy: []
covers: []
---

## What to build

Lay down the initial Rust project so the repo builds and the `dorfl.json` `verify`
gate (`cargo fmt --check && cargo clippy && cargo build && cargo test`) passes on a
clean tree. Create a Cargo workspace (the repo is currently a greenfield with no
`Cargo.toml`) with a top-level binary crate for the browser plus room for library
crates the seams will live in (renderer, fetcher, script-engine, native-renderer).
Add a `.gitignore` for Rust build output (`/target`). Add a minimal CI workflow
that runs the same gate so a tag/PR build can't ship a red tree. Include a
trivial passing test so `cargo test` is meaningful.

## Acceptance criteria

- [ ] A Cargo workspace exists with a runnable binary crate (`cargo run` starts and exits cleanly, even if it only prints a banner) and placeholder library crates for the seams.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` all pass green from a clean checkout.
- [ ] `/target` is gitignored; the tree is clean after a build.
- [ ] A CI workflow runs the identical `verify` gate.
- [ ] At least one real (if trivial) test exists and passes, mirroring the repo's chosen test style.

## Blocked by

- None — can start immediately.

## Prompt

> Goal: turn this greenfield repo into a building Rust workspace whose `verify`
> gate is green, so every downstream task has a foundation. There is no `Cargo.toml`
> yet — you are creating the project skeleton.
>
> Domain vocabulary (see `CONTEXT.md`): werust is a Rust web browser with a
> hot-swappable `Renderer` seam, plus `ScriptEngine` and `Fetcher` seams. Structure
> the workspace so those seams can become their own crates later (e.g. a `werust`
> binary crate + `renderer`, `fetcher`, `script-engine`, `native-renderer` library
> crates, or a sensible subset with clear module seams). Do NOT implement the seams
> here — just the crate structure + a clean build.
>
> The `verify` gate is `cargo fmt --check && cargo clippy && cargo build && cargo test`
> (from `dorfl.json`). Make ALL of it pass. Add a CI workflow running the same gate.
> Conventional-commit subjects are load-bearing for releases (see `CONTEXT.md`).
>
> Done = a clean checkout builds, the gate is green, CI runs it, and the workspace
> is shaped so the seam crates have obvious homes.
