---
title: review-gate non-blocking nits for 'macos-release-packaging-leg' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: macos-release-packaging-leg
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'macos-release-packaging-leg' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify decision 1: a new werust-core EXAMPLE (crates/werust-core/examples/print_version.rs) is introduced as the version-readout seam, and bundle-app.sh + check-macos-app-bundle.sh both shell out to cargo run -q -p werust-core --example print_version. This is a cross-task interaction: the backlog task windows-release-packaging-leg carries the same no-second-version-source clause and is expected to reuse this readout rather than mint a third path. Is the example (rather than, say, a werust-core bin or a shared shell helper) the shape you want every packaging leg to depend on?
  (docs/spikes/macos-release-packaging-leg/README.md decision 1; crates/werust-macos/bundle-app.sh step 4/4; work/tasks/backlog/windows-release-packaging-leg.md)
- Ratify decision 2: CFBundleIdentifier is com.github.wighawag.werust, the GTK APP_ID_STEM WITHOUT the version element the GTK shell appends, and deliberately NOT the iOS pattern (com.github.wighawag.werust.shell). Verified: APP_ID_STEM in crates/werust/src/main.rs:71 is exactly that string, so the stem claim is true. The rationale (a macOS bundle id must be stable across releases or macOS treats each release as a new app) is sound, but this id is hard to change later without orphaning user preferences and Gatekeeper decisions, so it is worth an explicit human yes.
  (crates/werust-macos/bundle-app.sh BUNDLE_ID; crates/werust/src/main.rs:71; spike README decision 2)
- Conceptual coherence: the word unsigned is doing two jobs. The task means no Developer-ID signature and no notarization, but the new test macos_desktop_leg_is_deliberately_unsigned asserts codesign appears NOWHERE in the job or in bundle-app.sh, which also forbids AD-HOC signing (codesign -s -). Ad-hoc signing is not Developer-ID signing: on Apple Silicon an arm64 Mach-O must carry at least an ad-hoc signature to execute at all. cargo/ld64 ad-hoc-signs the arm64 slice at link time, but whether that signature survives lipo -create intact is not asserted anywhere and has never been run. If the lipo'd bundle will not launch on Apple Silicon, the one-line fix (codesign -s - Werust.app) trips this test. Should the absence assertion be narrowed to the Developer-ID/notarization tools (notarytool, altool, stapler, codesign with a real identity) and ad-hoc explicitly permitted?
  (crates/werust-core/tests/release_plumbing_shape.rs, macos_desktop_leg_is_deliberately_unsigned, loop over codesign/notarytool/altool/stapler)
- Nothing in this leg has ever executed. Both darwin slices building, lipo, plutil, the bundle actually opening: all first-run on the macos-14 runner. The spike README also states plainly that the WINDOW half of werust-macos could not be type-checked against EITHER darwin target, because the typecheck-macos-from-linux.sh harness is broken by the desktop-paint extraction (already tasked as macos-spike-doc-accuracy-and-harness-guard item 0). The leg is decoupled so a failure only withholds the macOS artifact, but recommend firing the workflow_dispatch dry-run once before the next tag rather than discovering it on a release.
  (docs/spikes/macos-release-packaging-leg/README.md, What is proven by what + What still awaits a Mac)
- The macOS signing/notarization follow-on is named in four places (task body, release.yml header, README, windows-release-packaging-leg) but has no file in work/tasks/backlog/, unlike the Android side where android-apk-signing existed as a real task first. The agent captured this correctly as an observation, but an observation is not a backlog item, so the gap persists. Cut the task?
  (work/notes/observations/macos-signing-notarization-follow-on-has-no-task-2026-07-31.md; ls work/tasks/backlog shows no macos signing item)
- README accuracy nit: the open-it instructions lead with right-click then Open, which macOS 15 (Sequoia) removed as a Gatekeeper bypass for unsigned apps (the path there is System Settings, Privacy and Security, Open Anyway). The second option given, xattr -d com.apple.quarantine, still works everywhere, so no user is stranded, but the first bullet will be wrong for anyone on a current OS.
  (README.md, section The macOS release artifact (Werust.app, UNSIGNED))
