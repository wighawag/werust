---
title: review-gate non-blocking nits for 'native-renderer-t0-subset-path-behind-seam' (Gate 2 approve)
date: 2026-07-21
status: open
reviewOf: native-renderer-t0-subset-path-behind-seam
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'native-renderer-t0-subset-path-behind-seam' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- RATIFY: the T0 backend navigates ONLY data:text/html and REJECTS every fetch-requiring scheme (http(s)/ipfs) with InvalidUrl. Recorded with rationale in work/notes/observations/native-renderer-t0-data-url-navigation.md. Good, coherent, reversible: keeps T0 from fail-open-claiming a fetch capability it lacks (that is stories 8/9/12). Ratify.
  (backend.rs navigate + decode_data_html; note documents alternatives considered)
- percent_decode maps + to a space (form-urlencoding semantics), which is NOT standard for data: URLs per RFC 2397. Low impact: the tests percent-encode + as %2B to sidestep it, and T0 fixtures are authored. Worth a comment or dropping the + handling to stay coherent with data: semantics.
  (backend.rs percent_decode: `if bytes[i] == b'+' { out.push(b' ') }`)
