---
title: Fix CI verify — install WebKitGTK/GTK4 system deps before cargo build
slug: fix-ci-verify-missing-webkitgtk-system-deps
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: []
covers: []
---

## What to build

Fix a real CI bug: the `verify` workflow (`.github/workflows/verify.yml`) and the
`verify` job in `.github/workflows/release.yml` both run `cargo build` (the whole
workspace) on a bare `ubuntu-latest` runner WITHOUT installing the GTK/WebKitGTK system
libraries. Since the `webview-renderer` crate (and the desktop `werust` binary) link
`webkit6`/`gtk4`/`glib` via `pkg-config`, `cargo build` fails on the runner with:

```
Package glib-2.0 was not found in the pkg-config search path.
The system library `glib-2.0` required by crate `glib-sys` was not found.
```

(Local builds pass because a dev machine already has these `.pc` files; the CI runner
does not.) Example failing run:
https://github.com/wighawag/werust/actions/runs/29897183326/job/88849718727

## What to change

Add an "Install system dependencies" step BEFORE `cargo build` (and before `cargo
clippy`, which also compiles) in BOTH workflows' verify path:

```yaml
- name: Install WebKitGTK/GTK4 system dependencies
  run: |
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends pkg-config libwebkitgtk-6.0-dev
```

`libwebkitgtk-6.0-dev` provides `webkitgtk-6.0.pc` and pulls in `libgtk-4-dev` +
the glib dev packages (`glib-2.0.pc`, `gio-2.0.pc`) as dependencies, which is the
full set `webkit6`/`gtk4`/`glib-sys` need. Confirm the exact package name resolves on
the Ubuntu the runner uses (`libwebkitgtk-6.0-dev` is correct on Ubuntu 24.04 / the
current `ubuntu-latest`; it owns `/usr/lib/x86_64-linux-gnu/pkgconfig/webkitgtk-6.0.pc`).

Apply the SAME step to both:
1. `.github/workflows/verify.yml` (the `verify` job) — before `cargo clippy`/`cargo build`.
2. `.github/workflows/release.yml` (the `verify` job) — same placement. (The `goreleaser`
   desktop leg cross-builds with cargo-zigbuild and ALSO links these libs — verify that
   leg builds too; if it compiles the webview crate for the Linux targets it will need
   the same deps installed on its runner, so add the step there as well if required.)

Keep the cargo cache step. Do NOT change the gate command itself (still `cargo fmt
--check && cargo clippy && cargo build && cargo test`).

## Acceptance criteria

- [ ] `verify.yml` installs `libwebkitgtk-6.0-dev` (+ pkg-config) before the compile steps, so `cargo build`/`cargo clippy`/`cargo test` succeed on `ubuntu-latest`.
- [ ] The `verify` job in `release.yml` gets the same fix (and the `goreleaser` desktop leg builds — add the system deps to it too if it compiles the webview/gtk-linking crates).
- [ ] The exact apt package name is correct for the runner's Ubuntu and actually provides `glib-2.0.pc` + `gtk4.pc` + `webkitgtk-6.0.pc` (via its dependency chain).
- [ ] No change to the verify gate COMMAND; `dorfl.json`'s `verify` string stays as-is (system-dep installation is a CI-runner concern, not part of the gate command).
- [ ] The pure-Rust `verify` gate this task runs under still passes locally (this task only edits CI YAML; it does not touch Rust code).

## Prompt

> Goal: make CI green. The `verify` workflow builds the whole workspace on a bare
> ubuntu runner but never installs the WebKitGTK/GTK4/glib system libraries the
> `webview-renderer` + `werust` crates link, so `cargo build` fails with `glib-2.0 not
> found`. Add a `sudo apt-get install -y --no-install-recommends pkg-config
> libwebkitgtk-6.0-dev` step BEFORE the compile steps in BOTH `.github/workflows/verify.yml`
> and the `verify` job of `.github/workflows/release.yml` (and the goreleaser desktop leg
> if it compiles those crates). `libwebkitgtk-6.0-dev` pulls in gtk4 + glib dev packages.
> Do not change the gate command. This is a CI-YAML-only fix — no Rust changes.
>
> NOTE for the reviewer/human (do NOT act on it in this task): werust's core `verify`
> gate builds the WHOLE workspace including the webkit-linking crate, which is why the
> gate itself needs these system libs. wezig deliberately keeps WebKitGTK OFF its core
> gate and runs webkit work in a DEDICATED CI leg (ADR-0007). Whether werust should adopt
> that split (feature-gate the webview backend out of the default gate build) is a
> separate ADR-level decision, not this hotfix.
>
> Done = both verify workflows install the system deps and CI's `cargo build`/`test`
> pass on ubuntu-latest.
