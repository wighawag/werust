//! ENS name resolution: turn a `name.eth` into a decoded contenthash reference
//! (an `ipfs://<cid>`) or a DISTINCT typed failure, by composing the
//! [`EthereumProvider`](crate::ethereum::EthereumProvider) seam with the ENSIP-7
//! [`decode_contenthash`](crate::contenthash::decode_contenthash) decoder.
//!
//! This is the pure resolution CORE the spec `ens-to-ipfs-resolution-phase1-rpc-skeleton`
//! (stories 1 + 3) calls for: it does NOT recognise the URL bar, rewrite the
//! address, or render anything (that is the separate front-door task). It is a
//! `name -> reference | typed failure` function behind the seam, exercised
//! entirely off the live network against pinned fixture RPC responses.
//!
//! # The resolution path
//!
//! 1. [`namehash`] the name (ENSIP-1: normalize the labels, then fold the
//!    normalized dotted name from the right with
//!    `namehash(node) = keccak256(parent_node ++ keccak256(label))`, base case the
//!    zero node) into the 32-byte `node`.
//! 2. `registry.resolver(node)` — an `eth_call` through the seam to the canonical
//!    mainnet ENS [`REGISTRY_ADDRESS`], ABI-decoding the returned resolver
//!    address. A zero/absent resolver is a distinct fail-closed error.
//! 3. `resolver.contenthash(node)` — an `eth_call` through the seam to THAT
//!    resolver (ENSIP-7 / EIP-1577), ABI-decoding the returned `bytes`.
//! 4. Hand those bytes to the ENSIP-7
//!    [`decode_contenthash`](crate::contenthash::decode_contenthash) decoder (we
//!    do NOT re-decode) and surface its typed output.
//!
//! # Fail-closed is a hard requirement (spec story 3)
//!
//! Every failure step is a DISTINCT [`ResolutionError`] variant, never a partial
//! or guessed result: an unnormalizable name
//! ([`UnnormalizableName`](ResolutionError::UnnormalizableName)), a zero/absent
//! resolver ([`NoResolver`](ResolutionError::NoResolver)), a reverting/empty or
//! unsupported contenthash ([`Contenthash`](ResolutionError::Contenthash) /
//! [`UnsupportedContenthash`](ResolutionError::UnsupportedContenthash)), an
//! RPC/seam error ([`Provider`](ResolutionError::Provider)), and a malformed
//! `eth_call` return ([`MalformedReturn`](ResolutionError::MalformedReturn)) each
//! surface distinctly. The one SUCCESS is a decoded `ipfs://<cid>` reference; a
//! well-formed-but-unsupported protocol (Swarm/IPNS/…) is a NAMED refusal, not a
//! success.
//!
//! # Trust honesty
//!
//! Resolution goes through the TRUSTED [`RpcProvider`](crate::ethereum::RpcProvider)
//! (Phase 1 has no light client), so an ENS-resolved page is labelled
//! "content-verified, name via TRUSTED RPC" upstream, never "verified". This
//! module makes no trust CLAIM; it just resolves through the seam, whose backend
//! carries the trust level (Phase 2 swaps a trustless light client behind it).
//!
//! # Bound primitives, never hand-rolled (`CONTEXT.md` / `docs/adr/0001`)
//!
//! keccak256 (for `namehash` and the function selectors) is the vetted
//! `sha3::Keccak256` (the LEGACY Keccak Ethereum uses, NOT NIST SHA3), and name
//! normalization is the vetted `ens-normalize` crate (a Rust port of adraffy's
//! reference normalizer). The ABI encode/decode here is only the trivial
//! fixed-shape cases ENS needs (a 4-byte selector + one 32-byte `bytes32` word;
//! a right-padded 20-byte address; one dynamic `bytes` return), decoded by hand
//! against the well-specified ABI layout rather than pulling a full ABI codec —
//! see the ABI decision in
//! `docs/spikes/ens-namehash-registry-resolver-contenthash-resolution/`.

use sha3::{Digest, Keccak256};

use crate::contenthash::{decode_contenthash, ContenthashError, DecodedContenthash, ProtoCode};
use crate::ethereum::{EthCall, EthereumProvider, ProviderError};

/// The canonical mainnet **ENS registry** contract address (`0x`-prefixed,
/// checksummed as it appears everywhere in the ENS ecosystem).
///
/// The registry is the well-known root contract whose `resolver(bytes32 node)`
/// returns the resolver address for a node; it has been at this SAME address on
/// Ethereum mainnet since the ENS redeploy and is treated as a constant (there is
/// no per-network config in Phase 1 — mainnet only, spec Out of Scope). Passed as
/// the `to` of the first `eth_call`.
pub const REGISTRY_ADDRESS: &str = "0x00000000000C2E074eC69A0dFb2997BA6C7d2e1e";

