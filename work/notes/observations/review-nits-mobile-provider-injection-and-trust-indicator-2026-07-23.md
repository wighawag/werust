---
title: review-gate non-blocking nits for 'mobile-provider-injection-and-trust-indicator' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: mobile-provider-injection-and-trust-indicator
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'mobile-provider-injection-and-trust-indicator' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Un-recorded in-scope decision to RATIFY: the mobile sessions scope mark_content_verified to ONLY the ipfs scheme, so werust://settings (routed through the same resolve_ipfs dispatch) is NOT marked content-verified. This is a sound, security-correct call (an internal chrome page must not paint the verified badge), covered by tests, but it is absent from the decisions.md block. Ratify.
  (crates/werust-android/rust/src/lib.rs (is_ipfs guard around mark_content_verified) + iOS twin; tests an_internal_werust_settings_page_is_not_marked_content_verified. Not listed in docs/spikes/.../decisions.md.)
- Ratify the platform asymmetry in provider-shim injection timing: iOS uses a true document-start WKUserScript, Android uses onPageStarted evaluateJavascript (no exact document-start hook without androidx). A page whose own inline head script races the injection could see window.ethereum late on Android.
  (Recorded as Decision 4 / sub-decision 4a with an androidx WebViewCompat.addDocumentStartJavaScript follow-up pointer; acceptable for the dev/simulator app.)
