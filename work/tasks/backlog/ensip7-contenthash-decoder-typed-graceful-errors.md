---
title: ENSIP-7 contenthash decoder with typed, protocol-named graceful errors
slug: ensip7-contenthash-decoder-typed-graceful-errors
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [4]
---

## What to build

A pure decoder that turns an ENSIP-7 / EIP-1577 `contenthash` byte string into a small typed enum, dispatching by the contenthash's OWN multicodec protoCode (never defaulting to `ipfs://` for other types). The `ipfs-ns` case decodes to an `ipfs://<cid>` reference (the CID rendered as the canonical string the existing verified `ipfs://` path already consumes); every other case is a DISTINCT, protocol-named typed variant that maps to a legible user-facing load failure — never a crash, a mis-dispatch, or a blank fail.

The variant set the decoder must distinguish (each its own message):
- `0xe3` **ipfs-ns** (immutable CID) → SUPPORTED: an `ipfs://<cid>` reference.
- `0xe5` **ipns-ns** (mutable IPNS) → "this name uses a mutable IPNS pointer, not yet supported" (deferred to Phase 2/3).
- `0xe4` **swarm-ns** → "points to Swarm, not supported".
- **Arweave** / `onion` / `onion3` / `skynet` / `zeronet` / DNSLink / any unknown protoCode → name it if the protoCode is known ("points to Arweave, not supported"), else "unsupported/unknown contenthash protocol (0x..)".
- **NoContenthash** (empty/absent) and **Malformed** (undecodable bytes) → their own distinct messages.

This is decode/dispatch logic only — RESOLVING the non-IPFS protocols is explicitly out of scope; DETECTING them and erroring clearly is the whole point of the task.

## Acceptance criteria

- [ ] A pure function/type decodes ENSIP-7 contenthash bytes into a typed enum keyed by protoCode.
- [ ] `ipfs-ns` (`0xe3`) decodes to an `ipfs://<cid>` reference whose CID string is exactly what the existing verified `ipfs://` path consumes.
- [ ] `ipns-ns` (`0xe5`), `swarm-ns` (`0xe4`), Arweave, and an unknown protoCode each produce their OWN distinct, protocol-named failure variant/message.
- [ ] `NoContenthash` (empty) and `Malformed` (undecodable) are distinct variants with distinct messages.
- [ ] The decoder NEVER defaults an unrecognised protoCode to `ipfs://` and never panics on malformed input.
- [ ] A fixture test PER protoCode (ipfs-ns success; ipns-ns / swarm-ns / arweave / unknown each producing their distinct error; plus no-contenthash and malformed), all offline.
- [ ] Tests cover the new behaviour (mirror the repo's existing test style).

## Blocked by

- None — can start immediately.

## Prompt

> Goal: build the ENSIP-7 (EIP-1577) `contenthash` decoder — a pure, offline byte→typed-enum decoder that dispatches by the contenthash's own multicodec protoCode and produces graceful, protocol-NAMED errors for everything werust does not yet support. This is a hard requirement of the spec: an unsupported name must fail with a clear "points to <protocol>, not supported" message, never a crash, a mis-dispatch, or a blank failure.
>
> Domain vocabulary: an ENSIP-7 contenthash is a multicodec-prefixed byte string. The leading varint is the protoCode: `0xe3` = `ipfs-ns` (an IPFS CID follows), `0xe5` = `ipns-ns` (mutable IPNS), `0xe4` = `swarm-ns`, plus Arweave / `onion` / `onion3` / `skynet` / `zeronet` / DNSLink and others. Only `ipfs-ns` is supported in Phase 1; it yields an `ipfs://<cid>` reference. The CID it carries must come out as the same canonical CID string the existing `ipfs://` verified path already accepts (see `werust-core`'s ipfs module — `parse_ipfs_uri` consumes `ipfs://<cid>[/path]`, and the `fetcher` crate parses/verifies CIDs via the vetted `cid`/`multihash` crates; reuse those crates for CID parsing rather than hand-rolling byte layout).
>
> Where to look: the `fetcher` crate already depends on the `cid` / `multihash` crates and re-exports `Cid` — lean on that for the `ipfs-ns` CID decode so there is no version skew. The graceful-error stance mirrors the existing verified path's "reject-when-unsure, name the reason" discipline (`docs/adr/0001`). Keep this task PURE (no network, no seam) — it is byte-in, typed-enum-out, so the ENS resolution task can consume it and the URL-bar task can turn each variant into a legible chrome failure.
>
> Done = every protoCode in the spec's table has a fixture test asserting its distinct decoded variant/message, with `ipfs-ns` producing a usable `ipfs://<cid>` and everything else a named refusal. FIRST re-check against current reality (the CID crate surface / the `ipfs://` parser shape may have moved) per WORK-CONTRACT.md "Drift is a needs-attention signal". RECORD any non-obvious in-scope decision (e.g. how you canonicalise the `ipfs-ns` CID, exactly which protoCodes you name vs bucket as "unknown") durably per the task template.