/// The 4-byte ABI function selector for the ENS registry's `resolver(bytes32)`
/// (the first 4 bytes of `keccak256("resolver(bytes32)")`).
///
/// A constant rather than computed per call: the signature is fixed and the
/// selector is well known. The test
/// `the_function_selectors_match_keccak_of_their_signatures` re-derives it from
/// the signature via the bound keccak so a typo cannot slip in silently.
const RESOLVER_SELECTOR: [u8; 4] = [0x01, 0x78, 0xb8, 0xbf];

/// The 4-byte ABI function selector for a resolver's `contenthash(bytes32)`
/// (ENSIP-7 / EIP-1577; the first 4 bytes of `keccak256("contenthash(bytes32)")`).
const CONTENTHASH_SELECTOR: [u8; 4] = [0xbc, 0x1c, 0x58, 0xd1];

/// A DISTINCT typed failure from resolving an ENS name to a contenthash
/// reference.
///
/// Fail-closed (spec story 3): every step that can fail has its OWN variant, so a
/// caller (the front-door / chrome task) turns each into a legible, specific load
/// failure and never renders a guessed or partial result. The one non-error
/// outcome of [`resolve`] is a decoded `ipfs://<cid>` reference; a well-formed
/// contenthash naming a protocol werust does not support (Swarm/IPNS/…) is the
/// distinct [`UnsupportedContenthash`](ResolutionError::UnsupportedContenthash),
/// NOT a success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionError {
    /// The name could not be normalized to a valid ENSIP-1 name (an empty label,
    /// a disallowed character, a failed normalization). Rejected before any
    /// `eth_call` — a name we cannot normalize has no well-defined `node`.
    UnnormalizableName {
        /// The name that failed to normalize.
        name: String,
        /// The normalizer's reason.
        detail: String,
    },
    /// A read through the [`EthereumProvider`](crate::ethereum::EthereumProvider)
    /// seam failed (a transport error, a JSON-RPC error object, a non-2xx, an
    /// unparseable RPC envelope). Carries the seam's own typed
    /// [`ProviderError`]. This is the "RPC/seam error" fail-closed step: the
    /// resolution simply did not complete, distinct from a name that resolved to
    /// "no resolver" or "no contenthash".
    Provider(ProviderError),
    /// An `eth_call` succeeded at the seam but its ABI-encoded return bytes were
    /// not the shape ENS expects (a `resolver(node)` return too short to hold an
    /// address, or a `contenthash(node)` return that is not a well-formed dynamic
    /// `bytes`). Refused rather than guessed.
    MalformedReturn(String),
    /// The registry returned the ZERO address for the node's resolver: the name
    /// has no resolver set (or does not exist). A distinct, common fail-closed
    /// case — there is nothing to ask for a contenthash.
    NoResolver,
    /// The resolver's `contenthash(node)` could not be decoded into a reference:
    /// the name has no contenthash set, or the returned bytes are malformed / an
    /// invalid CID. Carries the ENSIP-7 decoder's own typed
    /// [`ContenthashError`] so "no site set" stays distinct from "broken bytes"
    /// from "bad CID".
    Contenthash(ContenthashError),
    /// The resolver returned a WELL-FORMED contenthash, but for a protocol werust
    /// does not support in Phase 1 (Swarm, IPNS, Arweave, …): a NAMED refusal,
    /// never a mis-dispatch to `ipfs://`. Carries the detected
    /// [`ProtoCode`](crate::contenthash::ProtoCode) so the chrome can say "points
    /// to <protocol>, not supported".
    UnsupportedContenthash(ProtoCode),
}

impl std::fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolutionError::UnnormalizableName { name, detail } => {
                write!(f, "'{name}' is not a valid ENS name: {detail}")
            }
            ResolutionError::Provider(e) => write!(f, "ENS resolution failed: {e}"),
            ResolutionError::MalformedReturn(m) => {
                write!(f, "ENS resolution got a malformed contract return: {m}")
            }
            ResolutionError::NoResolver => {
                write!(f, "this name has no ENS resolver set")
            }
            ResolutionError::Contenthash(e) => write!(f, "{e}"),
            ResolutionError::UnsupportedContenthash(proto) => {
                // Reuse the decoder's own protocol-named reason so the message
                // matches the ENSIP-7 decoder's taxonomy exactly (no second
                // wording to drift).
                let reason = DecodedContenthash::Unsupported(*proto)
                    .reason()
                    .unwrap_or_else(|| "unsupported contenthash protocol".to_string());
                write!(f, "{reason}")
            }
        }
    }
}

