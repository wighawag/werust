# macOS packaging: the universal, unsigned `Werust.app` release leg

Task: `macos-release-packaging-leg`. Decision it executes: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md), the "how `macos-desktop-build` should be split" block, sub-task 4 ("the CI packaging leg on the existing `macos-14` runner: lipo, `.app`, unsigned zip"). Window it ships: [`macos-appkit-window-and-chrome`](../macos-appkit-window-and-chrome/README.md).

**Read this first.** This was WRITTEN on Linux. Nothing below has been run on a Mac yet: the leg's proof is the `macos-desktop-app` job on the `macos-14` runner, and the human-judgement items are listed under [What still awaits a Mac](#what-still-awaits-a-mac-stated-plainly). Where a claim is checked by the Ubuntu `verify` gate it says so.

## What landed

- **`crates/werust-macos/bundle-app.sh`**: the packaging step. It adds both darwin Rust targets, builds `werust-macos` in release for `x86_64-apple-darwin` and `aarch64-apple-darwin`, `lipo`s the two slices into ONE universal binary, and writes `Werust.app/Contents/{MacOS/werust-macos,Info.plist}` with the minimal key set (`CFBundleName`, `CFBundleIdentifier`, `CFBundleExecutable`, `CFBundleVersion`, `CFBundlePackageType=APPL`). It lives in the CRATE, not in CI, so a human on a Mac runs exactly what the release runs (the `crates/werust-ios/build-and-run.sh` precedent).
- **`check-macos-app-bundle.sh`** (this directory): the BUILD-leg acceptance check, the macOS twin of the Android `check-apk-abis.sh` and the iOS `check-app-bundle.sh`: bundle layout, the five `Info.plist` keys, a Mach-O executable, `lipo -archs` showing BOTH `x86_64` and `arm64`, and `CFBundleVersion` equal to the version the compiled Rust core reports. It runs on the runner, because `lipo` and `plutil` cannot run in the Linux gate.
- **`crates/werust-core/examples/print_version.rs`**: a one-line readout of `werust_core::version()`, so a packaging SCRIPT can read werust's ONE version instead of computing its own (decision 1).
- **`.github/workflows/release.yml`**: a fourth leg, `macos-desktop-app`: `needs: verify` only, `runs-on: macos-14`, `WERUST_VERSION` + `WERUST_RPC_URL` injected exactly as the other legs inject them, bundle → check → zip, then an idempotent `gh release create` + `gh release upload` on a tag, or an `actions/upload-artifact` on the `workflow_dispatch` dry-run.
- **`crates/werust-core/tests/release_plumbing_shape.rs`**, criterion 9: the leg's shape is pinned from Linux (job key, runner, decoupling, both triples, `lipo`, the plist keys, the version readout, the ABSENCE of any signing tool, and the README's unsigned instructions). Network-isolated: it only reads files in this repo.

## Decisions

**1. `CFBundleVersion` is READ from the compiled core, never re-derived.** The bundle script runs `cargo run -q -p werust-core --example print_version`, i.e. it prints `werust_core::version()`, the value `crates/werust-core/build.rs` already resolved from `WERUST_VERSION` (the release tag) or `git describe`. *Alternative considered and rejected:* re-implement "tag, else `git describe`, strip the leading `v`" in shell. It is three lines and it is exactly the SECOND version source `version()`'s own docs forbid and that the sibling task `android-apk-version-from-the-release-tag` exists to undo: a readout cannot drift from what the app reports, a re-derivation can. *What it touches:* it adds a new `werust-core` EXAMPLE (build tooling, not a product surface, so it stays out of the crate's public shape and out of every shipped artifact), and `windows-release-packaging-leg` should reuse the same readout rather than mint a third path. `check-macos-app-bundle.sh` asserts the equality, so the "one source" claim is checked on every run rather than merely intended.

**2. `CFBundleIdentifier` is `com.github.wighawag.werust`, the GTK app-id STEM without the version element.** The GTK shell appends a version element to its application id on purpose (stale-process detection, `versioned-gtk-app-id-and-stale-process-detection`). A macOS bundle identifier must be the opposite: STABLE across releases, or macOS treats every release as a different application (separate preferences, separate Gatekeeper decisions, no in-place replacement). Same name, different rule, deliberately. *Alternative considered:* mirror iOS's `com.github.wighawag.werust.shell`, rejected because this is the desktop browser itself, not a shell around it, and the desktop identity already has a name.

**3. The artifact is NAMED unsigned: `Werust-macos-universal-unsigned.app.zip`.** The Android leg's honest-naming precedent (`app-debug-unsigned.apk`): nothing on the Release page may imply a signature the artifact does not carry. *What it touches:* the signing follow-on. When it lands it should attach a signed bundle under a name that says so, exactly as the Android leg attaches `app-release.apk` beside the debug one, and gate on a secrets-PRESENCE env flag with a graceful no-op, and NOT on the `secrets` context, which a step `if:` cannot read.

