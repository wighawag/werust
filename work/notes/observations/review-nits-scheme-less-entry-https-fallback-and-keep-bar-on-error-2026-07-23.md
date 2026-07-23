---
title: review-gate non-blocking nits for 'scheme-less-entry-https-fallback-and-keep-bar-on-error' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: scheme-less-entry-https-fallback-and-keep-bar-on-error
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'scheme-less-entry-https-fallback-and-keep-bar-on-error' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the classifier treats an IPv6-literal host (e.g. [::1]:8080 or [2001:db8::1]) as Invalid because is_plausible_authority splits the port on the first colon and the bracketed host has no dot. Acceptable as a documented conservative limitation, or worth a follow-up?
  (crates/werust-core/src/lib.rs is_plausible_authority uses split_once(':') and a dotted-host rule; DECISIONS.md explicitly frames the classifier as pragmatic, not a full URL parser. No realistic user path hits IPv6 literals in this browser today.)
- Ratify recorded in-scope decision: the browser-idiomatic https:// (not http://) default for a scheme-less plausible host, and localhost accepted bare/with-port while other dotless tokens are refused. Both are load-bearing UX defaults, recorded in DECISIONS.md.
  (docs/spikes/scheme-less-entry-https-fallback-and-keep-bar-on-error/DECISIONS.md sections on the conservative classifier and the https default; matches Brave/Chrome/Firefox behaviour and the field finding's verbatim intent.)
