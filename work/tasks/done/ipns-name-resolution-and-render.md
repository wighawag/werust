---
title: "IPNS name resolution and render — resolve ipns-ns (mutable) pointers to their current CID via verifiable IPNS records, then render through the verified ipfs:// path"
slug: ipns-name-resolution-and-render
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: [verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend]
covers: [1]
---

## Settled decisions (from the design discussion — DECIDED, build to them)

1. **Record retrieval + verification:** resolve IPNS via `GET /ipns/{key}?format=ipns-record` (`application/vnd.ipfs.ipns-record`, multicodec `0x0300`) from a trustless/delegated endpoint, then verify the record's signature + validity window CLIENT-SIDE against the IPNS key — same untrusted-source-plus-client-verify discipline as the CAR blocks. Bind a vetted IPNS-record crate for decode + signature verification (`docs/adr/0001`); do not hand-roll signature crypto.
2. **DNSLink is OUT of scope here** (a named follow-on): it is a `_dnslink` DNS TXT lookup, a different trust story from a signed libp2p-key record. This task does libp2p-key IPNS names only.
3. **Entry surface = ipns-ns via ENS only (Phase 1):** flip the ENSIP-7 decoder's `ipns-ns` (`0xe5`) refusal into a route into IPNS resolution, dispatched by the ENS front door. A bare `ipns://` in the URL bar is a follow-on, not this task.
4. **Trust posture = a NEW distinct `MutableName` warning (the two-axis model).** werust's trust indicator has TWO orthogonal axes and shows the MOST IMPORTANT applicable warning (see `work/notes/observations/trust-posture-two-axes-model-2026-07-22.md`):
   - Axis 1 resolution-trust: how the name->CID was learned (ENS Phase 1 = `NameViaTrustedRpc`; IPNS = a client-verified signed record, no RPC warning).
   - Axis 2 mutability: can the controller repoint the name? IPNS (key holder) and ENS (owner `setContenthash`) are BOTH mutable; only a direct `ipfs://<cid>` is immutable. We cannot cheaply detect a locked/immutable ENS name yet, so mutable is the honest default.
   - Add a NEW `TrustPosture::MutableName` ("content-verified, mutable name"): bytes hash-verified but the name is controller-repointable. Distinct from `ContentVerified` (honestly weaker) and from `UnverifiedOrigin` (the bytes DID verify). Thread it like the existing postures (renderer -> webview -> core `ChromeState` -> desktop chrome), with a legible warning label, NEVER "verified".
   - Display rule: show the loudest applicable warning. An IPNS load shows `MutableName`. This task ALSO reframes ENS: an ENS ipfs-ns load is mutable too, but its LOUDER warning is `NameViaTrustedRpc` (a misdirecting RPC is worse than an owner repointing), so ENS Phase-1 keeps showing `NameViaTrustedRpc` — `MutableName` becomes what ENS shows once Phase-2 clears the RPC-trust warning. Keep the display precedence explicit so ENS falls back to `MutableName` with no rule change later.
5. **TOFU (pin-and-warn-on-change) is a follow-on** (`ipns-tofu-pin-and-warn-on-change`): record a mutable name's blessed CID, warn on a later change. Needs a pin store + popup; not this task.

## What to build

Make an IPNS-pointed name actually resolve and render, closing the `ipns-ns` gap the ENSIP-7 decoder currently names as "not yet supported". The path: resolve an IPNS name (a signature-verifiable IPNS record, or a DNSLink) to the CID it currently points at, then feed that CID into the verified `ipfs://` render path (the same path the DAG/CAR retrieval task makes work for real multi-block sites). Every failure (an unresolvable name, an invalid/expired/unsigned record, a record pointing at an unsupported contenthash) fails closed with a distinct, legible reason.

This builds ON the verifiable-retrieval task (`verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend`): it reuses that task's verified `ipfs://<cid>` render (so an IPNS name resolving to a real directory site renders like any other), and it reuses the SAME untrusted-source-plus-client-verify discipline for the IPNS RECORD (fetch the record over HTTP from a trustless/delegated endpoint, verify its signature + validity client-side against the IPNS key). It also upgrades the ENSIP-7 decoder + ENS front door: an `ipns-ns` (`0xe5`) contenthash stops being a named refusal and instead routes into IPNS resolution.

## Acceptance criteria