impl std::error::Error for ResolutionError {}

/// keccak256 of `bytes` (the LEGACY Keccak Ethereum uses), via the bound
/// `sha3::Keccak256`.
fn keccak256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Compute the ENSIP-1 `namehash` of `name`: the 32-byte `node` an ENS read is
/// keyed on.
///
/// The algorithm (ENSIP-1): the empty name is the zero node; otherwise normalize
/// the name (ENSIP-15, via the bound `ens-normalize`) and fold its dotted labels
/// from the RIGHTMOST inward with
/// `node = keccak256(node ++ keccak256(label))`, starting from the zero node.
///
/// Normalization can FAIL (an empty label, a disallowed character): that surfaces
/// as [`ResolutionError::UnnormalizableName`] rather than a silently-mangled node
/// — the fail-closed "unnormalizable name" step. The empty string is the ENS root
/// (the zero node) and is returned directly (the normalizer rejects an empty
/// input, but the root node is well defined).
pub fn namehash(name: &str) -> Result<[u8; 32], ResolutionError> {
    let mut node = [0u8; 32];
    if name.is_empty() {
        return Ok(node);
    }

    let normalized =
        ens_normalize::normalize(name).map_err(|e| ResolutionError::UnnormalizableName {
            name: name.to_string(),
            detail: e.to_string(),
        })?;

    // Fold from the rightmost label inward: node = keccak256(node ++ keccak256(label)).
    for label in normalized.split('.').rev() {
        let label_hash = keccak256(label.as_bytes());
        let mut buf = [0u8; 64];
        buf[..32].copy_from_slice(&node);
        buf[32..].copy_from_slice(&label_hash);
        node = keccak256(&buf);
    }
    Ok(node)
}

/// ABI-encode the calldata for a `fn(bytes32)` call: the 4-byte `selector`
/// followed by the 32-byte `node` word, rendered as the `0x`-hex string an
/// [`EthCall`] carries as its `data`.
///
/// Both ENS reads this module issues (`resolver(bytes32)` and
/// `contenthash(bytes32)`) have this exact single-`bytes32`-argument shape, so
/// the encoding is a fixed 36-byte layout — no general ABI encoder needed.
fn encode_bytes32_call(selector: [u8; 4], node: &[u8; 32]) -> String {
    let mut data = Vec::with_capacity(4 + 32);
    data.extend_from_slice(&selector);
    data.extend_from_slice(node);
    let mut hex = String::with_capacity(2 + data.len() * 2);
    hex.push_str("0x");
    for b in &data {
        hex.push_str(&format!("{b:02x}"));
    }
    hex
}

/// ABI-decode a single `address` return (the `resolver(node)` result): a 20-byte
/// address right-aligned in a 32-byte word. Returns the `0x`-prefixed
/// lower-case 20-byte address string, or [`None`] if the return is the ZERO
/// address (no resolver set).
///
/// A return too short to hold a 32-byte word is a
/// [`ResolutionError::MalformedReturn`]; the top 12 bytes of the word must be
/// zero padding (they are for a real address) but we do not reject non-zero
/// padding — we take the low 20 bytes, matching how ABI addresses are laid out.
fn decode_address_return(bytes: &[u8]) -> Result<Option<String>, ResolutionError> {
    if bytes.len() < 32 {
        return Err(ResolutionError::MalformedReturn(format!(
            "resolver() return is {} bytes, expected a 32-byte address word",
            bytes.len()
        )));
    }
    // The address is the low 20 bytes of the 32-byte word.
    let addr = &bytes[12..32];
    if addr.iter().all(|&b| b == 0) {
        return Ok(None);
    }
    let mut hex = String::with_capacity(2 + 40);
    hex.push_str("0x");
    for b in addr {
        hex.push_str(&format!("{b:02x}"));
    }
    Ok(Some(hex))
}

