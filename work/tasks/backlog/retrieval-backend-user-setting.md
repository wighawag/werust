---
title: "User-choosable IPFS retrieval backend: a setting to pick gateway (default) / delegated-routing / embedded-p2p / custom gateway-or-node URL"
slug: retrieval-backend-user-setting
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
needsAnswers: true
blockedBy: [verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend]
covers: [1]
---

<!-- open-questions -->

## Open questions

1. **Where does the settings surface live?** werust has NO settings/preferences UI yet on any platform. DECIDE the minimum viable surface for THIS setting: a small settings panel/page in the desktop chrome (and the mobile shells), or an internal `werust://settings`-style page, or (interim) a config value with a sensible default that a later general settings task exposes in the UI. Whatever it is, it must land on desktop + iOS + Android per the parity guard, or be explicitly stubbed-with-linked-task there.
2. **Persistence.** How is the chosen backend persisted across launches (a small settings file — where, and isolated in tests per the shared-write rule)? There is no config subsystem today; decide whether this task introduces a minimal one or stays in-memory + a single override until a broader settings task lands.
3. **Which backends are offered at ship time.** The seam supports gateway (default), delegated-routing, embedded-p2p (Phase-2 async), and a custom user URL. Only the backends that actually EXIST when this ships can be offered; the others are greyed-out/"coming soon" or omitted. Confirm the initial option set (at minimum: default trustless gateway + a custom gateway/node URL).

<!-- /open-questions -->

## What to build

Let the USER choose how werust retrieves IPFS content, by selecting among the retrieval backends behind the `ContentRetriever` seam (from `verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend`): the default trustless-gateway CAR backend, delegated-routing, an embedded p2p client (once it exists, Phase-2), or a custom gateway / local-node URL the user supplies. This is the user-facing selector the seam was built for — the seam makes the choice a swap; this task exposes and persists it.

Privacy + trust framing (surface it, do not hide it): the retrieval backend is an egress + trust choice (a default public gateway sees every site the user visits; a custom/local node keeps it private). The setting should make that legible, mirroring how the trust indicator and the RPC-endpoint choice are honest product surfaces (`docs/adr/0001`). This is the natural first real customer of the platform-capability parity guard (`platform-capability-parity-guard`): it must ship on all contexts or be explicitly tracked per platform.

## Acceptance criteria

- [ ] A user-facing setting selects the active IPFS retrieval backend from the options available at ship time (at minimum: default trustless gateway + a custom gateway/local-node URL); unavailable backends are clearly not-yet-available, not silently broken.
- [ ] Selecting a backend actually switches the `ContentRetriever` the load path uses (proven by a load going through the chosen backend), and a custom URL is validated + used as the gateway/node endpoint.
- [ ] The choice persists across launches (or, if deferred, an in-memory + single-override interim with the persistence follow-on named).
- [ ] The setting is legible about the privacy/trust trade-off (a default public gateway sees browsing; a custom/local endpoint is the private choice).
- [ ] The setting is present on desktop + iOS + Android, or explicitly stubbed-with-linked-task on any platform where it is deferred (honouring the parity guard).
- [ ] Tests cover backend selection + custom-URL use + persistence, network-isolated; if persistence writes to a shared/global location, tests isolate it and assert the real one is untouched (shared-write rule).

## Blocked by

- Blocked by `verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend` (the `ContentRetriever` seam + default backend this setting switches between). Also `needsAnswers: true` until the settings surface, persistence, and the initial option set are settled. Relates to `platform-capability-parity-guard` (this setting is a per-platform capability the guard should track).

## Prompt

> Goal: let the user CHOOSE the IPFS retrieval backend (default trustless gateway / delegated-routing / embedded-p2p when it exists / a custom gateway-or-node URL), by exposing and persisting a selection over the `ContentRetriever` seam. The seam already makes the backend a swap; this task is the user-facing selector + persistence + the honest privacy/trust framing.
>
> Domain vocabulary: retrieval is a seam with swappable backends (built in `verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend`, modelled like `EthereumProvider`/`Fetcher`/`Renderer`). A trustless gateway needs no node but is an egress a third party sees; a custom/local node is the private, self-trusted choice. The RPC-endpoint default and the trust indicator are the precedents for treating such a choice as an honest product surface (`docs/adr/0001`).
>
> Where to look: the `ContentRetriever` seam + its `DEFAULT_*`/`with_*()` override pattern (no config crate today) from the blocking task; the desktop chrome (`crates/werust`) and the mobile shells (`crates/werust-android`, `crates/werust-ios`) for where a setting would surface — there is NO settings UI yet, so decide the minimal surface (open questions). The parity guard (`platform-capability-parity-guard`) is the mechanism that should track this setting per platform.
>
> Done = the user can pick a retrieval backend (incl. a custom URL), the choice switches the actual load path and persists, the privacy/trust trade-off is legible, it is present-or-tracked on all three platforms, and it is proven with network-isolated tests (with shared-write isolation if it persists to disk). FIRST re-check current reality (the seam's landed shape, whether any settings surface now exists) and route to needs-attention on drift. RECORD the settings-surface + persistence decisions durably per the task template.
