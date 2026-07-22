#!/usr/bin/env bash
# Build + launch the werust iOS shell on an iOS 17 Simulator from the REAL Xcode
# project (task mobile-ios-shell-and-static-lib, spec story 18). `xcodebuild`
# drives `crates/werust-ios/App/WerustShell.xcodeproj`, whose "Build Rust static
# lib" build phase cross-compiles the werust Rust core into `libwerust_mobile.a`
# as a NORMAL build step and links it into the Swift app. The direct twin of
# wezig's `mobile/ios/build-and-run.sh`, swapping the Zig lib for the Rust lib.
# Designed to run on a GitHub macos-14 runner (CI is the Mac; no physical Mac) OR
# locally on a Mac with Xcode. NO code signing / no Apple Developer account —
# Simulator only.
#
# Steps: build the app for the iOS Simulator with `xcodebuild` (which runs the
# Rust-lib build phase, then compiles + links the Swift shell); boot an iOS 17
# Simulator; install; launch; assert the Rust-linked greeting reached the log.
#
# BUILD_ONLY=1 stops after the .app is built (used by the release workflow to
# package the Simulator app without booting/launching a simulator, and by the
# BUILD-leg check `docs/spikes/mobile-ios-shell-and-static-lib/check-app-bundle.sh`).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
IOS_DIR="$REPO_ROOT/crates/werust-ios"
APP_DIR="$IOS_DIR/App"
PROJECT="$APP_DIR/WerustShell.xcodeproj"
SCHEME="WerustShell"
BUILD_DIR="${BUILD_DIR:-$IOS_DIR/build}"
DERIVED="$BUILD_DIR/DerivedData"
BUNDLE_ID="com.github.wighawag.werust.shell"

# macos-14 / Xcode 15.4 caps at iOS 17; the deployment-target FLOOR is iOS 16.0.
echo "== iOS shell build (real Xcode project) =="
xcodebuild -version || true

rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

# 1. Build the app for the iOS Simulator via the real project. The project's
#    pre-build "Build Rust static lib" phase cross-compiles libwerust_mobile.a for
#    the simulator SDK/arch and links it (OTHER_LDFLAGS -force_load); no signing.
# Pin a SINGLE architecture (the runner's native arch) for BOTH the link and the
# per-arch "Build Rust static lib" phase. Without this, `xcodebuild build` with no
# `-destination` resolves ARCHS to every simulator arch (arm64 + x86_64 on Apple
# Silicon) and links one, while the script phase sees CURRENT_ARCH=undefined_arch
# and builds another — an architecture mismatch (wezig hit this exact bug).
# ONLY_ACTIVE_ARCH + an explicit ARCHS forces one arch end-to-end and makes
# CURRENT_ARCH concrete for build-rust-lib.sh.
HOST_ARCH="$(uname -m)"   # arm64 on the Apple-Silicon macos-14 runner
echo "== 1/4 xcodebuild (runs the Rust-lib build phase, then Swift); arch: $HOST_ARCH =="
xcodebuild \
  -project "$PROJECT" \
  -scheme "$SCHEME" \
  -configuration Debug \
  -sdk iphonesimulator \
  -derivedDataPath "$DERIVED" \
  ONLY_ACTIVE_ARCH=YES ARCHS="$HOST_ARCH" \
  CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
  build

APP="$(find "$DERIVED/Build/Products" -maxdepth 2 -name 'WerustShell.app' -type d | head -1)"
if [ -z "$APP" ]; then
  echo "FAIL: WerustShell.app not produced by xcodebuild." >&2
  exit 1
fi
echo "built: $APP"

# Copy the built .app to a stable location the release workflow zips + the
# BUILD-leg check inspects.
cp -R "$APP" "$BUILD_DIR/WerustShell.app"
APP="$BUILD_DIR/WerustShell.app"
ls -la "$APP"

# BUILD_ONLY: stop here with the built .app (release-workflow packaging path).
if [ "${BUILD_ONLY:-0}" = "1" ]; then
  echo "BUILD_ONLY set: .app built at $APP; skipping simulator boot/launch."
  exit 0
fi

# 2. Boot an iOS 17 simulator (newest runtime present on the runner).
echo "== 2/4 boot simulator =="
RUNTIME="$(xcrun simctl list runtimes | grep -m1 'iOS 17' | grep -oE 'com.apple.CoreSimulator.SimRuntime.iOS-17[^ ]*' || true)"
if [ -z "$RUNTIME" ]; then
  echo "no iOS 17 runtime found; available runtimes:"; xcrun simctl list runtimes
  exit 1
fi
echo "runtime: $RUNTIME"
DEVTYPE="$(xcrun simctl list devicetypes | grep -m1 'iPhone 15 ' | grep -oE 'com\.apple\.CoreSimulator\.SimDeviceType\.[A-Za-z0-9-]+' | head -1)"
[ -z "$DEVTYPE" ] && DEVTYPE="com.apple.CoreSimulator.SimDeviceType.iPhone-SE-3rd-generation"
UDID="$(xcrun simctl create werust-ios17 "$DEVTYPE" "$RUNTIME")"
echo "device: $UDID"
xcrun simctl boot "$UDID"
xcrun simctl bootstatus "$UDID" -b

# 3. Install + launch; assert the Rust-linked greeting reached the device log.
echo "== 3/4 install + launch =="
xcrun simctl install "$UDID" "$APP"
xcrun simctl launch --console-pty "$UDID" "$BUNDLE_ID" > "$BUILD_DIR/launch.log" 2>&1 &
LAUNCH_PID=$!
sleep 8
xcrun simctl spawn "$UDID" log show --last 30s --predicate 'eventMessage CONTAINS "werust: linked Rust core"' --style compact > "$BUILD_DIR/werust.log" 2>&1 || true
kill "$LAUNCH_PID" 2>/dev/null || true

echo "---- launch.log ----"; cat "$BUILD_DIR/launch.log" || true
echo "---- werust.log ----"; cat "$BUILD_DIR/werust.log" || true

# 4. Verdict: the app launched and the Rust core is linked + callable.
echo "== 4/4 verdict =="
if grep -q "werust: linked Rust core" "$BUILD_DIR/launch.log" "$BUILD_DIR/werust.log" 2>/dev/null; then
  echo "PASS: iOS shell launched from the real Xcode project and the Rust core is linked + callable."
else
  echo "FAIL: did not observe the Rust-core greeting from the launched app."
  xcrun simctl shutdown "$UDID" || true
  exit 1
fi

xcrun simctl shutdown "$UDID" || true
echo "iOS shell (real Xcode project) GREEN."
