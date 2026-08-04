#!/usr/bin/env bash
# BUILD-leg check: assert the built Simulator `.app` is a real app bundle carrying
# the app binary — the werust Rust core linked in (task
# mobile-ios-shell-and-static-lib, criterion 4: "the packaged `.app` contains the
# app bundle + binary").
#
# This is the iOS twin of the Android `check-apk-abis.sh`, and like it a SEPARATE
# leg from the repo's pure-Rust `verify` gate (cargo fmt/clippy/build/test),
# because building the `.app` needs Xcode + the iOS SDK and cannot run inside that
# gate. The mobile-ios CI job (macos-14) runs `build-and-run.sh BUILD_ONLY=1` to
# produce the `.app`, then runs this check; the release job packages the same
# `.app` (ADR-0002's hand-written mobile jobs).
#
# It asserts:
#   1. the path is a `.app` bundle directory,
#   2. it carries an `Info.plist` with the expected CFBundleExecutable,
#   3. that executable exists inside the bundle and is a real Mach-O,
#   4. the executable statically links the Rust core (the `werust_ios_*` C-ABI
#      symbols the Swift shell calls are present — proof the `-force_load`
#      static-lib link actually pulled the Rust core into the binary), INCLUDING
#      `werust_ios_activate_reload_stop_control`, the entry point the toolbar's
#      ONE reload/stop control (and therefore CANCEL) rides on since task
#      `ios-chrome-collapse-reload-stop-and-drop-history-buttons`. There is no
#      Mac on this project, so nobody can press that control by hand; this leg
#      building the Swift and finding the symbol in the shipped binary is the
#      only runtime-adjacent evidence the collapsed control exists at all.
#
# Usage:
#   check-app-bundle.sh [path/to/WerustShell.app]
# With no argument it defaults to build-and-run.sh's stable BUILD_ONLY output.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$here/../../.." && pwd)"
default_app="$repo_root/crates/werust-ios/build/WerustShell.app"
app="${1:-$default_app}"

exe_name="WerustShell"

fail() { echo "FAIL: $*" >&2; exit 1; }

# 1. The bundle directory.
[[ -d "$app" ]] || fail "app bundle not found: $app
      Build it first: (cd crates/werust-ios && BUILD_ONLY=1 ./build-and-run.sh)"
case "$app" in
  *.app) : ;;
  *) fail "not a .app bundle: $app" ;;
esac
echo "ok: app bundle $app"

# 2. The Info.plist + its declared executable name.
plist="$app/Info.plist"
[[ -f "$plist" ]] || fail "missing $plist"
if command -v plutil >/dev/null 2>&1; then
  exe_name="$(plutil -extract CFBundleExecutable raw "$plist" 2>/dev/null || echo "$exe_name")"
fi
echo "ok: Info.plist declares CFBundleExecutable=$exe_name"

# 3. The executable exists and is a Mach-O binary.
exe="$app/$exe_name"
[[ -f "$exe" ]] || fail "missing app binary: $exe"
if command -v file >/dev/null 2>&1; then
  file "$exe" | grep -qi "Mach-O" || fail "app binary is not a Mach-O: $exe"
fi
echo "ok: app binary present ($exe)"

# 4. The Rust core is linked in: its C-ABI exports are in the binary's symbols.
# `nm` is available in the Xcode/CLT toolchain on the macos runner. A static
# `-force_load` link keeps these symbols in the executable.
#
# The list is the SESSION handle plus the entry points behind the chrome controls
# a user can actually press, so a build that compiled but lost the core (or lost
# one of those controls' actions) fails here rather than shipping.
required_symbols=(
  _werust_ios_session_new
  # The ONE reload/stop control's action — and therefore CANCEL, which is that
  # same control in its Stop mode (task
  # ios-chrome-collapse-reload-stop-and-drop-history-buttons).
  _werust_ios_activate_reload_stop_control
)
if command -v nm >/dev/null 2>&1; then
  symbols="$(nm "$exe" 2>/dev/null || true)"
  for symbol in "${required_symbols[@]}"; do
    grep -q "$symbol" <<<"$symbols" \
      || fail "the app binary does not carry the Rust core C-ABI symbol $symbol"
  done
  echo "ok: the Rust core is linked (${#required_symbols[@]} werust_ios_* C-ABI symbols present)"
else
  echo "note: nm unavailable; skipped the Rust-core symbol check"
fi

echo "PASS: $app is a real app bundle carrying the app binary with the Rust core linked."
