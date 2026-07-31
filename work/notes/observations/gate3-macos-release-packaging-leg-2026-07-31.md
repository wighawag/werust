---
title: "Gate-3 verdict: macos-release-packaging-leg (APPROVE) — and the conductor fired the dry run the spike said had never happened"
date: 2026-07-31
status: open
reviewOf: macos-release-packaging-leg
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main`. A fourth release leg (`macos-desktop-app` on `macos-14`), the crate-side `bundle-app.sh`, the spike-side `check-macos-app-bundle.sh`, a `print_version` example as the single version readout, and 339 lines added to `release_plumbing_shape.rs` to pin the shape from Linux.

## I measured the thing the spike admitted was unmeasured

The spike README stated plainly, to its credit, that **nothing in this leg had ever executed**: both darwin slices, `lipo`, `plutil`, the bundle itself — all first-run, and the Gate-2 reviewer's recommendation was to fire the `workflow_dispatch` dry run "rather than discovering it on a release". That is a conductor action, so I did it rather than merging on a promise.

**[Run 30594040744](https://github.com/wighawag/werust/actions/runs/30594040744), `release.yml`, `workflow_dispatch` on `main`: ALL FIVE JOBS SUCCESS** — `verify`, `goreleaser`, `android-apk`, `ios-simulator-app` and the new `macos-desktop-app`. Four artifacts uploaded, including **`werust-macos-desktop-app`, 4,184,625 bytes**. So the leg is not merely well-shaped: it really does add both darwin targets, build release for each, `lipo` them into one universal binary, write the bundle, pass `check-macos-app-bundle.sh` (which asserts the layout, the five plist keys, a Mach-O executable, `lipo -archs` showing BOTH `x86_64` and `arm64`, and `CFBundleVersion` equal to what the compiled core reports) and upload the artifact. Criteria 1, 2 and 3 are therefore MEASURED, not argued.

It also confirmed the decoupling is real: five jobs, one runner image shared with iOS, no leg waiting on another beyond `verify`.

## Criteria, ticked against the merged diff

1. **A tagged release attaches a macOS artifact alongside the existing ones.** MET. The tag path is `gh release create` (idempotent) + `gh release upload`, copied from the mobile legs; the dispatch path uploads a workflow artifact, which is what I exercised. The tag branch itself remains unexercised until a real tag, which is inherent.
2. **Universal, both architectures verified in the job.** MET and measured: `lipo -archs` asserts both slices, in the run above.
3. **Version from the existing source; no second source.** MET, and the design is the best available: `bundle-app.sh` runs `cargo run -q -p werust-core --example print_version`, i.e. it READS `werust_core::version()` rather than re-deriving "tag, else `git describe`" in shell. That re-derivation is exactly the second version source `android-apk-version-from-the-release-tag` exists to undo, and the check script asserts the plist equals the core's value on every run, so "one source" is checked rather than intended.
4. **`macos-14`, decoupled both ways, dry-run uploads without publishing.** MET, and confirmed by the run.
5. **Shape pinned by a `release_plumbing_shape.rs`-style test, network-isolated.** MET, including absence assertions (no signing tool crept in) and a test that the README's unsigned instructions exist.
6. **README says it is unsigned, how to open it, and names the signing follow-on.** MET, with one factual error (nit 6 below).

## Review-nit triage (6 raised, all non-blocking)

**Acted on:**

- **The signing/notarization follow-on was named in four places and existed in none.** ACTED: I cut `work/tasks/backlog/macos-app-signing-and-notarization.md`. This was the right nit to act on rather than file, because the Android side proved the pattern: `android-apk-signing` was a real task BEFORE anything referenced it, and it got built. A deferral referenced everywhere and tracked nowhere is how a release ships unsigned forever. It carries `needsAnswers: true` deliberately: it cannot be built without an Apple Developer account and its secrets, and that is the human's call, not an agent's.
- **The `deliberately unsigned` guard is over-broad, and it may block the fix for a bug nobody has hit yet.** REAL, and carried into that new task. `macos_desktop_leg_is_deliberately_unsigned` forbids `codesign` ANYWHERE, but ad-hoc signing (`codesign -s -`) is not Developer-ID signing, and on Apple Silicon an arm64 Mach-O needs at least an ad-hoc signature to execute. `ld64` ad-hoc-signs the arm64 slice at link time; whether that survives `lipo -create` is asserted nowhere and has never been run. So the bundle may not launch on Apple Silicon, and the one-line fix trips a green test. Nobody can settle it without a Mac; the fix and the narrowed assertion are now owned.
- **The README's Gatekeeper instructions lead with right-click -> Open, which macOS 15 (Sequoia) REMOVED for unsigned apps.** ACTED: planted as item 4 of `macos-spike-doc-accuracy-and-harness-guard`, which is the doc-accuracy task I am driving next, so it lands with its siblings.

**Ratified, no action:**

- **The `print_version` EXAMPLE as the version-readout seam.** Ratified as the shape every packaging leg should reuse (`windows-release-packaging-leg` carries the same no-second-source clause). An example is build tooling: it stays out of the crate's public shape and out of every shipped artifact, which a `bin` would not. Raised to the human as a ratify item because it becomes a cross-task convention.
- **`CFBundleIdentifier = com.github.wighawag.werust`** — the GTK app-id STEM without the version element, deliberately NOT the iOS `.shell` pattern. The reasoning is right and non-obvious (the GTK id is versioned ON PURPOSE for stale-process detection; a macOS bundle id must be the opposite, stable across releases, or macOS treats every release as a new app with separate preferences and Gatekeeper decisions). Verified: `APP_ID_STEM` really is that string. Raised to the human anyway, because a bundle id is expensive to change later — it orphans user preferences.
- **Nothing in the leg had ever executed.** No longer true; see above.

## An off-path finding I filed while reviewing

`goreleaser-leg-is-not-idempotent-on-rerun-2026-07-31.md`: checking this leg's precedents surfaced that **the last real release, `v0.2.9`, is RED** — both its runs failed on `goreleaser`, with `422 already_exists` on `checksums.txt` and the Linux tarball. The first attempt uploaded them; every re-run is then guaranteed to fail on its predecessor's assets. The repo deliberately made the mobile legs (and now this macOS leg) idempotent for exactly this reason; the Linux desktop leg is the one that did not get that treatment, and it is the one that is red. Filed, not fixed: it belongs to whoever owns the release path. It matters because a permanently-red release run stops being read, which is how the next real failure gets missed.
