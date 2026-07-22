#!/usr/bin/env bash
# BUILD-leg check: assert the built debug APK carries the werust Rust core
# (libwerust_mobile.so) for BOTH floor ABIs (arm64-v8a + x86_64).
#
# This is the acceptance check "the APK carries the Rust core lib for both ABIs"
# from work/tasks/.../mobile-android-shell-and-static-lib.md. It is a SEPARATE
# leg from the repo's Rust `verify` gate (cargo fmt/clippy/build/test), because a
# Gradle/APK build needs the Android SDK+NDK and cannot run inside that pure-Rust
# gate. The release/mobile CI job (ADR-0002's hand-written mobile jobs) runs this
# after building the APK.
#
# Usage:
#   check-apk-abis.sh [path/to/app-debug.apk]
# With no argument it defaults to the app module's debug output.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$here/../../.." && pwd)"
default_apk="$repo_root/crates/werust-android/app/build/outputs/apk/debug/app-debug.apk"
apk="${1:-$default_apk}"

lib="libwerust_mobile.so"
required_abis=("arm64-v8a" "x86_64")

if [[ ! -f "$apk" ]]; then
  echo "FAIL: APK not found: $apk" >&2
  echo "      Build it first: (cd crates/werust-android && ./gradlew :app:assembleDebug)" >&2
  exit 1
fi

# The APK is a zip; list its entries once and grep the native-lib paths.
entries="$(unzip -Z1 "$apk")"

missing=0
for abi in "${required_abis[@]}"; do
  path="lib/$abi/$lib"
  if grep -qxF "$path" <<<"$entries"; then
    echo "ok: $path"
  else
    echo "FAIL: missing $path in $apk" >&2
    missing=1
  fi
done

if [[ "$missing" -ne 0 ]]; then
  echo "FAIL: the APK does not carry the Rust core for every floor ABI" >&2
  exit 1
fi

echo "PASS: $apk carries $lib for ${required_abis[*]}"
