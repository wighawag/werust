#!/usr/bin/env bash
# MEASURE, from Linux, which Windows SUBSYSTEM `werust-windows.exe` is linked as.
#
# The console window this task closed is a property of the BINARY, not of any
# code path: no test can execute it, the `windows-latest` runner has no desktop
# to see it on, and the Ubuntu `verify` gate never links a Windows binary at all.
# What CAN be checked anywhere is the flag rustc hands the linker, and that flag
# IS the behaviour: `/SUBSYSTEM:windows` is what makes Windows allocate no
# console for the process.
#
# So this script asks rustc for the link line of the real `werust-windows` bin
# target, cross-compiled for `x86_64-pc-windows-msvc`, and checks the subsystem
# in it. It is the measurement behind the "no console window" claim in
# `README.md` -- reproducible, with a negative control (below), and it needs no
# Windows box.
#
# Measured 2026-07-31 on the change that added the attribute:
#
#   with `#![cfg_attr(windows, windows_subsystem = "windows")]` in `src/main.rs`
#       /SUBSYSTEM:windows /ENTRY:mainCRTStartup
#   NEGATIVE CONTROL, the same tree with the attribute commented out
#       neither flag is passed at all -- the MSVC linker then defaults to
#       /SUBSYSTEM:console, which is the console window the human saw.
#
# To re-run the negative control, comment the attribute out and run this again:
# it must FAIL. A guard that cannot fail measures nothing.
#
# The link itself does not complete here and is not meant to: embedding
# `app.manifest` needs `mt.exe`, which exists only on Windows. The link ARGS are
# printed before that, which is all this measurement needs. The actual linking is
# the `windows-desktop-app` release job's business.
#
# Prerequisites are the same as the sibling type-check harness
# (`docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh`):
#
#   rustup target add x86_64-pc-windows-msvc
#   cargo install cargo-xwin
#   apt-get install clang lld llvm      # then LLVM_BIN=/usr/lib/llvm-19/bin $0
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$repo_root"

if [ -n "${LLVM_BIN:-}" ]; then
  PATH="$LLVM_BIN:$PATH"
  export PATH
fi

# The link is EXPECTED to fail (no `mt.exe`), so the exit status is not the
# verdict here -- the printed link line is.
link_args="$(cargo xwin rustc \
  -p werust-windows --bin werust-windows \
  --target x86_64-pc-windows-msvc \
  -- --print link-args 2>&1 || true)"

subsystem="$(printf '%s' "$link_args" | tr '"' '\n' | grep -oE '/SUBSYSTEM:[A-Za-z]+' | sort -u | head -1 || true)"

if [ "$subsystem" = "/SUBSYSTEM:windows" ]; then
  echo "werust-windows.exe links as a GUI app: $subsystem (no console window)"
  exit 0
fi

echo "werust-windows.exe does NOT link as a GUI app." >&2
echo "expected /SUBSYSTEM:windows, rustc passed: ${subsystem:-<nothing, so the linker defaults to console>}" >&2
echo "Windows would allocate a console window beside the browser." >&2
exit 1
