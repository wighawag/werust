---
title: Fix release goreleaser leg — pin a real Zig version for setup-zig
slug: fix-release-setup-zig-pin-version
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: []
covers: []
---

## What to build

Fix the release workflow's `goreleaser` desktop job. It uses `mlugg/setup-zig@v1` with
NO version pin, which defaults to `version: latest` and resolves to an UNRELEASED Zig
(the dispatch dry-run tried to fetch Zig `0.16.0`, which does not exist on any mirror →
all mirrors returned 404/503 → the step failed → the whole release run failed, and the
`android-apk` + `ios-simulator-app` jobs `needs: goreleaser` were skipped).

Zig is needed because GoReleaser's `builder: rust` cross-compiles with **cargo-zigbuild**
(Zig as the cross-linker) for the desktop Linux targets (`x86_64` + `aarch64`).

Observed in the workflow_dispatch dry-run (run 29900203491): `goreleaser` failed at
"Set up Zig (for cargo-zigbuild cross-compiles)" — `Fetching zig-linux-x86_64-0.16.0`,
then every mirror 404/503.

## What to change

In `.github/workflows/release.yml`, the `goreleaser` job's Zig step:

- Bump the action `mlugg/setup-zig@v1` -> `mlugg/setup-zig@v2` (v1 is what werust used;
  wezig uses v2).
- PIN a concrete, RELEASED Zig version instead of the implicit `latest`, e.g.:

  ```yaml
  - name: Set up Zig (for cargo-zigbuild cross-compiles)
    uses: mlugg/setup-zig@v2
    with:
      version: 0.14.1
  ```

  Use a real stable Zig that cargo-zigbuild supports (0.14.1 is a released stable at the
  time of writing; if the pinned cargo-zigbuild requires a specific Zig range, pick a
  released version inside it — do NOT use `latest`/`master`, which drift to unreleased
  dev tarballs that 404 on the mirrors).

- Confirm no OTHER `setup-zig` usage in the repo has the same unpinned/`v1` problem
  (grep `setup-zig` across `.github/workflows/`); if the ios/android legs don't use Zig
  (they use cargo-NDK / cargo-ios, not cargo-zigbuild), leave them.

## Acceptance criteria

- [ ] The `goreleaser` job pins a real released Zig version via `mlugg/setup-zig@v2` (no implicit `latest`).
- [ ] Every `setup-zig` use in `.github/workflows/` is `@v2` with a pinned released version (or removed if that leg does not need Zig).
- [ ] No change to `.goreleaser.yaml`'s builder config or the gate; this is a workflow-YAML-only fix.
- [ ] The pure-Rust `verify` gate this task runs under passes locally (no Rust code touched).

## Prompt

> Goal: unblock the release. The `goreleaser` desktop leg's `mlugg/setup-zig@v1` with no
> version pin fetched an unreleased Zig 0.16.0 that 404s on every mirror, failing the
> release run (and skipping the android/ios jobs that need it). Bump to
> `mlugg/setup-zig@v2` and pin a REAL released Zig version (e.g. `0.14.1`) that
> cargo-zigbuild supports. Fix every `setup-zig` use in `.github/workflows/`. YAML-only,
> no Rust changes, no `.goreleaser.yaml` builder changes.
>
> Done = the goreleaser leg pins a real Zig via setup-zig@v2, and a re-run of the release
> workflow_dispatch dry-run gets past "Set up Zig" and builds the desktop dist.
