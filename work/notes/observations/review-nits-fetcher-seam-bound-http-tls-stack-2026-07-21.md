---
title: review-gate non-blocking nits for 'fetcher-seam-bound-http-tls-stack' (Gate 2 approve)
date: 2026-07-21
status: open
reviewOf: fetcher-seam-bound-http-tls-stack
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'fetcher-seam-bound-http-tls-stack' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: bind rustls via ureq rather than libcurl. Task allowed either; agent picked pure-Rust rustls to avoid a C toolchain at the TLS edge, consistent with the pure-Rust thesis and adr/0002. Sound and reversible (contained to the fetcher crate).
  (docs/spikes/fetcher-seam-bound-http-tls-stack/README.md Decisions; crates/fetcher/Cargo.toml)
- Ratify user-visible default: a non-2xx HTTP status is returned as Ok(Response) with the status, not raised as a FetchError (http_status_as_error(false)). Reasonable for a byte-fetch seam; documented and tested.
  (HttpFetcher::new; test non_2xx_status_is_returned_not_raised_as_an_error)
- Ratify user-visible defaults: 10s connect / 30s global timeouts so an unreachable host fails promptly instead of hanging. Recorded at the choice site and in the spike README.
  (DEFAULT_CONNECT_TIMEOUT / DEFAULT_GLOBAL_TIMEOUT in crates/fetcher/src/lib.rs)
- final_url is set from response.get_uri() and the tests assert it equals the request URL only in the no-redirect case; redirect-following behaviour (ureq default) is claimed in docs but not exercised by a test. Minor coverage gap, not a defect.
  (Response.final_url; module docs mention 'final URL after redirects')
