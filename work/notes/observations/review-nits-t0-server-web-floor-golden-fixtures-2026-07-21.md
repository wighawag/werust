---
title: review-gate non-blocking nits for 't0-server-web-floor-golden-fixtures' (Gate 2 approve)
date: 2026-07-21
status: open
reviewOf: t0-server-web-floor-golden-fixtures
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 't0-server-web-floor-golden-fixtures' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the new public css API surface introduced without a Decisions block: SUPPORTED_PROPERTIES, is_supported_property, is_supported_selector. These are a new user-visible/machine-readable companion to tree::ELEMENT_ALLOWLIST. Sensible and covered by a test that asserts the allowlist matches parse_declaration, but it is an in-scope design choice the task did not name.
  (crates/native-renderer/src/css.rs new pub const + two pub fns; commit message has no Decisions block.)
- The golden transcript captures bold/italic/underline marks but NOT colour, yet transcribe's doc-comment claims it captures colour and the fixtures are colour-heavy (universal *, .class colour, inline colour, cascade order). A colour-cascade regression on these fixtures would not turn any golden red. Pre-existing paint limitation, but the guard under-covers what the fixtures appear to exercise and the doc overclaims.
  (crates/native-renderer/src/paint.rs transcribe(): only bold/italic/underline marks; article/headings/inline-styles fixtures declare colour rules never asserted.)
- Ratify: is_supported_selector deliberately does its own single-token validation instead of reusing parse_selector (which accepts malformed .class/#id). Recorded as an observation note. Correct call for a drift guard whose job is to reject, but it forks selector-validation logic that could later diverge from the cascade parser.
  (css.rs is_supported_selector; work/notes/observations/parse-selector-accepts-malformed-class-id.md)
