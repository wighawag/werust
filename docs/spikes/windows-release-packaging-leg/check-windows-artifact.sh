#!/usr/bin/env bash
# BUILD-leg check: assert the packaged Windows artifact is a real `.exe` carrying
# the application MANIFEST and the ONE released version, and that the zip a user
# downloads really contains it (task `windows-release-packaging-leg`,
# criteria 1-3).
#
# The Windows twin of the Android `check-apk-abis.sh`, the iOS
# `check-app-bundle.sh` and the macOS `check-macos-app-bundle.sh`, and a SEPARATE
# leg from the repo's pure-Rust `verify` gate for the same reason they are: the
# `.exe` only exists once an MSVC toolchain has built it, and the Ubuntu gate has
# neither. The release workflow's `windows-desktop-app` job builds the binary and
# then runs THIS over the result; the workflow's SHAPE is pinned from Linux by
# `crates/werust-core/tests/release_plumbing_shape.rs`.
#
# Written in bash (run with `shell: bash` on the Windows runner, where Git Bash
# provides it) rather than in PowerShell, for two reasons: it reads as the twin of
# the three checks it is modelled on, and it can be exercised from a Linux
# development box against a captured artifact.
#
# THE EXE CHECK asserts:
#   1. the path is a PE image (it starts with `MZ`),
#   2. the embedded manifest declares the comctl32 v6 dependency by its full
#      documented identity (name + version + public key token) — a wrong token
#      does not fail a build, it silently leaves the process on comctl32 5.82,
#   3. the embedded manifest declares per-monitor-v2 DPI awareness,
#   4. the exe contains the EXACT version string the compiled Rust core reports,
#      which is what makes "no second version source" a checked fact rather than
#      an intention. `werust_core::version()` is `env!("WERUST_VERSION")`, a
#      compile-time literal, so the released version is verbatim in the binary.
#
# THE ZIP CHECK (`--zip <path>`) asserts the archive exists, is a real zip, names
# itself UNSIGNED, and carries the `.exe` entry (a zip stores its entry names
# uncompressed, so this needs no unzip binary — which a Windows runner's bash
# does not have).
#
# What this check CANNOT do, stated plainly: judge the manifest's EFFECT. That
# the chrome now draws in the modern visual style, and how a DPI-aware werust
# looks on a 150%/200% display, are pixels — a human on a Windows box is the only
# instrument. Those steps live in
# `docs/spikes/windows-win32-window-and-chrome/README.md`.
#
# Usage:
#   check-windows-artifact.sh [path/to/werust-windows.exe]
#   check-windows-artifact.sh --zip path/to/werust-windows-x86_64-unsigned.zip
# With no argument it defaults to the release build's stable output path.
# Set WERUST_EXPECTED_VERSION to skip the cargo run in assertion 4 (it is the
# same value the build resolved, so a caller that already has it can pass it).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$here/../../.." && pwd)"
default_exe="$repo_root/target/x86_64-pc-windows-msvc/release/werust-windows.exe"

fail() { echo "FAIL: $*" >&2; exit 1; }

# A binary-safe substring search: `grep -a` reads the PE as text, so the manifest
# resource (which link.exe embeds as plain XML) and Rust's UTF-8 string literals
# are both findable without any Windows tooling.
contains() { LC_ALL=C grep -a -q -F -- "$2" "$1"; }

magic() { head -c "${#2}" "$1" | LC_ALL=C tr -d '\0'; }

check_zip() {
  local zip="$1"
  [[ -f "$zip" ]] || fail "zip not found: $zip"
  [[ "$(magic "$zip" PK)" == "PK" ]] || fail "not a zip archive: $zip"
  case "$(basename "$zip")" in
    *unsigned*) : ;;
    *) fail "the attached zip must be NAMED unsigned (nothing on the Release page may imply a
      signature this artifact does not carry): $zip" ;;
  esac
  contains "$zip" "werust-windows.exe" \
    || fail "the zip carries no \`werust-windows.exe\` entry: $zip"
  echo "ok: $zip is a zip, named unsigned, carrying werust-windows.exe"
  echo "PASS: the attached Windows zip carries the browser."
}

if [[ "${1:-}" == "--zip" ]]; then
  [[ -n "${2:-}" ]] || fail "--zip needs a path"
  check_zip "$2"
  exit 0
fi

exe="${1:-$default_exe}"

# 1. A real PE image.
[[ -f "$exe" ]] || fail "exe not found: $exe
      Build it first: cargo build -p werust-windows --release --target x86_64-pc-windows-msvc"
[[ "$(magic "$exe" MZ)" == "MZ" ]] || fail "not a PE executable (no MZ header): $exe"
echo "ok: PE executable $exe"

# 2/3. The embedded application manifest, declaration by declaration.
for declaration in \
  "Microsoft.Windows.Common-Controls" \
  "6.0.0.0" \
  "6595b64144ccf1df" \
  "PerMonitorV2"; do
  contains "$exe" "$declaration" \
    || fail "the exe has no embedded manifest declaring '$declaration'.
      The manifest is embedded by crates/werust-windows/build.rs, which passes
      /MANIFEST:EMBED + /MANIFESTINPUT: to the MSVC linker. Without it the chrome
      draws pre-Vista style and the process is DPI-unaware
      (docs/spikes/windows-release-packaging-leg/README.md)."
done
echo "ok: the application manifest is embedded (comctl32 v6 + per-monitor-v2 DPI awareness)"

# 4. The ONE version source: the exe carries exactly what the core reports.
expected="${WERUST_EXPECTED_VERSION:-$(cd "$repo_root" && cargo run -q -p werust-core --example print_version)}"
[[ -n "$expected" ]] || fail "could not resolve the expected version from werust-core"
contains "$exe" "$expected" || fail "the exe does not carry the version the Rust core reports ('$expected').
      The shipped binary's version must come from the SAME source the ⋮ menu
      reads (WERUST_VERSION, resolved by crates/werust-core/build.rs); a second
      source is what lets an artifact and its menu disagree."
echo "ok: the exe carries the version the Rust core reports ($expected)"

echo "PASS: $exe is a real, manifested, correctly-versioned werust build."
