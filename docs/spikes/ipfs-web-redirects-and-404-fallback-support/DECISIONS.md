# Decisions: the IPFS `_redirects` + custom-404 fallback (`ipfs-web-redirects-and-404-fallback-support`)

Durable record of the design choices this task made and of exactly WHICH SUBSET of IPIP-0002 landed (the task's "record exactly what subset landed"). Linked from the task done-record. Spec: `ens-to-ipfs-resolution-phase1-rpc-skeleton`. Spec being implemented: <https://specs.ipfs.tech/http-gateways/web-redirects-file/> (IPIP-0002). Builds on the verified retrieval path (`docs/adr/0004`, `docs/spikes/ipfs-per-resource-car-scope-not-whole-dag/DECISIONS.md`).

## Reality re-check (before building)

- FIRST re-check per the prompt: `crates/fetcher/src/retriever.rs::resolve_in_dag` DOES still return `RetrieveError::PathNotFound` for a path that does not resolve in the UnixFS DAG (a missing directory entry, a directory with no `index.html`, or a segment descending into a file), and `crates/werust-core/src/ipfs.rs::resolve_ipfs_request` DID surface that as a hard `RendererError::Backend` failed load. The task premise held exactly.
- Field shape re-confirmed live: `jolly-roger.eth`'s current root (`bafybeihasag6ramvwiiox2p7pgtx37wevyd5xqnhfoeu4w7b7ndfgozjyu`) serves a `_redirects` whose entire content is `/* /404.html/index.html 404`, and `jolly-roger.eth.limo/unknown` answers **HTTP 404 with the custom page body** (not a 200, not a redirect). So "serve the page AND report not-found" is the behaviour being matched.
- No drift found; the build proceeded.

## What LANDED (the supported subset)

| IPIP-0002 feature | Status |
| --- | --- |
| `_redirects` at the ROOT CID, evaluated ONLY when the path is absent from the DAG (§3.3) | **landed** |
| Grammar `from to [status]`, `\n`/`\r\n` lines, blank/whitespace tolerance, omitted status = 301 (§2, §2.4.3, §2.4.4) | **landed** |
| First-match-wins, top-to-bottom evaluation (§3.2) | **landed** |
| 64 KiB max file size (§2.4.5) | **landed** (over-size = fail-closed refusal) |
| `200` rewrite (SPA/PWA: serve the target AT the requested URL, no bar change) | **landed** |
| `404` custom error page, served **with a 404 status** (the jolly-roger case, incl. a directory-index target like `/404.html/index.html`) | **landed** |
| `410` / `451` error pages, served with their status | **landed** (same code path as 404) |
| `:placeholder` capture in `from` + injection in `to`, incl. repeated use in `to`; duplicate name in `from` is an error (§2.4) | **landed** |
| Trailing `*` catch-all with `:splat` injection (§2.4.1) | **landed** |
| Default root `404.html` convention (no `_redirects` needed) | **landed** |
| Error handling: a broken/unparseable `_redirects` surfaces as a failed load, never ignored (§3.4) | **landed** (as a fail-closed load failure, see Decision 4) |
| `301` / `302` / `303` / `307` / `308` **redirects** (a NAVIGATION) | **NOT landed HERE** — parsed, but a MATCHING one failed the load with a legible reason (Decision 3). **Since landed** by the follow-on `ipfs-redirects-3xx-navigation-support` (`docs/spikes/ipfs-redirects-3xx-navigation-support/DECISIONS.md`): a matching 3xx now navigates. |
| §3.5 query-parameter merging into a `Location` header | **NOT landed** (it only affects 3xx `Location`, which is not supported) |
| The spec's shared conformance fixture CIDs (§5.1) | **NOT used** (they need the live network; the fixtures here are synthesized offline — see Tests) |

## Decision 1 — the fallback lives at the `ipfs://` SEAM, not inside the retriever

**Chosen:** the fallback is implemented in `werust_core::ipfs::resolve_ipfs_request` (the `PathNotFound` branch), with the pure rule grammar/matching in a new `werust_core::redirects` module. `crates/fetcher`'s `ContentRetriever` / `resolve_in_dag` are **unchanged**.

**Why:** `ContentRetriever` is the trust boundary and its contract is exactly "given a CID + a path, return the verified bytes or a typed failure". `_redirects` is a *web-pathing* policy (which resource answers a request), not a retrieval mechanism, and it is inherently about the ROOT CID of a *site* — a notion the retriever deliberately does not have (it resolves a path under whatever CID it is handed). Putting the policy at the seam keeps the verify boundary single-purpose, keeps `PathNotFound` an honest primitive that other callers can still rely on, and means the fallback is expressed entirely in terms of ordinary `retrieve(cid, path)` calls — which is *why* it cannot bypass verification.

**Alternative considered:** doing it inside `resolve_in_dag` (rejected: it would make the retriever silently return a DIFFERENT resource than the one asked for, which is exactly the kind of "helpful" ambiguity a verify boundary must not have, and it would give a second meaning to `retrieve`'s return value).

**Touches:** nothing else consumes `PathNotFound` for control flow today; the mobile/desktop edges consume `resolve_ipfs_request`, so all three platforms get the behaviour from one place.

## Decision 2 — `SchemeResponse` gains a `status` (a seam widening), and each edge maps it

**Chosen:** `renderer::SchemeResponse` gained a `status: u16` field (with `SchemeResponse::ok(mime, body)` for the overwhelmingly common 200 case and a `renderer::STATUS_OK` constant). Each OS edge maps a non-200 status onto its platform response: desktop via `webkit6::URISchemeResponse::set_status` + `finish_with_response`; Android via the status-taking `WebResourceResponse` overload; iOS via an `HTTPURLResponse` on the `WKURLSchemeTask`. The mobile FFI grew one accessor each (`nativeResolutionStatus`, `werust_ios_resolution_status`).

**Why (a USER-VISIBLE, cross-platform choice, so it is recorded):** a gateway serves a site's custom 404 page **with a 404 status**, and the whole point of a custom 404 is that the page renders. werust could have served the page as a 200, but that would be werust *lying about* a page the site itself declared missing — a trust-honesty regression in a browser whose thesis is "never claim more than you verified". Since the status is a property of the resolution, it travels with the body on the seam rather than being reconstructed per platform.

**Deliberately NOT done:** `SchemeResponse` is **not** a redirect channel. A 3xx would be a navigation (a URL-bar and page-identity change), which belongs to the navigation path, not to answering an intercepted request in place; adding a `Location` to this struct would put navigation policy in the wrong layer.

**Touches:** every `SchemeResponse` construction site in the workspace (all migrated to `SchemeResponse::ok`, so no behaviour changed anywhere else), the two mobile FFI surfaces + their Kotlin/Swift wrappers, and the `werust://settings` page (unchanged: 200). The new matrix row `ipfs-web-pathing-fallback` in `docs/platform-capability-matrix.toml` records the three edges.

## Decision 3 — a matching 3xx rule is REFUSED, not skipped (what did not land)

**Chosen:** `301`/`302`/`303`/`307`/`308` rules are *parsed* (so an unrelated redirect line never breaks a file whose catch-all is what matters), but a rule of that kind that actually MATCHES fails the load with `RedirectsError::RedirectNotSupported` naming the status and the target.

**Why:** following a redirect means driving a NAVIGATION (updating the URL bar and the identity/trust the bar describes) from inside a scheme-resolution callback, which the current seam cannot express — the resolver returns a response, it cannot start a load. The scoping guidance allowed deferring redirects; the real decision is what to do when one matches. Silently *skipping* to the next rule would serve a page the site's author never named for that path (with a `/*` catch-all present, which is the common shape, the user would silently get the 404 page instead of the intended target — a wrong-but-plausible render, the worst outcome). Failing with a legible reason is the honest, fail-closed answer and makes the gap visible rather than mysterious.

**Alternatives considered:** (a) skip to the next rule (rejected above); (b) serve the 3xx target's content in place as if it were a 200 rewrite (rejected: it changes the meaning of the site's rule — a redirect is supposed to change the URL, and pretending otherwise would leave the bar showing a URL whose content is not what the site serves there).

**Touches:** a follow-on "follow `_redirects` 3xx as a real navigation" task, which needs a navigation path out of the scheme-resolution edge (the same plumbing an `ipfs://` -> `ipfs://` in-page redirect would need). The refusal message is the discoverable pointer.

**SUPERSEDED (2026-07-27)** by `ipfs-redirects-3xx-navigation-support`: that navigation path now exists (a shared `RedirectSink` the scheme handler pushes into and `BrowserShell::pump` drains), so a matching 3xx NAVIGATES and `RedirectsError::RedirectNotSupported` is gone. The rationale for the mechanism, the chain bound, and what is still deliberately not differentiated live in `docs/spikes/ipfs-redirects-3xx-navigation-support/DECISIONS.md`. Everything else in this document still stands.

## Decision 4 — a broken `_redirects` FAILS THE LOAD (the werust analogue of §3.4's "500")

**Chosen:** an unparseable/oversized/non-UTF-8 `_redirects`, an off-root target, or a matched-but-unsupported rule all fail the load with a legible `ipfs:// _redirects fallback failed: …` reason. A `_redirects` that fails to VERIFY (tamper/incomplete/budget) fails on that real reason.

**Why:** IPIP-0002 §3.4 requires an error rather than silently ignoring the file, for the same reason as Decision 3: ignoring it serves a different page than the author wrote. werust has no HTTP status to return here in the general case, so "fail the load with the honest reason" is the closest and most trust-coherent analogue. Note this only ever affects a path that was ALREADY not-found: a site with a broken `_redirects` still serves every real page normally.

**Touches:** a site author debugging their own `_redirects` sees the line number and reason.

## Decision 5 — the unique-origin rule: a `to` may never leave the root CID (the security rationale)

**Chosen:** an expanded target is rejected as `RedirectsError::OffRootTarget` if it carries a scheme (`https://…`, `ipfs://…`), a protocol-relative authority (`//host/…`), is not root-relative, or uses `..` to climb above the root. The check runs AFTER placeholder/`:splat` injection, so an escape smuggled in through a captured segment is caught too, and it runs BEFORE any retrieval, so an off-root target is never even fetched.

**Why (the recorded security rationale):** IPIP-0002 §3.1/§4 only permit `_redirects` evaluation where Same-Origin isolation per root CID holds (a subdomain/DNSLink gateway, or a browser with a native `ipfs://` handler — werust), precisely because a rewrite/redirect is a PER-SITE capability that must not be able to speak for another content root. werust serves `ipfs://<cid>` as its own content root, so the `_redirects` of `<rootcid>` governs ONLY paths under `<rootcid>`: every target is resolved as a path under the SAME root CID through the SAME verifying retrieval. That is what makes the feature incapable of letting one site impersonate another, and it is also why the feature adds NO verification bypass — there is no code path here that returns bytes which were not hash-verified under the requested site's own root CID.

**Refused rather than skipped**, for the Decision-3 reason: falling through to a later rule would serve a different page than the author named.

## Decision 6 — no cost, and no behaviour change, for a site that does not opt in

**Chosen:** the rules are consulted ONLY on a `PathNotFound` (IPIP-0002 §3.3). A found resource is returned exactly as before, with zero extra retrievals (asserted by `a_resolvable_path_never_reads_the_redirects_file_at_all`). A site with neither `_redirects` nor a root `404.html` keeps werust's original fail-closed not-found, with the ORIGINAL reason preserved verbatim.

**Why:** the feature must be opt-in per site (the task's explicit criterion) and must not slow down or change the normal path. Keeping the original `PathNotFound` error (rather than a new "no fallback found" one) means the honest not-found message users and tests already rely on is untouched.

**Cost when a site DOES opt in:** at most two extra entity-scoped retrievals on a not-found path (`/_redirects`, then the target), each bounded by the existing `RetrievalBudget`. The rules are not re-evaluated for the target (no fallback loops possible).

## Decision 7 — an OPTIONAL probe treats a transport failure as "absent", verification failures never

**Chosen:** looking for `_redirects` / `404.html` goes through one `probe_optional` helper. It counts BOTH a local `PathNotFound` AND a `Source` (transport) failure as ABSENT (fall through to the next convention, ultimately to the original honest not-found). Every verification-class failure (tamper, incomplete DAG, budget, malformed, unsupported codec/hash, invalid CID) still fails the load on its real reason.

**Why:** gateways differ on how they answer a `dag-scope=entity` request for a path that is not in the DAG, so absence can arrive in two shapes. MEASURED on the live network (2026-07-26, against the jolly-roger root `bafybeihasag6ramvwiiox2p7pgtx37wevyd5xqnhfoeu4w7b7ndfgozjyu`): `dweb.link`, `trustless-gateway.link` and `ipfs.io` all answer `/ipfs/<cid>/does-not-exist?format=car&dag-scope=entity` with **HTTP 200 and the traversal blocks it managed to walk**, so the not-found is decided LOCALLY by werust's own verified walk (`PathNotFound`) and never taken on the gateway's word — the good case. But a gateway is free to answer with an HTTP error instead, which would surface as `RetrieveError::Source(Transport(...))`; if only `PathNotFound` counted as absence, such a gateway would make every site WITHOUT a `_redirects` fail its not-found path with a confusing gateway-transport message instead of the honest "path not found" it has always given — breaking the opt-in promise on the live path while offline fixtures stayed green. Both shapes are pinned by tests (`a_scoped_gateway_site_*` for the measured behaviour, `a_gateway_that_http_404s_the_optional_probes_is_tolerated_as_absence` for the other).

**Safety:** this is a strictly *narrowing* interpretation on an OPTIONAL lookup. It can never produce content (the branch returns "absent") and never suppresses a verification signal (those variants are untouched); the worst case is the pre-existing not-found. A transport failure on a target that a rule actually NAMED is *not* affected: that path goes through `serve_fallback_target`, which fails the load.

**Touches:** any future optional-file convention under the root CID should reuse `probe_optional` rather than re-deriving which errors mean "absent".

## Naming / coherence check (per the coherence rule)

- `redirects` (module), `_redirects` (the file), `FallbackAction`, `RedirectsError` — all take their names from the IPIP-0002 spec being implemented, so no existing werust term is re-meaned. Checked against `CONTEXT.md`'s glossary: no existing concept named "redirect", "fallback", or "web-pathing" existed.
- `SchemeResponse.status` is an HTTP-equivalent status, the same meaning `fetcher::Response.status` already carries in this codebase (no second meaning).
- The new capability row is named `ipfs-web-pathing-fallback` rather than "ipfs-redirects", because what landed is the not-found *pathing* fallback and NOT redirect-following (Decision 3); naming it after redirects would have over-claimed.

## Verification / fail-closed unchanged (checked)

- Both the `_redirects` file and every fallback target are fetched through the SAME `ContentRetriever::retrieve(cid, path)` as any other resource — every block hash-verified by the same per-block CAR check, under the same `RetrievalBudget` (`max_bytes`/`max_blocks` unchanged, and the extra fetches are ordinary bounded retrievals).
- A tampered fallback page fails the load as `BlockHashMismatch` and never renders (proven end to end in `tests/ipfs_redirects_fixture.rs::the_fallback_content_is_hash_verified_through_the_same_retrieval`).
- A missing target is a fail-closed not-found naming the target; an off-root target is refused before it is fetched.
- Every pre-existing `fetcher`/`ipfs` test (tamper, missing-block, truncated-CAR, budget, path-not-found, HAMT, chunked file, SvelteKit fixture) stays green.

## Tests (all network-isolated)

- `crates/werust-core/src/redirects.rs` (unit): the IPIP-0002 grammar against the SPEC's own example file (§5.1), size cap, malformed-line refusals, duplicate-placeholder refusal, first-match-wins, placeholders + `:splat`, off-root refusals (incl. an escape hidden in a capture), query-drop, the 3xx refusal, 410/451.
- `crates/werust-core/src/ipfs.rs` (unit, at the seam): the jolly-roger 404 case, the 200 rewrite, "a found path never reads `_redirects`", the opt-out site keeping its honest not-found, the default `404.html`, fall-through when no rule matches, a missing target, an off-root target never being fetched, the 3xx refusal, and a tampered `_redirects`.
- `crates/werust-core/tests/ipfs_redirects_fixture.rs` (integration, over a REAL synthesized content-addressed site + the PRODUCTION `TrustlessGatewayCarRetriever`): the custom-404 fallback with its 404 status, an existing path never intercepted by the catch-all, the 200 rewrite, first-match-wins, the no-opt-in site, the default `404.html`, a missing target failing closed, off-root targets rejected, a TAMPERED fallback page failing closed, and a broken `_redirects` failing the load. Plus the PER-RESOURCE-SCOPED (`dag-scope=entity`) gateway shape the production backend really talks to — each fallback file its own scoped fetch — in both measured gateway behaviours for an absent path (traversal-blocks-with-200, and HTTP-error).

All fixtures are synthesized offline (dag-pb/UnixFS blocks framed into a CARv1 stream by the same vetted crates the production path decodes with); the only network use in this task was the one-off re-confirmation of the live jolly-roger + gateway behaviour recorded above, which no test depends on.