- [ ] A libp2p-key IPNS name resolves to its current CID via a verifiable IPNS record (record fetched from an untrusted endpoint, signature + validity verified client-side against the key), NO IPFS node required.
- [ ] The resolved CID renders through the verified `ipfs://` path (reusing the DAG/CAR retrieval task's output), so an IPNS name pointing at a real directory site renders end to end.
- [ ] An `ipns-ns` (`0xe5`) ENS contenthash is no longer a hard "not supported" refusal: it routes into IPNS resolution (the ENSIP-7 decoder + front door are updated), while every OTHER unsupported protoCode stays a named refusal.
- [ ] A resolved IPNS page shows the NEW `TrustPosture::MutableName` warning ("content-verified, mutable name"), threaded to a distinct legible indicator; it is NEVER `ContentVerified` and NEVER labelled "verified". ENS ipfs-ns loads keep showing `NameViaTrustedRpc` (the louder warning) per the display-precedence rule, and the precedence is explicit so ENS falls back to `MutableName` once Phase-2 clears the RPC warning.
- [ ] Fail-closed on every failure: unresolvable name, invalid/expired/unsigned record, a record pointing at an unsupported/unverifiable contenthash \u2014 each a distinct legible reason, nothing guessed or unverified rendered.
- [ ] DNSLink is explicitly deferred (a named follow-on), not built here.
- [ ] Tests are network-isolated (pinned IPNS record + contenthash + content fixtures, loopback, no live network) and mirror the repo's test style, including the failure paths and the honest-mutability posture.

## Blocked by

- Blocked by `verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend` (this reuses its verified `ipfs://<cid>` render path for the resolved CID, and its untrusted-source-plus-client-verify shape for the IPNS record). The design forks are settled above.

## Prompt

> Goal: resolve and render IPNS (mutable) names \u2014 close the `ipns-ns` gap the ENSIP-7 decoder currently refuses. Resolve an IPNS name to its current CID via a client-VERIFIED IPNS record (no node), then render that CID through the verified `ipfs://` path the trustless-retrieval task builds. Be honest that an IPNS name is MUTABLE (never imply the immutability of an ipfs-ns CID), and fail closed on every bad record / bad target.
>
> Domain vocabulary: IPNS is the mutable-pointer layer of IPFS. A libp2p-key IPNS name is a public-key hash; its current value is a SIGNED IPNS record (multicodec `0x0300`) that maps the name to a `/ipfs/<cid>` (or another `/ipns/`), with a sequence number + validity \u2014 verify the signature against the key and the validity window client-side, treating the fetched record as UNTRUSTED (same discipline as the CAR blocks). A DNSLink name resolves via a `_dnslink` DNS TXT record instead (a DNS lookup, not a signed record). The ENSIP-7 `ipns-ns` protoCode is `0xe5` (the decoder already names it). The Trustless Gateway spec exposes verifiable IPNS records at `GET /ipns/{key}?format=ipns-record`.
>
> Where to look: the ENSIP-7 decoder (`werust-core` contenthash module) currently returns a distinct "mutable IPNS pointer, not yet supported" refusal for `ipns-ns` \u2014 change that to route into IPNS resolution. The ENS front door (`bare-eth-urlbar-front-door-end-to-end` in `tasks/done/`) dispatches by decoded contenthash type; add the ipns-ns branch. The verified `ipfs://<cid>` render + real directory-site rendering come from `verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend` (the blocking task) \u2014 REUSE it for the resolved CID; do not reimplement content retrieval. Bind a vetted IPNS-record crate for decode + signature verification (`docs/adr/0001`); do not hand-roll signature crypto.
>
> Trust honesty (a hard requirement, like the Phase-1 `NameViaTrustedRpc` decision): an IPNS name is mutable, so even a signature-verified record + content-verified bytes must NOT be presented as immutable. Add the `MutableName` posture per the settled two-axis model (`work/notes/observations/trust-posture-two-axes-model-2026-07-22.md`) and record it as an ADR.
>
> Done = a libp2p-key IPNS name resolves via a client-verified record to a CID that renders through the verified path; an ipns-ns ENS contenthash routes here instead of refusing; the mutable-name posture is honest; every failure is distinct and fail-closed; all proven offline. FIRST re-check current reality (the decoder's ipns-ns handling, the front-door dispatch, the retrieval task's landed render API) and route to needs-attention on drift. RECORD the record-verification approach, the mutable-name posture, and the entry surface durably (findings note + ADR for the trust posture).
