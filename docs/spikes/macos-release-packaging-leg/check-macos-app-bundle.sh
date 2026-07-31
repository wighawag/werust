#!/usr/bin/env bash
# BUILD-leg check: assert the packaged `Werust.app` is a real, UNIVERSAL app
# bundle reporting the release version (task `macos-release-packaging-leg`,
# criteria 1-3).
#
# The macOS twin of the Android `check-apk-abis.sh` and the iOS
# `check-app-bundle.sh`, and a SEPARATE leg from the repo's pure-Rust `verify`
# gate for the same reason they are: `lipo` and `plutil` are macOS tools, and the
# bundle only exists once both darwin slices have been built. The release
# workflow's `macos-desktop-app` job builds the bundle with
# `crates/werust-macos/bundle-app.sh` and then runs THIS over the result; the
# workflow shape itself is pinned from Linux by
# `crates/werust-core/tests/release_plumbing_shape.rs`.
#
# It asserts:
#   1. the path is a `.app` bundle directory with the `Contents/MacOS` layout,
#   2. its `Info.plist` declares the minimal key set (`CFBundleName`,
#      `CFBundleIdentifier`, `CFBundleExecutable`, `CFBundleVersion`,
#      `CFBundlePackageType=APPL`),
#   3. the declared executable exists and is a Mach-O,
#   4. that executable is UNIVERSAL: BOTH `x86_64` and `arm64` slices are in it
#      (criterion 2: "verified with `lipo -info` or equivalent in the job"),
#   5. `CFBundleVersion` is EXACTLY the version the compiled Rust core reports,
#      which is what makes "no second version source" a checked fact rather than
#      an intention (criterion 3).
#
# Usage:
#   check-macos-app-bundle.sh [path/to/Werust.app]
# With no argument it defaults to bundle-app.sh's stable output path.
# Set WERUST_EXPECTED_VERSION to skip the cargo run in assertion 5 (it is the
# same value bundle-app.sh stamped, so a caller that already has it can pass it).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$here/../../.." && pwd)"
default_app="$repo_root/crates/werust-macos/build/Werust.app"
app="${1:-$default_app}"

fail() { echo "FAIL: $*" >&2; exit 1; }

plist_value() {
  plutil -extract "$1" raw -o - "$plist" 2>/dev/null || fail "Info.plist has no $1"
}

# 1. The bundle directory + layout.
[[ -d "$app" ]] || fail "app bundle not found: $app
      Build it first: crates/werust-macos/bundle-app.sh"
case "$app" in
  *.app) : ;;
  *) fail "not a .app bundle: $app" ;;
esac
[[ -d "$app/Contents/MacOS" ]] || fail "missing $app/Contents/MacOS (a .app is a LAYOUT, not a renamed directory)"
echo "ok: app bundle $app"

# 2. The Info.plist and its minimal key set.
plist="$app/Contents/Info.plist"
[[ -f "$plist" ]] || fail "missing $plist"
command -v plutil >/dev/null 2>&1 || fail "plutil not found (this check runs on macOS)"
bundle_name="$(plist_value CFBundleName)"
bundle_id="$(plist_value CFBundleIdentifier)"
exe_name="$(plist_value CFBundleExecutable)"
bundle_version="$(plist_value CFBundleVersion)"
package_type="$(plist_value CFBundlePackageType)"
[[ -n "$bundle_name" ]] || fail "empty CFBundleName"
[[ -n "$bundle_id" ]] || fail "empty CFBundleIdentifier"
[[ "$package_type" == "APPL" ]] || fail "CFBundlePackageType is '$package_type', expected APPL"
echo "ok: Info.plist declares $bundle_name ($bundle_id), executable $exe_name, version $bundle_version, type $package_type"

# 3. The executable exists and is a Mach-O.
exe="$app/Contents/MacOS/$exe_name"
[[ -f "$exe" ]] || fail "missing app binary: $exe"
if command -v file >/dev/null 2>&1; then
  file "$exe" | grep -qi "Mach-O" || fail "app binary is not a Mach-O: $exe"
fi
echo "ok: app binary present ($exe)"

# 4. It is UNIVERSAL: both architectures in one binary.
command -v lipo >/dev/null 2>&1 || fail "lipo not found (this check runs on macOS)"
lipo -info "$exe"
archs="$(lipo -archs "$exe")"
for arch in x86_64 arm64; do
  # Word-boundary match: `lipo -archs` prints a space-separated list.
  [[ " $archs " == *" $arch "* ]] || fail "the app binary is NOT universal: missing the $arch slice (has: $archs)"
done
echo "ok: universal binary (architectures: $archs)"

# 5. The bundle version is the version the compiled core reports: the ONE
#    source. Any drift here means a second version source crept in.
expected="${WERUST_EXPECTED_VERSION:-$(cd "$repo_root" && cargo run -q -p werust-core --example print_version)}"
[[ -n "$expected" ]] || fail "could not resolve the expected version from werust-core"
[[ "$bundle_version" == "$expected" ]] || fail "CFBundleVersion is '$bundle_version' but the Rust core reports '$expected'
      The bundle version must come from the SAME source the core resolves; a
      second source has crept in (see docs/spikes/macos-release-packaging-leg/README.md)."
echo "ok: CFBundleVersion == the version the Rust core reports ($expected)"

echo "PASS: $app is a real, universal, correctly-versioned app bundle."
