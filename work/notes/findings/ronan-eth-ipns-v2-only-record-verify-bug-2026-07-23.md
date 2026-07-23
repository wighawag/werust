---
title: "ronan.eth IPNS load failed because rust-ipns 0.9.0 rejects V2-only records (DataMismatch); the fix is a wire-normalization shim, all crypto still in the crate"
date: 2026-07-23
status: verified
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
task: prominent-load-failure-and-ipns-resolution-diagnosis
kind: finding
source:
  - live: GET https://dweb.link/ipns/k51qzi5uqu5dlu1ien9spji7pu49mfw97mn0qv4azugqcvenj0dvzq9bgwp1zc?format=ipns-record (301 -> trustless-gateway.link, 200, application/vnd.ipfs.ipns-record)
  - https://specs.ipfs.tech/ipns/ipns-record/ (IPIP-428: V2-only records omit the deprecated protobuf fields)
  - rust-ipns 0.9.0 src/lib.rs (Record::data / verify_signature)
---

## The actual ronan.eth failure (reproduced against the live network)

`ronan.eth`'s `contenthash(node)` is `0xe5010172002408011220e2bf9e13d80b02d817938692adf17e8cae6f6b2d46b39eb467c0b3a52cac2648` (captured via `cast call`). The `0xe5` protoCode is `ipns-ns`; werust's decoder canonicalises the libp2p-key CID to the Base36 name `k51qzi5uqu5dlu1ien9spji7pu49mfw97mn0qv4azugqcvenj0dvzq9bgwp1zc`.

Resolving that name through werust's real path (`GatewayIpnsRecordSource` over `DEFAULT_IPNS_GATEWAY = https://dweb.link`, then `resolve_ipns_name`) produces:

```
IpnsError::Unverifiable { detail: "dag-cbor data does not match the protobuf fields" }
Display: IPNS record did not verify: dag-cbor data did not match the protobuf fields
```

The record IS fetched and decoded fine (221 bytes, HTTP 200 after dweb.link 301-redirects to trustless-gateway.link). The failure is in `Record::verify`, NOT the fetch. So werust surfaced this as a scary "did not verify" trust failure when in fact the record is perfectly valid and the target is `/ipfs/bafybeiepw4aijr4dtlhth2xkzskxaxcjvtk6neqsd6zua7rfv6m5nbkesu` (confirmed identical to what `ronan.eth.limo` serves).

## Root cause: rust-ipns 0.9.0 rejects modern V2-only records

The record on the wire has ONLY protobuf field 8 (`signatureV2`) and field 9 (`data`, the dag-cbor map). It has NO field 1 (`value`), 2 (`signatureV1`), 3 (`validityType`), 4 (`validity`), 5 (`sequence`), 6 (`ttl`). This is a **V2-only IPNS record** (IPIP-428): the deprecated top-level protobuf fields that duplicate the CBOR `Data` are OMITTED. Modern gateways (dweb.link / trustless-gateway.link, and go-ipfs/boxo) serve records this way.

`rust-ipns 0.9.0`'s `Record::verify` -> `verify_signature` calls `self.data()`, which decodes the CBOR `Data` and then **requires the CBOR fields to equal the top-level protobuf fields**:

```rust
if data.value != self.value || data.validity != self.validity
   || data.validity_type != self.validity_type
   || data.sequence != self.sequence || data.ttl != self.ttl {
    return Err(Error::DataMismatch);
}
```

For a V2-only record `self.value` / `self.validity` / `self.sequence` / `self.ttl` are empty/zero (the protobuf fields are absent), while the CBOR `data.value` is `/ipfs/...`, so the equality fails -> `DataMismatch`. The V2 SIGNATURE itself (over `ipns-signature:` ++ the CBOR data) is valid; only this redundant cross-check breaks. 0.9.0 is the latest published rust-ipns, so there is no upstream fix to bump to.

