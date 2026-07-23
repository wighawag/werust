---
title: "IPNS resolution: a libp2p-key name is a CID whose multihash is the PeerId; its record is a signed 0x0300 with a client-verifiable signature + validity"
date: 2026-07-23
status: verified
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
kind: finding
source:
  - https://specs.ipfs.tech/ipns/ipns-record/
  - https://specs.ipfs.tech/http-gateways/trustless-gateway/
  - https://docs.rs/rust-ipns/latest/rust_ipns/
---

## Ground truth (gathered while building `ipns-name-resolution-and-render`)

- **An IPNS name is a public key, addressed as a CID.** A libp2p-key IPNS name is a CIDv1 with the `libp2p-key` multicodec (code `0x72`), whose multihash IS the key (the `PeerId`). Small keys (ed25519 by default) inline the public key into the multihash via the `identity` hash; larger keys are referenced by a sha2-256 hash and the record must then carry the `pubKey` field. Canonically the name is rendered case-insensitive Base36 (the `k51…` form), which a trustless gateway accepts at `GET /ipns/{name}`. Source: IPNS Record spec + IPFS "address IPFS on the web".
- **The current value is a SIGNED record, verified client-side.** The record (the `IpnsEntry` protobuf; the routing-layer wrapper is multicodec `0x0300`) carries `Value` (a `/ipfs/<cid>` or `/ipns/<name>` path), `Validity` (an EOL RFC3339 timestamp) + `ValidityType`, `Sequence`, `TTL`, a dag-cbor `Data` field, and a V2 signature over `ipns-signature:` ++ the dag-cbor data. A client MUST confirm the record's signature matches the libp2p-key from the requested name, and MUST treat the fetched bytes as untrusted (same discipline as CAR blocks). Source: IPNS Record spec + Trustless Gateway spec (`?format=ipns-record`, `application/vnd.ipfs.ipns-record`).
- **`rust-ipns` verifies all three in one call.** `rust_ipns::Record::decode(bytes)` then `Record::verify(peer_id)` validates the name binding (key matches the name), the V2 signature, AND that the EOL validity has not elapsed, against a `libp2p_identity::PeerId`. Its signature crypto is `libp2p-identity` (ed25519/secp256k1/ecdsa/rsa), and it sits on the `ipld-core 0.4` / `cid 0.11` / `multihash` / `quick-protobuf` / `chrono` lineage the repo's CAR path already binds. `Record::value()` returns the raw `/ipfs/<cid>` (or `/ipns/<name>`) path bytes. Source: `rust-ipns` docs.rs.

## UPDATE 2026-07-23: `rust-ipns 0.9.0`'s `verify` is INCOMPLETE for modern V2-only records

The "`rust-ipns` verifies all three in one call" claim above holds only for records that ALSO carry the deprecated top-level protobuf fields. Modern gateways serve **V2-only** records (IPIP-428) that omit them, and `Record::verify` then wrongly returns `DataMismatch`. This is exactly why `ronan.eth` failed. werust now normalizes the wire form before verifying (all crypto still in the crate). See `work/notes/findings/ronan-eth-ipns-v2-only-record-verify-bug-2026-07-23.md`.

## How werust used it

The `ipns-ns` ENS contenthash decodes to the canonical Base36 name; `werust_core::ipns::resolve_ipns_name` derives the `PeerId` from the name (the multihash bytes -> `PeerId::from_bytes`), fetches the record over an untrusted `IpnsRecordSource` (default: a trustless-gateway GET), `Record::decode` + `verify(peer_id)`, then canonicalises the `/ipfs/<cid>` target into `ipfs://<cid>` for the existing verified render path. A `/ipns/<name>` chain is refused (a named follow-on), never blindly followed. DNSLink (a `_dnslink` DNS-TXT lookup) is a DIFFERENT trust story and is deferred. Recorded as `docs/adr/0007` (approach) + `docs/adr/0006` (the honest mutable-name posture).
