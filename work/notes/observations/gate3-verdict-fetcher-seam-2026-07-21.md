---
title: Gate-3 (conductor) verdict — fetcher-seam-bound-http-tls-stack — APPROVE
date: 2026-07-21
kind: observation
reviewOf: fetcher-seam-bound-http-tls-stack
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main in --merge mode, commit b30252e)

Conductor's own diff-vs-criteria review. `do` ran Gate-1 + Gate-2, both green.

### Acceptance criteria — all met

- ✅ `Fetcher` trait + `HttpFetcher` impl over a BOUND HTTP+TLS stack: `ureq` with
  a **rustls** TLS backend. TLS is never hand-written; rustls chosen over libcurl
  with recorded rationale (pure-Rust thesis, no C toolchain at the TLS edge).
- ✅ No HTTP client leaks past the seam: no crate outside `fetcher` references
  ureq/reqwest — callers fetch only through the `Fetcher` trait.
- ✅ Failures surface as structured `FetchError` (InvalidUrl / Tls / Transport /
  Io), never panics.
- ✅ Seam-contract tests run against an in-process loopback HTTP server bound to
  `127.0.0.1:0` (ephemeral port, NO real network): 200 success, 404 non-2xx-is-Ok,
  invalid-url rejection, and error paths.

### Triage of the 4 non-blocking Gate-2 nits — all KEEP

1. rustls-via-ureq over libcurl — sound, pure-Rust thesis, contained to the crate.
2. Non-2xx returned as `Ok(Response)` (not an error) — reasonable for a byte-fetch
   seam; documented + tested.
3. 10s connect / 30s global timeouts — sensible default (fail promptly, no hang).
4. `final_url` redirect-following not exercised by a test — minor coverage gap,
   not a defect; not a task criterion.

### Deferred, correctly

The durable TLS trust-store / pinning POLICY is left open (module docs point at the
exploration spec) — not finalised here, as the task instructed.

### Notable captured signal (agent-filed, kept)

`sandbox-loopback-connect-to-closed-port-hangs.md`: in this build sandbox a
`TcpStream::connect` to a CLOSED loopback port hangs ~134s on SYN retransmit
instead of a fast connection-refused. The agent worked around it in the fetcher
tests. Useful for the next person writing loopback network tests (e.g. t0/t1
server-web-floor fixtures).

### What this unlocks

fetcher-seam landing unlocks `fetcher-hash-verified-content-addressed-path` (the
verified content-addressed path built on this `HttpFetcher`).