/// ABI-decode a single dynamic `bytes` return (the `contenthash(node)` result)
/// into the raw contenthash byte string.
///
/// The ABI layout of a lone dynamic `bytes` return is: a 32-byte word holding the
/// offset to the data (0x20 for a single return), then at that offset a 32-byte
/// length word, then the `length` bytes of payload (zero-padded to a 32-byte
/// boundary, which we ignore). An EMPTY return (`0x`) — a resolver with no
/// `contenthash` support at all, or one that returned nothing — decodes to an
/// EMPTY byte string, which the ENSIP-7 decoder then reports as
/// [`ContenthashError::NoContenthash`]. Any structurally-impossible return (too
/// short for the offset/length words, an offset/length past the end) is a
/// [`ResolutionError::MalformedReturn`].
fn decode_bytes_return(bytes: &[u8]) -> Result<Vec<u8>, ResolutionError> {
    // An empty return is a legitimate "no contenthash bytes" and is passed
    // through empty (the decoder maps it to NoContenthash).
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if bytes.len() < 64 {
        return Err(ResolutionError::MalformedReturn(format!(
            "contenthash() return is {} bytes, too short for an ABI `bytes` (offset+length)",
            bytes.len()
        )));
    }
    let offset = read_u256_as_usize(&bytes[0..32])
        .ok_or_else(|| ResolutionError::MalformedReturn("bytes return offset overflows".into()))?;
    // The offset must land on a length word that fits in the return.
    if offset > bytes.len() || bytes.len() - offset < 32 {
        return Err(ResolutionError::MalformedReturn(format!(
            "bytes return offset {offset} is out of range for a {}-byte return",
            bytes.len()
        )));
    }
    let len = read_u256_as_usize(&bytes[offset..offset + 32])
        .ok_or_else(|| ResolutionError::MalformedReturn("bytes return length overflows".into()))?;
    let data_start = offset + 32;
    if bytes.len() - data_start < len {
        return Err(ResolutionError::MalformedReturn(format!(
            "bytes return claims {len} bytes but only {} follow the length word",
            bytes.len() - data_start
        )));
    }
    Ok(bytes[data_start..data_start + len].to_vec())
}

/// Read a 32-byte big-endian ABI word as a `usize`, returning [`None`] if the
/// value does not fit in a `usize` (its high bytes are non-zero). Used for the
/// ABI offset/length words, which for any real contenthash are small.
fn read_u256_as_usize(word: &[u8]) -> Option<usize> {
    debug_assert_eq!(word.len(), 32);
    let split = 32 - std::mem::size_of::<usize>();
    if word[..split].iter().any(|&b| b != 0) {
        return None;
    }
    let mut value: usize = 0;
    for &b in &word[split..] {
        value = (value << 8) | usize::from(b);
    }
    Some(value)
}