Per the IPNS spec the CBOR `data` is the AUTHORITATIVE source of truth that the V2 signature covers; the protobuf fields are a deprecated backwards-compat mirror. A verifier that REQUIRES them is wrong for V2-only records.

## The fix (implemented): a wire-normalization shim, crypto untouched

Before `Record::decode`, werust normalizes the raw record bytes: if the protobuf `value` field is empty but a `data` (CBOR) field is present (the V2-only shape), it decodes the CBOR `Data` (via `serde_ipld_dagcbor` -> `rust_ipns::Data`, the SAME crate) and re-encodes the `IpnsEntry` protobuf with fields 1/3/4/5/6 populated FROM the CBOR data (which the V2 signature already covers), preserving fields 7/8/9 verbatim. The normalized bytes decode into a "V1+V2-shaped" record whose `data()` cross-check now passes, and `Record::verify` does the real V2 signature + name-binding + validity check unchanged.

All cryptography stays in `rust-ipns` / `libp2p-identity` (ADR 0001: bind vetted crypto, never hand-roll). The shim only reshapes an equivalent wire form; it copies the CBOR-signed values into their protobuf mirror, so it cannot smuggle in an unsigned value (the crate still verifies the signature over the CBOR data, and the CBOR data is unchanged). A record that already carries the protobuf fields is passed through untouched.

Proven end to end against the LIVE ronan.eth record: after normalization `Record::verify` returns Ok and `resolve_ipns_name` yields `ipfs://bafybeiepw4aijr4dtlhth2xkzskxaxcjvtk6neqsd6zua7rfv6m5nbkesu`.

## Reproduction / measurement artifact

`docs/spikes/prominent-load-failure-and-ipns-resolution-diagnosis/` holds the captured raw record bytes + a README with the exact reproduction (cast contenthash call, the live GET, the byte-level protobuf dump, the before/after verify). The offline unit tests in `crates/werust-core/src/ipns.rs` pin the V2-only case with a signed fixture so this never regresses without a live network.

## Decisions recorded here (so a reviewer/human can ratify or reverse)

- **The V2-only fix is a wire-normalization SHIM, not a re-implemented verifier.** Alternatives considered: (a) hand-verify the V2 signature in werust with `libp2p-identity` over the CBOR data (rejected: duplicates/re-means the crate's job and re-introduces crypto handling werust deliberately delegates, against ADR 0001); (b) fork/patch `rust-ipns` (rejected: heavier, and 0.9.0 is already the latest published). The shim reshapes an equivalent wire form and lets the crate do ALL crypto. It touches only `crates/werust-core/src/ipns.rs` (the `resolve_ipns_name` step-3 path); no other command/flag. The upstream follow-on below is the real long-term home.
- **Prominent failure surfacing is ADDITIVE (a banner ON TOP OF the existing footer status), and a new cross-platform parity CAPABILITY (`prominent-load-failure`).** The footer `status_line` / `statusLine()` stays (it is the compact one-liner); the banner is the unmissable in-view surface a failed load raises. It reuses the SAME `last_error` / chrome-JSON `error` fact (no new state, no re-meaning of `last_error` or the trust posture — the banner is orthogonal to the trust indicator, which is about a SUCCESSFUL load's posture). It is registered in `docs/platform-capability-matrix.toml` as its own capability so it cannot silently ship on one platform (the same guard that caught the mobile `ipfs://` gap). Reviewer nod wanted on: the banner wording ("⚠ This page failed to load: <reason>") and that a new parity capability row is the right layer for this (vs folding it into `trust-indicator`, which it deliberately does NOT, since a failure is not a trust posture).

## Follow-on (out of this task's scope, noted so the signal is captured)

Upstream `rust-ipns` should relax `Record::data()` to treat the CBOR `data` as authoritative for V2 records (or skip the cross-check when the protobuf fields are absent). Worth a PR / issue upstream so werust can later drop the shim. Not done here.
