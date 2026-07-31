---
title: review-gate non-blocking nits for 'android-enable-dom-storage-and-guard-web-platform-parity' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: android-enable-dom-storage-and-guard-web-platform-parity
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'android-enable-dom-storage-and-guard-web-platform-parity' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The per-cell evidence guard is weaker than the prose claims: every_cell_in_the_web_storage_row_names_the_evidence_that_backs_it only asserts the marker EVIDENCE (platform): exists somewhere in the row, so a future change could flip macos to implemented while leaving its EVIDENCE (macos): NONE paragraph intact and stay green. Should the guard also couple the marker to the cell state, or should the README/test prose stop claiming a cell cannot be flipped without writing down what measured it?
  (crates/werust-core/tests/web_storage_edge_wiring_shape.rs (every_cell_... loops PLATFORMS asserting row.contains(EVIDENCE (platform):)) vs docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/README.md Decision 7 and the test's own doc comment)
- RATIFY Decision 6: stubbed is stretched from ADR-0005's meaning (a known gap, the matrix face of a no-op'd seam method) to not established / unmeasured for macos, windows and ios. Three cells will now read to a release reader as real capability gaps when nothing suggests a defect. Ratify the stretch, or mint an unmeasured state (guard + ADR + glossary together) and pin the term in CONTEXT.md?
  (docs/platform-capability-matrix.toml web-storage row; docs/adr/0005-platform-capability-parity-guard.md; README.md Decision 6 records the stretch and rejects a fourth state as out of scope for this task)
- RATIFY Decision 4, a cross-task interaction: the Ubuntu gate now asserts BrowserActivity.kt sets NONE of the seven audited WebSettings, so a future UX task that enables pinch-zoom or the wide viewport reds cargo test until it also edits WEBSETTINGS-AUDIT.md and this test. Deliberate friction, but it constrains tasks this one does not own.
  (web_storage_edge_wiring_shape.rs the_websettings_audit_is_recorded_and_changed_nothing asserts !activity.contains(setting) for builtInZoomControls, displayZoomControls, setSupportZoom, useWideViewPort, loadWithOverviewMode, mediaPlaybackRequiresUserGesture, textZoom)
- RATIFY Decision 1's desktop cell: it stays implemented on a SITE-LEVEL field report (mandalas.eth worked on GTK) rather than a property read-back, while three edges are stubbed. The handoff called either choice defensible provided it is stated, and it is stated in-cell and in the follow-on; a human should still confirm the line is drawn where they want it.
  (docs/platform-capability-matrix.toml EVIDENCE (desktop) paragraph; README.md Decision 1; follow-on acceptance criterion to upgrade or restate the desktop cell)
- Bookkeeping residue: the task lands in work/tasks/done/ still carrying needsAnswers: true, and work/questions/task-android-enable-dom-storage-and-guard-web-platform-parity.md still shows allAnswered=false with five identical unanswered stuck questions whose subject this branch has now resolved. Should the sidecar and the gate axis be cleared as part of landing, so a done item does not read as unanswered?
  (work/tasks/done/android-enable-dom-storage-and-guard-web-platform-parity.md frontmatter; work/questions/task-android-enable-dom-storage-and-guard-web-platform-parity.md header comment)
- Minor framing inconsistency: the Kotlin comment and the shape test's header both state as fact that the other four edges have web storage on by default, which is exactly why the gap was Android-only, while the matrix row (correctly) disclaims an engine default as evidence on the ipfs:// origins those edges serve. Worth aligning the prose so the next reader does not take the default as the evidence the row just refused.
  (BrowserActivity.kt WHY THE DEFAULT IS WRONG HERE block; web_storage_edge_wiring_shape.rs module doc; contrast with the EVIDENCE (macos)/(windows) paragraphs in docs/platform-capability-matrix.toml)
