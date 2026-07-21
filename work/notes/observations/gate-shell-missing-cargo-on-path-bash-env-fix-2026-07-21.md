---
title: dorfl verify gate can't find `cargo` — non-interactive gate shell drops ~/.cargo/bin; fixed via BASH_ENV
date: 2026-07-21
kind: observation
tags: [infrastructure, toolchain, verify-gate, rust, dorfl]
---

## What happened

The first `dorfl do task:bootstrap-cargo-workspace-and-verify-gate --isolated --review --merge`
build committed its work but the **acceptance gate failed with exit 127**:

```
bash: line 1: cargo: command not found
>> Bounced ... to stuck (lock): acceptance gate failed (exit 127) on the rebased tip —
   the failing step was: `cargo fmt --check && cargo clippy && cargo build && cargo test`
```

This was routed to needs-attention (lock `state: stuck`, branch preserved). It is an
**environment/toolchain problem, NOT a code bug and NOT a human-decision block.**

## Root cause

- Rust IS installed on this machine via rustup: `~/.cargo/bin/cargo` → `cargo 1.97.1`,
  with `rustfmt`, `clippy`, `rustc` all present.
- `~/.cargo/env` (which prepends `~/.cargo/bin` to PATH) is sourced by `~/.bashrc:117`
  and `~/.profile:28` — i.e. only in **login / interactive** shells.
- dorfl runs the verify command in a **non-interactive, non-login `bash -c`**, which
  reads neither `.bashrc` nor `.profile`, so `~/.cargo/bin` is absent from PATH and
  `cargo` is not found. `which cargo` fails in `bash -c` but succeeds in `bash -lc`.

## Fix (no task-source edits, no git dance)

Export `BASH_ENV` pointing at the rustup env script before invoking `dorfl do`, so
every non-interactive `bash -c` the gate spawns sources it and gets `cargo` on PATH:

```sh
export BASH_ENV="$HOME/.cargo/env"
```

Verified: `BASH_ENV="$HOME/.cargo/env" bash -c 'cargo --version'` → `cargo 1.97.1`.

This is a conductor infrastructure fix applied for the whole drive session. It does
not require requeue `--reset` (the kept branch's code is fine); a plain `requeue`
(keep + continue) + re-`do` with `BASH_ENV` set lets the gate finally run against the
already-built tree.

## Follow-up worth considering (out of scope for this drive)

A durable fix belongs on the environment/CI side, e.g. a machine profile that puts
`~/.cargo/bin` on PATH for non-interactive shells, or dorfl invoking the gate under a
login shell. Filed as an observation, not tasked here.
