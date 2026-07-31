#!/usr/bin/env bash
# Build the macOS desktop shell as a UNIVERSAL, unsigned `Werust.app` bundle
# (task `macos-release-packaging-leg`, `docs/adr/0011-webview2-for-windows.md`'s
# macOS split sub-task 4).
#
# `macos-appkit-window-and-chrome` shipped a window a person can use; this is the
# step that hands it to them. It:
#
#   1. builds `werust-macos` in RELEASE for BOTH desktop darwin targets
#      (`x86_64-apple-darwin` + `aarch64-apple-darwin`),
#   2. `lipo`s the two slices into ONE universal binary, so a single download
#      runs natively on Intel and Apple Silicon,
#   3. wraps it in a minimal `Werust.app` bundle whose `Info.plist` carries
#      `CFBundleName`, `CFBundleIdentifier`, `CFBundleExecutable`,
#      `CFBundleVersion` and `CFBundlePackageType=APPL`.
#
# UNSIGNED and UNNOTARIZED, deliberately: both need an Apple Developer account
# and are a separate follow-on (the macOS analogue of the landed
# `android-apk-signing` leg). Nothing here runs `codesign` or `notarytool`; the
# artifact is named `-unsigned` so the Release page never implies otherwise, and
# the README says how to open it.
#
# THE VERSION IS READ, NEVER RE-DERIVED. `CFBundleVersion` is the output of
# `werust-core`'s `print_version` example, i.e. `werust_core::version()`: the
# ONE version `build.rs` resolves (`WERUST_VERSION` from the release tag, else
# `git describe`) and the ⋮ menu inside the app reports. Re-deriving it in shell
# here would mint the second source that `android-apk-version-from-the-release-tag`
# exists to undo. Decisions: docs/spikes/macos-release-packaging-leg/README.md.
#
# The direct twin of `crates/werust-ios/build-and-run.sh` (BUILD_ONLY path): it
# lives in the crate, not in CI, so a human on a Mac runs the SAME packaging the
# release runs. The release workflow's `macos-desktop-app` job calls it, then runs
# docs/spikes/macos-release-packaging-leg/check-macos-app-bundle.sh over the
# result; the workflow shape is pinned by
# crates/werust-core/tests/release_plumbing_shape.rs.
#
# Usage:
#   crates/werust-macos/bundle-app.sh          # -> crates/werust-macos/build/Werust.app
#   BUILD_DIR=/tmp/out crates/werust-macos/bundle-app.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BUILD_DIR="${BUILD_DIR:-$REPO_ROOT/crates/werust-macos/build}"
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
APP="$BUILD_DIR/Werust.app"

# The binary crate's name is also the bundle's executable name.
EXECUTABLE="werust-macos"
# The bundle's display name (`Werust.app`, the Finder name).
BUNDLE_NAME="Werust"
# The SAME reverse-DNS stem the GTK shell uses (`APP_ID_STEM` in
# crates/werust/src/main.rs), WITHOUT the version element that id appends: a GTK
# app id is versioned on purpose (stale-process detection), but a macOS bundle
# identifier must be STABLE across releases or macOS treats every release as a
# different application.
BUNDLE_ID="com.github.wighawag.werust"

fail() { echo "FAIL: $*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || fail "this packages a macOS .app and needs a Mac (lipo + the darwin SDKs).
      On Linux the shell's host-independent half is covered by the verify gate; the
      bundle itself is built by the release workflow's macos-desktop-app job."

cd "$REPO_ROOT"

echo "== 1/4 install both desktop darwin Rust targets =="
rustup target add x86_64-apple-darwin aarch64-apple-darwin

echo "== 2/4 build both slices (release) =="
# Only `-p werust-macos`: the rest of the workspace (`werust`, `webview-renderer`)
# is GTK/WebKitGTK-bound and does not build here.
cargo build -p werust-macos --release --target x86_64-apple-darwin
cargo build -p werust-macos --release --target aarch64-apple-darwin

echo "== 3/4 lipo the two slices into ONE universal binary =="
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
lipo -create -output "$APP/Contents/MacOS/$EXECUTABLE" \
  "$TARGET_DIR/x86_64-apple-darwin/release/$EXECUTABLE" \
  "$TARGET_DIR/aarch64-apple-darwin/release/$EXECUTABLE"
lipo -info "$APP/Contents/MacOS/$EXECUTABLE"

echo "== 4/4 write the minimal Info.plist =="
# The ONE version source, READ back out of the compiled core (see the header).
VERSION="$(cargo run -q -p werust-core --example print_version)"
[ -n "$VERSION" ] || fail "the werust-core print_version example produced no version"
echo "version: $VERSION"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>$BUNDLE_NAME</string>
	<key>CFBundleIdentifier</key>
	<string>$BUNDLE_ID</string>
	<key>CFBundleExecutable</key>
	<string>$EXECUTABLE</string>
	<key>CFBundleVersion</key>
	<string>$VERSION</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
</dict>
</plist>
PLIST

echo "bundled: $APP (universal, unsigned, version $VERSION)"