**4. `CFBundleVersion` carries the resolved version VERBATIM, including a `git describe` suffix on non-tag builds.** On a tag it is a clean `0.2.9`; on the dispatch dry-run it is something like `0.2.9-3-gabc1234`. *Alternative considered and rejected:* normalise to a numeric triple (the shape the Android `versionCode` mapping needs). Rejected because nothing in this artifact's path requires it (`CFBundleVersion` is only strictly validated for App Store submission and for some signed/notarized flows, and neither applies to an unsigned direct download), while the correspondence with the version the ⋮ menu reports is the property this task was asked to preserve. *What it touches:* the signing/notarization follow-on. If notarization ever rejects a non-numeric `CFBundleVersion`, that is the task that must add a normalisation (and it should then also set `CFBundleShortVersionString`), NOT this one.

**5. A SIBLING job (`macos-desktop-app`), not an extension of `ios-simulator-app`.** They share the `macos-14` runner shape and nothing else. Extending the iOS job would have made an iOS Simulator build failure withhold the macOS desktop artifact, the exact coupling `fix-release-native-x86-desktop-and-decouple-mobile` removed between the mobile legs and the desktop-Linux leg. The two jobs now run in parallel on the same runner image with separate cargo caches.

**6. The bundling script lives in the crate, the acceptance check lives here.** `crates/werust-macos/bundle-app.sh` is the product's packaging step (a human on a Mac runs it), so it sits with the crate like `crates/werust-ios/build-and-run.sh`. `check-macos-app-bundle.sh` is an ACCEPTANCE artifact for this task, so it sits in this spike directory like the Android and iOS BUILD-leg checks.

**7. `CFBundleExecutable` is in the plist even though the task's key list did not name it.** Without it the bundle names no binary to launch and `Werust.app` opens nothing. This is a factual gap in a minimal plist, not a design choice, but the check script asserts it so the omission cannot come back.

## What is proven by what

| claim | proven by | where |
|---|---|---|
| the leg exists, is decoupled, and cannot block or be blocked | parsing `release.yml` | Ubuntu `verify` (`release_plumbing_shape.rs`) |
| both darwin triples are built and `lipo`'d | parsing `bundle-app.sh` | Ubuntu `verify` |
| the plist's minimal key set, `APPL`, the `Contents/MacOS` layout | parsing `bundle-app.sh` | Ubuntu `verify` |
| the version is read from the ONE source, not re-derived | parsing `bundle-app.sh` + the example | Ubuntu `verify` |
| no signing/notarization crept into the leg | absence assertions over the job + the script | Ubuntu `verify` |
| the README says the artifact is unsigned and how to open it | parsing `README.md` | Ubuntu `verify` |
| the binary really is universal (`x86_64` + `arm64` in one file) | `lipo -archs` on the built binary | `macos-desktop-app` job |
| `CFBundleVersion` really equals the version the core reports | `plutil` + the `print_version` example | `macos-desktop-app` job |
| the `x86_64-apple-darwin` slice type-checks at all | the engine half + its smoke were type-checked against `x86_64-apple-darwin` from Linux (the `typecheck-macos-from-linux.sh` harness with the target substituted). The WINDOW half could not be checked on either darwin target, because that harness was broken by the `desktop-paint` extraction, an already-tasked bug (`macos-spike-doc-accuracy-and-harness-guard`, item 0), not something this leg introduced. **Repaired 2026-07-31** by that task, which ran it clean on `aarch64-apple-darwin` including the window half; the `x86_64-apple-darwin` substitution above has not been re-run since. | local, Linux |

## What still awaits a Mac (stated plainly)

- **That the zip a user downloads actually opens.** Gatekeeper's System Settings → Privacy & Security → **Open Anyway** path and the `xattr -d com.apple.quarantine` path are documented from the platform's rules, not from a run. Nobody has double-clicked this bundle. (The right-click → Open path this line first named was removed as a Gatekeeper bypass for unsigned apps in macOS 15 Sequoia; the top-level `README.md` was corrected by `macos-spike-doc-accuracy-and-harness-guard`, item 4.)
- **Retina rendering.** The plist is minimal and does not set `NSHighResolutionCapable`. Binaries linked against a current SDK are high-resolution capable by default, so this is expected to be fine, but it has not been seen on a Retina display. If the window is blurry on a HiDPI Mac, that key is the first thing to add.
- **No app icon, no menu-bar app name beyond the executable name, no `CFBundleShortVersionString`.** Deliberately minimal, per the task. What that looks like in the Dock and in the About box is unverified.
- **The universal binary running on Intel.** The `macos-14` runner is Apple Silicon, so the `x86_64` slice is built and inspected but never EXECUTED by CI.

Please record what you saw, especially anything in this list, as a dated note in `work/notes/observations/`.
