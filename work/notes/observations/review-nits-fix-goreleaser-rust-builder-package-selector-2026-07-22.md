---
title: review-gate non-blocking nits for 'fix-goreleaser-rust-builder-package-selector' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: fix-goreleaser-rust-builder-package-selector
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'fix-goreleaser-rust-builder-package-selector' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the recorded in-scope decision: the desktop build sets flags [--release, --package=werust], adding --release even though the task example showed --package=werust alone. Correct and required, not a scope creep. Ratify or reverse.
  (DECISIONS.md documents it; verified against GoReleaser source internal/builders/rust/build.go: WithDefaults only sets --release when flags is empty, and Build copies from target/<triple>/release/, so an explicit flags list without --release would build a debug binary and leave nothing to package.)