/// Resolve an ENS `name` to a decoded contenthash reference through the
/// [`EthereumProvider`](crate::ethereum::EthereumProvider) seam, or a DISTINCT
/// typed [`ResolutionError`].
///
/// The composed path (see the module docs): [`namehash`] the name, `eth_call`
/// the registry's `resolver(node)` (fail-closed on a zero resolver), `eth_call`
/// that resolver's `contenthash(node)`, then hand the returned bytes to the
/// ENSIP-7 [`decode_contenthash`](crate::contenthash::decode_contenthash) decoder
/// and surface its typed output.
///
/// On success returns [`DecodedContenthash::Ipfs`] — an `ipfs://<cid>` reference
/// ready to feed the existing verified `ipfs://` path. A well-formed
/// contenthash for an unsupported protocol is NOT returned as a success: it
/// surfaces as [`ResolutionError::UnsupportedContenthash`], so the caller cannot
/// accidentally treat a "points to Swarm" name as loadable. Every failure step is
/// its own variant (fail-closed), never a guessed or partial result.
///
/// `provider` is `&dyn` so the SAME resolution drives the Phase-1 trusted
/// [`RpcProvider`](crate::ethereum::RpcProvider) today and a Phase-2 trustless
/// light-client backend later, unchanged.
pub fn resolve(
    provider: &dyn EthereumProvider,
    name: &str,
) -> Result<DecodedContenthash, ResolutionError> {
    let node = namehash(name)?;

    // 1. registry.resolver(node) -> resolver address (or none).
    let resolver_return = provider
        .eth_call(&EthCall::new(
            REGISTRY_ADDRESS,
            &encode_bytes32_call(RESOLVER_SELECTOR, &node),
        ))
        .map_err(ResolutionError::Provider)?;
    let resolver_addr =
        decode_address_return(&resolver_return)?.ok_or(ResolutionError::NoResolver)?;

    // 2. resolver.contenthash(node) -> raw ENSIP-7 contenthash bytes.
    let contenthash_return = provider
        .eth_call(&EthCall::new(
            &resolver_addr,
            &encode_bytes32_call(CONTENTHASH_SELECTOR, &node),
        ))
        .map_err(ResolutionError::Provider)?;
    let contenthash_bytes = decode_bytes_return(&contenthash_return)?;

    // 3. Decode via the ENSIP-7 decoder (do NOT re-decode); map its typed output
    //    onto the resolution taxonomy. The two HANDLED cases — an immutable
    //    `ipfs-ns` and a mutable `ipns-ns` — are returned as successes for the
    //    front door to dispatch on (the `ipns-ns` name is RESOLVED via a
    //    client-verified record before it loads); a well-formed UNSUPPORTED
    //    protocol stays a NAMED refusal, never a success.
    match decode_contenthash(&contenthash_bytes) {
        Ok(decoded @ (DecodedContenthash::Ipfs { .. } | DecodedContenthash::Ipns { .. })) => {
            Ok(decoded)
        }
        Ok(DecodedContenthash::Unsupported(proto)) => {
            Err(ResolutionError::UnsupportedContenthash(proto))
        }
        Err(e) => Err(ResolutionError::Contenthash(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fetcher::{cid_v1_raw_sha256, Cid};
    use serde_json::json;
    use std::cell::RefCell;

    // -- Known-answer namehash vectors (ENSIP-1) -----------------------------

    fn hex32(bytes: &[u8; 32]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn namehash_matches_the_canonical_ensip1_known_answers() {
        // Acceptance: namehash computes the correct ENSIP-1 node, verified against
        // the canonical known-answer vectors from the ENSIP-1 spec.
        assert_eq!(
            namehash("").expect("the empty root normalizes"),
            [0u8; 32],
            "namehash('') is the zero node (the ENS root)"
        );
        assert_eq!(
            hex32(&namehash("eth").expect("eth")),
            "93cdeb708b7545dc668eb9280176169d1c33cfd8ed6f04690a0bcc88a93fc4ae",
            "namehash('eth')"
        );
        assert_eq!(
            hex32(&namehash("foo.eth").expect("foo.eth")),
            "de9b09fd7c5f901e23a3f19fecc54828e9c848539801e86591bd9801b019f84f",
            "namehash('foo.eth')"
        );
    }

    #[test]
    fn namehash_normalizes_case_before_hashing() {
        // ENSIP-1 folds the NORMALIZED name, so a mixed-case label hashes to the
        // same node as its lower-case form (proves normalization actually runs).
        assert_eq!(
            namehash("Foo.ETH").expect("mixed case normalizes"),
            namehash("foo.eth").expect("lower case"),
            "normalization makes case irrelevant to the node"
        );
    }

    #[test]
    fn an_unnormalizable_name_is_a_distinct_typed_failure() {
        // Fail-closed: a name that cannot be normalized (an empty label) is its
        // OWN typed error, rejected before any eth_call, never a mangled node.
        let err = namehash("a..b.eth").expect_err("an empty label is unnormalizable");
        assert!(
            matches!(err, ResolutionError::UnnormalizableName { .. }),
            "got: {err:?}"
        );
    }

    #[test]
    fn the_function_selectors_match_keccak_of_their_signatures() {
        // The selectors are constants; re-derive them from the signatures via the
        // BOUND keccak so a typo cannot slip in silently (and to prove the keccak
        // binding is the legacy-Keccak Ethereum uses, not NIST SHA3).
        assert_eq!(&keccak256(b"resolver(bytes32)")[..4], &RESOLVER_SELECTOR);
        assert_eq!(
            &keccak256(b"contenthash(bytes32)")[..4],
            &CONTENTHASH_SELECTOR
        );
    }

    // -- A scripted EthereumProvider double ----------------------------------

    /// An in-process [`EthereumProvider`] double: answers each `eth_call` in
    /// order from a queue of canned results and CAPTURES the calls it received,
    /// so a test can pin the two-step resolution (resolver then contenthash) with
    /// NO live network and assert the calldata that went to each contract. Mirrors
    /// the fixture-double style the `ethereum` / `ipfs` seam tests use.
    struct ScriptedProvider {
        answers: RefCell<std::collections::VecDeque<Result<Vec<u8>, ProviderError>>>,
        calls: RefCell<Vec<EthCall>>,
    }

    impl ScriptedProvider {
        fn new(answers: Vec<Result<Vec<u8>, ProviderError>>) -> Self {
            Self {
                answers: RefCell::new(answers.into_iter().collect()),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<EthCall> {
            self.calls.borrow().clone()
        }
    }

    impl EthereumProvider for ScriptedProvider {
        fn eth_call(&self, call: &EthCall) -> Result<Vec<u8>, ProviderError> {
            self.calls.borrow_mut().push(call.clone());
            self.answers
                .borrow_mut()
                .pop_front()
                .expect("the scripted provider ran out of canned answers")
        }
    }

    /// A 32-byte ABI word holding a right-aligned 20-byte address (the shape a
    /// `resolver(node)` return has).
    fn address_word(addr20: &[u8; 20]) -> Vec<u8> {
        let mut word = vec![0u8; 32];
        word[12..32].copy_from_slice(addr20);
        word
    }

    /// ABI-encode a dynamic `bytes` return the way a `contenthash(node)` result
    /// looks on the wire: an offset word (0x20), a length word, then the payload
    /// zero-padded to a 32-byte boundary.
    fn abi_bytes_return(payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        // offset = 0x20
        let mut offset = [0u8; 32];
        offset[31] = 0x20;
        out.extend_from_slice(&offset);
        // length
        let mut len = [0u8; 32];
        len[24..32].copy_from_slice(&(payload.len() as u64).to_be_bytes());
        out.extend_from_slice(&len);
        // payload, padded to 32
        out.extend_from_slice(payload);
        let pad = (32 - payload.len() % 32) % 32;
        out.extend(std::iter::repeat_n(0u8, pad));
        out
    }

    /// The raw ENSIP-7 `ipfs-ns` contenthash bytes (protoCode 0xe3 varint + CID
    /// bytes) plus the canonical CID string it decodes to — derived with the SAME
    /// `cid_v1_raw_sha256` helper the verified path uses, so the round-trip is
    /// honest.
    fn ipfs_contenthash_fixture() -> (Vec<u8>, String) {
        let cid_str = cid_v1_raw_sha256(b"ronan.eth's immutable site").expect("derive cid");
        let cid_bytes = Cid::try_from(cid_str.as_str())
            .expect("cid parses")
            .to_bytes();
        let mut ch = varint(0xe3); // ipfs-ns protoCode as an unsigned LEB128 varint
        ch.extend_from_slice(&cid_bytes);
        (ch, cid_str)
    }

    /// Encode a multicodec protoCode as an unsigned LEB128 varint, the real
    /// on-the-wire contenthash prefix (0xe3 etc. have the high bit set, so they
    /// are NOT single bytes) — mirrors the decoder module's own test builder.
    fn varint(mut code: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut byte = (code & 0x7f) as u8;
            code >>= 7;
            if code != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if code == 0 {
                break;
            }
        }
        out
    }

    #[test]
    fn a_known_fixture_name_resolves_end_to_end_to_an_ipfs_reference() {
        // Acceptance (the DONE bar): a known fixture name resolves end to end
        // through namehash -> resolver -> contenthash -> decode to a decoded
        // `ipfs://<cid>` reference, entirely off the network against pinned RPC
        // responses. Also asserts the two eth_calls carried the right target +
        // ABI calldata (namehashed node), so a mis-encoded call could not pass.
        let (contenthash_bytes, cid_str) = ipfs_contenthash_fixture();
        let resolver_addr = [0x11u8; 20];
        let provider = ScriptedProvider::new(vec![
            Ok(address_word(&resolver_addr)),
            Ok(abi_bytes_return(&contenthash_bytes)),
        ]);

        let decoded = resolve(&provider, "ronan.eth").expect("the fixture name resolves");
        assert_eq!(
            decoded,
            DecodedContenthash::Ipfs {
                uri: format!("ipfs://{cid_str}"),
                cid: cid_str.clone(),
            }
        );

        // The decoded reference feeds the EXISTING verified ipfs:// parser with no
        // skew.
        let parsed = crate::ipfs::parse_ipfs_uri(&format!("ipfs://{cid_str}"))
            .expect("the resolved ipfs uri parses on the verified path");
        assert_eq!(parsed.cid, cid_str);

        // The two calls went to the right contracts with the right calldata.
        let calls = provider.calls();
        assert_eq!(calls.len(), 2, "resolver() then contenthash()");
        let node = namehash("ronan.eth").unwrap();
        assert_eq!(
            calls[0].to, REGISTRY_ADDRESS,
            "resolver() hits the registry"
        );
        assert_eq!(
            calls[0].data,
            encode_bytes32_call(RESOLVER_SELECTOR, &node),
            "resolver() calldata is the selector + namehashed node"
        );
        assert_eq!(
            calls[1].to, "0x1111111111111111111111111111111111111111",
            "contenthash() hits the resolver the registry returned"
        );
        assert_eq!(
            calls[1].data,
            encode_bytes32_call(CONTENTHASH_SELECTOR, &node),
            "contenthash() calldata is the selector + the SAME node"
        );
    }

    #[test]
    fn a_zero_resolver_is_a_distinct_no_resolver_failure() {
        // Fail-closed: the registry returning the ZERO address (no resolver set /
        // name does not exist) is its OWN typed error, and does NOT issue a
        // contenthash() call.
        let provider = ScriptedProvider::new(vec![Ok(address_word(&[0u8; 20]))]);
        let err = resolve(&provider, "no-resolver.eth").expect_err("zero resolver fails closed");
        assert_eq!(err, ResolutionError::NoResolver);
        assert_eq!(
            provider.calls().len(),
            1,
            "a missing resolver short-circuits before contenthash()"
        );
    }

    #[test]
    fn an_empty_contenthash_is_a_distinct_no_contenthash_failure() {
        // Fail-closed: a resolver that returns an empty `bytes` (no contenthash
        // record) surfaces the decoder's NoContenthash as a distinct resolution
        // error, never a guess.
        let provider = ScriptedProvider::new(vec![
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&[])),
        ]);
        let err = resolve(&provider, "empty.eth").expect_err("no contenthash fails closed");
        assert_eq!(
            err,
            ResolutionError::Contenthash(ContenthashError::NoContenthash)
        );
    }

    #[test]
    fn a_completely_empty_contenthash_return_is_no_contenthash_not_malformed() {
        // A resolver contract that does not implement contenthash at all typically
        // reverts (a Provider error) — but a `0x` empty return is a legitimate
        // "no bytes" and must reach the decoder as NoContenthash, not a malformed
        // ABI decode.
        let provider = ScriptedProvider::new(vec![Ok(address_word(&[0x22u8; 20])), Ok(Vec::new())]);
        let err = resolve(&provider, "bare.eth").expect_err("empty return");
        assert_eq!(
            err,
            ResolutionError::Contenthash(ContenthashError::NoContenthash)
        );
    }

    #[test]
    fn a_reverting_resolver_is_a_distinct_provider_failure() {
        // Fail-closed: a resolver whose contenthash() REVERTS surfaces as a
        // Provider (RPC/seam) error carrying the endpoint's own reason, distinct
        // from a "no contenthash set" or a decode failure.
        let provider = ScriptedProvider::new(vec![
            Ok(address_word(&[0x33u8; 20])),
            Err(ProviderError::Rpc {
                code: -32000,
                message: "execution reverted".to_string(),
            }),
        ]);
        let err = resolve(&provider, "reverts.eth").expect_err("a revert fails closed");
        assert!(
            matches!(err, ResolutionError::Provider(ProviderError::Rpc { .. })),
            "got: {err:?}"
        );
    }

    #[test]
    fn an_rpc_error_on_the_resolver_lookup_is_a_distinct_provider_failure() {
        // Fail-closed: an RPC/seam error on the FIRST call (resolver lookup) is a
        // Provider error and never proceeds to contenthash().
        let provider = ScriptedProvider::new(vec![Err(ProviderError::Transport(
            "connection refused".to_string(),
        ))]);
        let err = resolve(&provider, "unreachable.eth").expect_err("rpc error fails closed");
        assert!(
            matches!(err, ResolutionError::Provider(ProviderError::Transport(_))),
            "got: {err:?}"
        );
        assert_eq!(
            provider.calls().len(),
            1,
            "no contenthash() after an rpc error"
        );
    }

    #[test]
    fn an_unsupported_protocol_contenthash_is_a_distinct_named_refusal() {
        // Fail-closed: a well-formed contenthash for a protocol werust does not
        // support (Swarm here) is a DISTINCT named refusal, never a success and
        // never mis-dispatched to ipfs://.
        let mut swarm_ch = varint(0xe4); // swarm-ns protoCode as a varint
        swarm_ch.extend_from_slice(b"some swarm address bytes");
        let provider = ScriptedProvider::new(vec![
            Ok(address_word(&[0x44u8; 20])),
            Ok(abi_bytes_return(&swarm_ch)),
        ]);
        let err = resolve(&provider, "swarm-site.eth").expect_err("swarm is not supported");
        assert_eq!(
            err,
            ResolutionError::UnsupportedContenthash(ProtoCode::Swarm)
        );
        assert_eq!(err.to_string(), "points to Swarm, not supported");
    }

    #[test]
    fn a_malformed_contenthash_return_is_a_distinct_failure() {
        // Fail-closed: a contenthash() return whose ABI `bytes` framing is broken
        // (an offset past the end) is a MalformedReturn, distinct from a decoder
        // error, never a guess.
        let mut broken = vec![0u8; 32];
        broken[31] = 0xff; // offset 255, past the end of a 32-byte return
        let provider = ScriptedProvider::new(vec![Ok(address_word(&[0x55u8; 20])), Ok(broken)]);
        let err = resolve(&provider, "broken.eth").expect_err("a broken abi return fails closed");
        assert!(
            matches!(err, ResolutionError::MalformedReturn(_)),
            "got: {err:?}"
        );
    }

    #[test]
    fn a_short_resolver_return_is_a_distinct_malformed_failure() {
        // Fail-closed: a resolver() return too short to hold a 32-byte address
        // word is a MalformedReturn (refused), never treated as the zero address.
        let provider = ScriptedProvider::new(vec![Ok(vec![0u8; 10])]);
        let err = resolve(&provider, "short.eth").expect_err("a short return fails closed");
        assert!(
            matches!(err, ResolutionError::MalformedReturn(_)),
            "got: {err:?}"
        );
    }

    #[test]
    fn resolution_end_to_end_over_the_bound_rpc_transport_off_the_network() {
        // The full production shape headless and off the live network: resolution
        // driving the REAL RpcProvider over a loopback JSON-RPC fixture endpoint
        // that answers the resolver() then contenthash() calls in order. Proves
        // the whole composed path (namehash -> two eth_calls through the seam ->
        // ENSIP-7 decode) works through the actual bound transport, not just an
        // in-process EthereumProvider double.
        use crate::ethereum::RpcProvider;

        let (contenthash_bytes, cid_str) = ipfs_contenthash_fixture();
        let resolver_addr = address_word(&[0x66u8; 20]);
        let ch_return = abi_bytes_return(&contenthash_bytes);

        // Two canned JSON-RPC `result` bodies, served in call order.
        let bodies = vec![
            json!({ "jsonrpc": "2.0", "id": 1, "result": to_hex(&resolver_addr) })
                .to_string()
                .into_bytes(),
            json!({ "jsonrpc": "2.0", "id": 1, "result": to_hex(&ch_return) })
                .to_string()
                .into_bytes(),
        ];
        let server = SequencedRpcServer::start(bodies);
        let provider = RpcProvider::with_endpoint(&server.endpoint());

        let decoded = resolve(&provider, "ronan.eth").expect("resolves over the bound transport");
        assert_eq!(
            decoded,
            DecodedContenthash::Ipfs {
                uri: format!("ipfs://{cid_str}"),
                cid: cid_str,
            }
        );

        // Both requests went over the wire as eth_calls (method + params), proving
        // the whole path is real, not short-circuited.
        let sent = server.captured_request_bodies();
        assert_eq!(sent.len(), 2, "resolver() then contenthash() over the wire");
        for body in &sent {
            let v: serde_json::Value = serde_json::from_slice(body).expect("wire body is json");
            assert_eq!(v["method"], "eth_call");
        }
    }

    fn to_hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(2 + bytes.len() * 2);
        s.push_str("0x");
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    // A loopback JSON-RPC endpoint that answers a SEQUENCE of canned bodies (one
    // per accepted request, in order) and captures each request body — the ENS
    // two-call analogue of the `ethereum` module's single-answer LocalRpcServer,
    // off the live network. Torn down on Drop.
    use std::io::Write;
    use std::net::{SocketAddr, TcpListener};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    struct SequencedRpcServer {
        addr: SocketAddr,
        shutdown: Arc<AtomicBool>,
        requests: Arc<Mutex<Vec<Vec<u8>>>>,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl SequencedRpcServer {
        fn start(bodies: Vec<Vec<u8>>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            listener.set_nonblocking(true).expect("non-blocking");
            let addr = listener.local_addr().expect("addr");
            let shutdown = Arc::new(AtomicBool::new(false));
            let stop = shutdown.clone();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let captured = requests.clone();
            let next = Arc::new(AtomicUsize::new(0));
            let handle = thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let _ = stream.set_nonblocking(false);
                            // Drain the COMPLETE request (head + full
                            // `Content-Length` body) before responding, via the
                            // shared race-hardened reader
                            // (`crate::loopback_test_server`), so each captured
                            // request body carries the whole `eth_call` JSON even
                            // when the body arrives in a later TCP segment under
                            // parallel load.
                            if let Some(body) =
                                crate::loopback_test_server::read_request_body(&mut stream)
                            {
                                captured.lock().unwrap().push(body);
                            }
                            let idx = next.fetch_add(1, Ordering::Relaxed);
                            let empty = Vec::new();
                            let body = bodies.get(idx).unwrap_or(&empty);
                            let head = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let _ = stream.write_all(head.as_bytes());
                            let _ = stream.write_all(body);
                            let _ = stream.flush();
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            });
            Self {
                addr,
                shutdown,
                requests,
                handle: Some(handle),
            }
        }

        fn endpoint(&self) -> String {
            format!("http://{}/", self.addr)
        }

        fn captured_request_bodies(&self) -> Vec<Vec<u8>> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl Drop for SequencedRpcServer {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }
}
