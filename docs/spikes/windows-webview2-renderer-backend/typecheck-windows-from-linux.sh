#!/usr/bin/env bash
# Type-check the `#[cfg(windows)]` WebView2 backend from Linux, WITHOUT a Windows
# box and without a CI round trip.
#
# The Ubuntu `verify` gate never compiles `crates/windows-renderer/src/backend.rs`
# (it is `#[cfg(windows)]`), so a typo there would otherwise only surface on the
# `windows-renderer` CI leg, minutes later. `cargo-xwin` downloads the MSVC CRT +
# Windows SDK headers/import-libs and drives `clang-cl`/`lld-link`, which is
# enough to COMPILE the crate for `x86_64-pc-windows-msvc` from here.
#
# It is a TYPE-CHECK, not a build and certainly not a test: nothing links a real
# WebView2 loader, nothing runs, and no WebView2 Runtime is involved. Treat the
# `windows-renderer` workflow as the actual verdict; this is the fast inner loop.
# (The macOS sibling `typecheck-macos-from-linux.sh` exists for the same reason.)
#
# Prerequisites (once):
#   rustup target add x86_64-pc-windows-msvc
#   cargo install cargo-xwin
#   apt-get install clang lld llvm      # `llvm-lib` is needed by `cc-rs` to
#                                       # archive ring's C code for this target
#
# On Debian the LLVM tools are not on PATH by default, so point at them:
#   LLVM_BIN=/usr/lib/llvm-19/bin ./typecheck-windows-from-linux.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$repo_root"

if [ -n "${LLVM_BIN:-}" ]; then
  PATH="$LLVM_BIN:$PATH"
  export PATH
fi

if ! command -v llvm-lib >/dev/null 2>&1; then
  echo "llvm-lib is not on PATH: ring's C sources cannot be archived for" >&2
  echo "x86_64-pc-windows-msvc without it. Install LLVM and set LLVM_BIN," >&2
  echo "e.g. LLVM_BIN=/usr/lib/llvm-19/bin $0" >&2
  exit 1
fi

exec cargo xwin check \
  -p windows-renderer \
  --target x86_64-pc-windows-msvc \
  --tests --examples "$@"
