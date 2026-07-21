# Spike: the `Fetcher` seam over a bound HTTP+TLS stack (TLS never hand-written)

Durable evidence + decisions for task `fetcher-seam-bound-http-tls-stack` (spec story 8).

## What was built

- The `Fetcher` seam (trait) in `crates/fetcher/src/lib.rs`: `Fetcher::fetch(&self, url) -> Result<Response, FetchError>`. `Response` carries the response bytes plus a little metadata (status, `Content-Type`, the final URL after redirects); `FetchError` is the seam's error type (`InvalidUrl`, `Tls`, `Transport`, `Io`). The rest of werust fetches ONLY through this trait.
- `HttpFetcher`, the real implementation, binds `ureq` (a small synchronous HTTP client) whose TLS backend is **rustls**. TLS is delegated entirely to the bound stack; nothing hand-written.
- Seam-contract tests against a controlled local loopback endpoint (no live-network dependency): a successful HTTP fetch of bytes + metadata, a non-2xx status returned (not raised) as data, non-`http(s)` URL rejection, a TLS-handshake failure surfaced as a seam error, and a broken/closed-response transport failure surfaced as a seam error.

## Reproducing

```sh
cargo test -p fetcher
```

All six seam-contract tests run headless against a `127.0.0.1:0` loopback server the test itself stands up and tears down (no internet).

## Decisions

- **Bind rustls (via `ureq`), not libcurl.** The task allowed either. rustls was chosen because the project thesis is to stand on the mature *pure-Rust* stack (`CONTEXT.md`, `docs/adr/0001`): it needs no C toolchain at the TLS edge (keeping the build Rust-only, consistent with `docs/adr/0002`'s Zig-less/Rust-only aim), it is a vetted memory-safe TLS implementation, and `ureq`'s SYNCHRONOUS surface matches this seam and werust's other seams (the `Renderer` seam is sync) with no async runtime dragged in. libcurl (via `curl`/`curl-sys`) was the rejected alternative: it would reintroduce a C dependency and a system-library/vendoring surface for the single most security-sensitive component, against the pure-Rust thesis. Touches: only the `fetcher` crate's dependency set; `ureq` does not leak past the seam (no other crate depends on it).
- **Non-2xx HTTP status is `Ok`, not a `FetchError`.** `ureq` is configured with `http_status_as_error(false)`, so a reachable server answering `404`/`500` yields a `Response` with that status and body; the caller decides meaning. Rationale: fetching a URL that answers `404` still SUCCEEDED as a fetch; folding it into the error type would force every caller to special-case status recovery. Alternative considered: raise non-2xx as an error (ureq's default) — rejected as the wrong layer for a byte-fetch seam. This is a user-visible seam-contract default, so it is recorded here and in the `HttpFetcher::new` / `fetch` doc comments.
- **A safe DEFAULT trust store; pinning POLICY deferred.** `HttpFetcher` uses ureq's default rustls config (public webpki roots, real cert + host verification) — a working, safe default. The durable TLS trust-store / pinning policy (custom roots, cert pinning, whether content-addressed fetches relax origin trust because verification moves to the hash) is an OPEN QUESTION on the exploration spec `rust-successor-native-renderer-architecture-benchmark` and is deliberately NOT finalized here, per the task prompt.
- **Bounded connect/global timeouts as safe defaults.** `HttpFetcher` sets a 10 s connect timeout and a 30 s global timeout so a fetch to an unreachable/silent host surfaces promptly as a `FetchError` instead of hanging. This is a user-visible default; noted here and at the choice site.

## Notes

- **Sandbox loopback quirk (build environment only, not a product bug).** In the build sandbox a TCP `connect` to a *closed* loopback port does NOT get an immediate RST; it hangs on SYN-retransmit for ~134 s before erroring. So the transport-failure test does NOT rely on connect-to-a-dead-port; it uses a server that accepts then closes without a response (a fast, deterministic premature-EOF). Pinned to `docs/spikes` because it will bite any future networking test written the naive way. Also captured as an observation note.
- Pinned to `ureq = "=3.3.0"` (with its default `rustls` + `rustls-webpki-roots` features) to match the repo's exact-version pinning house style.
