# Spike: ronan.eth IPNS resolution failure + prominent load-failure surfacing

Task: `prominent-load-failure-and-ipns-resolution-diagnosis`
Spec: `ens-to-ipfs-resolution-phase1-rpc-skeleton`
Finding: `work/notes/findings/ronan-eth-ipns-v2-only-record-verify-bug-2026-07-23.md`

This directory captures the reproduction of the real `ronan.eth` failure the human hit, so it is auditable without re-running the live network.

## What `ronan.eth` resolves to

Captured with Foundry `cast` against a public Ethereum RPC:

```
$ cast namehash ronan.eth
0xecbbe339da8d5e78b3295a0ae79b9bc99ae09a002367653f16029c31ee66dee9

$ cast call --rpc-url <rpc> 0x00000000000C2E074eC69A0dFb2997BA6C7d2e1e "resolver(bytes32)(address)" <node>
0x231b0Ee14048e9dCcD1d247744d114a4EB5E8E63

$ cast call --rpc-url <rpc> 0x231b0Ee14048e9dCcD1d247744d114a4EB5E8E63 "contenthash(bytes32)(bytes)" <node>
0xe5010172002408011220e2bf9e13d80b02d817938692adf17e8cae6f6b2d46b39eb467c0b3a52cac2648
```

`0xe5` = `ipns-ns`. werust's decoder canonicalises the following libp2p-key CID to the Base36 name:

```
k51qzi5uqu5dlu1ien9spji7pu49mfw97mn0qv4azugqcvenj0dvzq9bgwp1zc
```

## The record werust fetches (captured here)

`ronan-eth.ipns-record` is the raw 221-byte record body from:

```
GET https://dweb.link/ipns/k51qzi5uqu5dlu1ien9spji7pu49mfw97mn0qv4azugqcvenj0dvzq9bgwp1zc?format=ipns-record
Accept: application/vnd.ipfs.ipns-record
# dweb.link 301-redirects to trustless-gateway.link; HTTP 200, 221 bytes
```

Byte-level protobuf `IpnsEntry` fields present in it:

- field 8 (`signatureV2`), 64 bytes
- field 9 (`data`, dag-cbor `Data` map), 152 bytes

Fields 1 (`value`), 2 (`signatureV1`), 3 (`validityType`), 4 (`validity`), 5 (`sequence`), 6 (`ttl`) are ABSENT. This is a **V2-only IPNS record** (IPIP-428). The CBOR `data` decodes to `Value=/ipfs/bafybeiepw4aijr4dtlhth2xkzskxaxcjvtk6neqsd6zua7rfv6m5nbkesu`, `Sequence=4`, `Validity=2026-07-25T00:00:10.503000000Z`.

## The bug

`rust-ipns 0.9.0` `Record::verify` rejects this valid record with `DataMismatch` ("dag-cbor data does not match the protobuf fields"), because its `data()` cross-check requires the deprecated top-level protobuf fields to be present and equal to the CBOR data. For a V2-only record they are absent. The V2 signature over the CBOR data is valid; only the redundant cross-check breaks. See the finding for the exact upstream code and the reasoning.

## The fix

A wire-normalization shim in `crates/werust-core/src/ipns.rs` (`normalize_ipns_record_bytes`): if the protobuf `value` field is empty but `data` (CBOR) is present, populate protobuf fields 1/3/4/5/6 from the CBOR `Data` (which the V2 signature already covers), re-encode, then decode + `verify` with the crate unchanged. All crypto stays in `rust-ipns` / `libp2p-identity`. Verified end to end against this exact captured record.

## Prominent failure surfacing (Part 1)

The other half of the task: a fail-closed load's reason (stored in `ChromeState::last_error`) was surfaced only in a subtle one-line status label, "not easily seen". Now the desktop chrome shows an in-view, high-contrast error banner (a prominent bar the user cannot miss), and the mobile shells (Android/iOS) show the same via their chrome-JSON `error` field, painted as a prominent error banner rather than a small footer status. Covered by tests in the core, the desktop `main.rs`, and the mobile ffi/edge.
