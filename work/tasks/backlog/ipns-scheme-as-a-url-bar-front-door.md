---
title: "`ipns://<name>` in the URL bar: the entry door ADR-0007 named and deferred (IPNS resolution already exists, it just has no front door but ENS)"
slug: ipns-scheme-as-a-url-bar-front-door
blockedBy: []
covers: []
---

## What to build

Field-noticed 2026-08-01: `ipns://` does nothing. That is not a bug, it is a **recorded Phase-1 boundary** in `docs/adr/0007-ipns-name-resolution-via-a-client-verified-record.md` decision 4:

> **Entry surface = `ipns-ns` via ENS only (Phase 1).** [...] A bare `ipns://` in the URL bar, and DNSLink (a `_dnslink` DNS-TXT lookup, a DIFFERENT trust story from a signed libp2p-key record), are named follow-ons, NOT built here.

This task builds the first of those two follow-ons. **All of the hard work already exists**: `resolve_ipns_name` verifies a record against its libp2p key, the `IpnsRecordSource` seam fetches candidate record bytes over the user's chosen retrieval backend, the fail-closed `IpnsError` taxonomy is complete, and the resolved CID renders through the verified `ipfs://` path. The only missing piece is the DOOR: today an `ipns://…` entry is classified as an explicit scheme and taken literally, so it is handed to a backend that has no `ipns` handler registered, and the load simply fails.

**Build it as a name-resolution front door, not as a new scheme handler.** The ENS front door already does exactly this shape: recognise the name at the entry door, resolve it to a CID, then load the resolved `ipfs://<cid>[/path]` through the existing verified path. An `ipns://<name>[/path]` entry should travel the same route, which means **no new scheme registration on any of the four edges** (and so no repeat of the per-edge registration work `ipfs://` needed). Reuse, do not parallel-build: a second resolution path for the same protocol is what this repo keeps removing.

Carry over the precedents rather than re-deciding them:

- **The bar keeps the name.** The ENS front door pins the `.eth` name in the URL bar rather than exposing the resolved `ipfs://<cid>`; an `ipns://` load should likewise keep showing `ipns://<name>`, for the same reason (the user navigated to a name, and the CID is an implementation detail of this visit).
- **The posture is `MutableName`, never `ContentVerified`.** ADR-0007 decision 5: a signature-verified record plus hash-verified bytes is STILL a mutable name, because the key holder can publish a new record. Reached directly (not via ENS) there is no trusted-RPC step, so `MutableName` is the posture rather than the louder `NameViaTrustedRpc`.
- **It is blessable, by the mutability axis.** Per `CONTEXT.md`, blessability follows mutability, not the displayed posture, so a direct `ipns://` load takes a trusted name pin and the TOFU change-warning exactly as an ENS-reached IPNS name does. That is inherited behaviour, not new work, but it must be verified rather than assumed.
- **The sub-path threads through.** `ipns://<name>/blog/` must resolve the NAME and thread `/blog/` into the resolved `ipfs://<cid>/blog/`, the way the `.eth`-with-path entry already does.

## Out of scope, deliberately

**DNSLink** (`_dnslink` TXT lookup). ADR-0007 names it in the same breath but flags it as a different trust story: a DNS answer is not a signed libp2p-key record, so it needs its own posture reasoning and its own ADR. Building both here would smuggle an unreviewed trust decision in behind a URL-bar feature.

## Acceptance criteria

- [ ] `ipns://<name>` typed in the URL bar resolves through the EXISTING `resolve_ipns_name` path and renders the CID it currently points at, on desktop and on both mobile edges.
- [ ] `ipns://<name>/<path>` threads the sub-path into the resolved `ipfs://<cid>/<path>` load.
- [ ] No new custom scheme is registered on any edge (the entry door resolves the name; the load is the existing verified `ipfs://` path).
- [ ] The settled trust posture for a direct `ipns://` load is `MutableName`, and the trust surface explains it in the shared derivation's words (no new per-edge string).
- [ ] The load is blessable, and a later resolution to a DIFFERENT CID raises the existing TOFU change warning.
- [ ] Every `IpnsError` variant surfaces werust's own fail-closed reason and renders NOTHING unverified (the ADR-0007 taxonomy is preserved end to end, not collapsed into one generic failure).
- [ ] A malformed `ipns://` entry (not a usable libp2p-key name) fails legibly rather than reaching a platform loader with an unloadable scheme (see `malformed-ipfs-cid-must-fail-legibly-on-every-edge-not-as-a-platform-error-page` for the class).
- [ ] DNSLink is NOT implemented, and its absence is recorded rather than silent.
- [ ] Tests cover the new entry-door recognition at the seam boundary, network-isolated, mirroring the repo's style.

## Blocked by

- None. `docs/adr/0007` is landed and IPNS resolution + the verified `ipfs://` render path are in `work/tasks/done/`.

## Prompt

> Goal: make `ipns://<name>[/path]` work as a URL-bar front door. This is the follow-on `docs/adr/0007-ipns-name-resolution-via-a-client-verified-record.md` decision 4 explicitly named and deferred ("a bare `ipns://` in the URL bar [...] a named follow-on, NOT built here"). Read that ADR first, including its fail-closed taxonomy and its decision 5 on the honest mutable-name posture.
>
> The resolution machinery is DONE: `werust_core::ipns::resolve_ipns_name`, the `IpnsRecordSource` seam and its gateway backend, and the verified `ipfs://` render path. What is missing is only the entry-door recognition. Model it on the ENS front door, which recognises a name, resolves it, and loads the resulting `ipfs://<cid>[/path]`: that gives you every edge for free and needs NO new scheme registration (contrast `ipfs://`, which needed per-edge `register_uri_scheme` / `WKURLSchemeHandler` / WebView2 registration work). Look at how the `.eth` name-and-path recogniser splits an entry and how the front door pins the name in the URL bar; mirror both.
>
> Respect the repo's ONE-derivation rule for user-facing strings (`CONTEXT.md`) and the trust vocabulary in `docs/adr/0006`: the posture for a direct `ipns://` load is `MutableName`, and its explanation is a core derivation, not a per-edge literal. Blessability follows the MUTABILITY axis, so verify (do not assume) that the trusted-name-pin and its change warning apply to this door.
>
> DNSLink is OUT OF SCOPE and must not be built here: ADR-0007 flags it as a different trust story needing its own reasoning.
>
> FIRST, check this task against current reality (it is a launch snapshot and may have DRIFTED): confirm the IPNS resolution entry points, the ENS front door's name+path split, and the pin/bless mechanism are still shaped as described. If the entry-door classification has changed, route to needs-attention rather than building on this description.
>
> RECORD non-obvious in-scope decisions durably and link them from the done record. Candidates you will likely hit: what the URL bar shows after resolution (the precedent says keep the name), and whether an `ipns://` name that resolves to a `/ipns/` chain is refused (ADR-0007's `UnsupportedTarget` says refuse, do not blindly follow) at the door or deeper in.

---

### Claiming this task

```sh
dorfl claim ipns-scheme-as-a-url-bar-front-door --arbiter origin
git fetch origin && git switch -c work/ipns-scheme-as-a-url-bar-front-door origin/main
git mv work/tasks/ready/ipns-scheme-as-a-url-bar-front-door.md work/tasks/done/ipns-scheme-as-a-url-bar-front-door.md
```
