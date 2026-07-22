#!/usr/bin/env bash
# The Xcode "Build Rust static lib" build phase (task
# mobile-ios-shell-and-static-lib, criterion 2): cross-compile the werust Rust
# core into `libwerust_mobile.a` for the iOS SDK + arch Xcode is currently
# building, as a NORMAL build step — the load-bearing piece of the Zig-less build
# experiment (ADR-0002). Xcode runs this phase BEFORE Compile Sources, so the
# Swift target links the freshly-built archive (wired via OTHER_LDFLAGS
# `-force_load .../libwerust_mobile.a`). It is the direct twin of wezig's
# `build-zig-lib.sh`, swapping `zig build ios-lib` for `cargo build --target`.
#
# Simulator only for this task: device/store builds need signing (out of scope).
# The device triple is handled too so the phase is total, but the app is only
# ever built for `iphonesimulator` here.
set -euo pipefail

# This script lives at crates/werust-ios/App/, so ../../.. is the repo root
# (which is the Cargo workspace root the werust-ios-core crate belongs to).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

CRATE="werust-ios-core"
LIB="libwerust_mobile.a"

# The arch Xcode is building for. Prefer the single active arch (ONLY_ACTIVE_ARCH
# in Debug); fall back to the first of ARCHS. Simulator on Apple Silicon is arm64.
ARCH="${CURRENT_ARCH:-}"
if [ -z "$ARCH" ] || [ "$ARCH" = "undefined_arch" ]; then
  ARCH="$(echo "${ARCHS:-arm64}" | awk '{print $1}')"
fi

# Map (PLATFORM_NAME, arch) -> the Rust target triple. PLATFORM_NAME is
# `iphonesimulator` or `iphoneos`.
PLATFORM_NAME="${PLATFORM_NAME:-iphonesimulator}"
case "$PLATFORM_NAME" in
  iphonesimulator)
    [ "$ARCH" = "arm64" ] && RUST_TARGET="aarch64-apple-ios-sim" || RUST_TARGET="x86_64-apple-ios"
    ;;
  iphoneos)
    RUST_TARGET="aarch64-apple-ios"
    ;;
  *)
    echo "build-rust-lib.sh: unsupported PLATFORM_NAME='$PLATFORM_NAME'" >&2
    exit 1
    ;;
esac

# Xcode's build-phase PATH is minimal; make sure cargo/rustup are reachable.
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
if ! command -v cargo >/dev/null 2>&1; then
  echo "build-rust-lib.sh: 'cargo' not found on PATH; install the Rust toolchain." >&2
  exit 1
fi

echo "== werust: build Rust static lib =="
echo "platform: $PLATFORM_NAME  arch: $ARCH  rust-target: $RUST_TARGET"

# Add the target on the runner if missing (idempotent), then cross-compile.
rustup target add "$RUST_TARGET" >/dev/null 2>&1 || true
( cd "$REPO_ROOT" && cargo build --release -p "$CRATE" --target "$RUST_TARGET" )

BUILT="$REPO_ROOT/target/$RUST_TARGET/release/$LIB"
if [ ! -f "$BUILT" ]; then
  echo "build-rust-lib.sh: expected $BUILT after cargo build" >&2
  exit 1
fi

# Stage the archive at the STABLE path the Xcode project links via -force_load
# (target/ios-lib/libwerust_mobile.a), so OTHER_LDFLAGS is arch-agnostic and the
# per-arch build above is the single source of the linked archive.
DEST_DIR="$REPO_ROOT/target/ios-lib"
mkdir -p "$DEST_DIR"
cp "$BUILT" "$DEST_DIR/$LIB"
file "$DEST_DIR/$LIB" || true
echo "== werust: Rust static lib ready at $DEST_DIR/$LIB =="
