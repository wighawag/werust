---
title: Fix .goreleaser.yaml rust builder — select the werust package in the workspace
slug: fix-goreleaser-rust-builder-package-selector
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: []
covers: []
---

## What to build

Fix the GoReleaser desktop build. On the release `workflow_dispatch` dry-run (run
29901439037) GoReleaser's rust builder failed:

```
release failed: build failed: you need to specify which workspace to build, please add
'--package=[name]' to your build flags, setting name to one of the available workspaces:
[crates/werust crates/werust-core crates/renderer crates/webview-renderer
 crates/native-renderer crates/fetcher crates/script-engine crates/werust-android/rust
 crates/werust-ios/rust]  target=x86_64-unknown-linux-gnu
```

Root cause: werust is a multi-crate Cargo workspace, and the `.goreleaser.yaml`
`builds[]` entry sets `binary: werust` but does NOT tell the rust builder WHICH package
to build, so cargo cannot disambiguate. (This is the exact CI-runtime risk flagged at
the release task's Gate-3: "no explicit package/dir set; resolution relies on the single
werust bin" — GoReleaser's rust builder does not auto-resolve it.)

## What to change

In `.goreleaser.yaml`, the `builds: - id: werust-desktop` entry: add the package
selector so cargo builds the `werust` binary crate. GoReleaser's rust builder passes
`flags` through to the cargo/zigbuild invocation, so:

```yaml
builds:
  - id: werust-desktop
    builder: rust
    binary: werust
    tool: cargo
    command: zigbuild
    flags:
      - --package=werust
    targets:
      - x86_64-unknown-linux-gnu
      - aarch64-unknown-linux-gnu
```

Use whatever the installed GoReleaser (v2.17) rust builder documents as the package
selector — `flags: [--package=werust]` is the cargo-native form the error message itself
suggests (`--package=[name]`). If GoReleaser's rust builder exposes a dedicated
`package:` key instead, prefer that; otherwise `flags` is correct. Verify against the
GoReleaser v2 rust-builder docs for the exact spelling.

## Acceptance criteria

- [ ] `.goreleaser.yaml`'s desktop build selects the `werust` package (via `flags: [--package=werust]` or the builder's `package:` key), so the rust builder no longer errors "you need to specify which workspace to build".
- [ ] No other `.goreleaser.yaml` change (targets, archives, changelog, builder stay as-is); no Rust code change.
- [ ] The pure-Rust `verify` gate this task runs under passes locally (YAML-only change).
- [ ] (Verification is at CI runtime — a re-run of the release `workflow_dispatch` dry-run should get PAST "building binaries" and produce the desktop dist. State this in the done record; it cannot be proven in the pure-Rust gate.)

## Prompt

> Goal: make GoReleaser's rust builder build the `werust` binary in the multi-crate
> workspace. It currently fails with "you need to specify which workspace to build ...
> add '--package=[name]'". Add the package selector to the `werust-desktop` build in
> `.goreleaser.yaml` (`flags: [--package=werust]`, or the GoReleaser v2 rust-builder's
> `package:` key if it has one). YAML-only; no Rust changes; leave targets/archives/
> changelog untouched. This is the CI-runtime resolution issue flagged at the release
> task's Gate-3.
>
> Done = `.goreleaser.yaml` selects the werust package, the gate is green locally, and a
> release dry-run gets past the "building binaries" step.
