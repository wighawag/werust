//! The ENSIP-7 / EIP-1577 `contenthash` decoder: a PURE, offline
//! byte -> typed-enum decoder that dispatches by the contenthash's OWN
//! multicodec protoCode and produces graceful, protocol-NAMED refusals for every
//! protocol werust does not yet support.
//!
//! # What a contenthash is
//!
//! An ENSIP-7 contenthash (the bytes an ENS resolver's `contenthash(node)`
//! returns) is a multicodec-prefixed byte string: a leading unsigned varint —
//! the **protoCode** — names the protocol, and the remainder is that protocol's
//! own address. `0xe3` = `ipfs-ns` (an IPFS CID follows), `0xe5` = `ipns-ns`
//! (mutable IPNS), `0xe4` = `swarm-ns`, plus Arweave / `onion` / `onion3` /
//! Skynet / ZeroNet / DNSLink and others (see [`ProtoCode`]).
//!
//! # The whole point: dispatch by the contenthash's own type, never guess
//!
//! Only `ipfs-ns` is SUPPORTED in Phase 1 (`CONTEXT.md`, spec
//! `ens-to-ipfs-resolution-phase1-rpc-skeleton`): it decodes to an
//! `ipfs://<cid>` reference the existing verified `ipfs://` path already consumes.
//! Every OTHER protoCode is a DISTINCT, protocol-named typed variant that maps to
//! a legible user-facing load failure. This mirrors the verified path's
//! "reject-when-unsure, name the reason" discipline (`docs/adr/0001`): the decoder
//! NEVER defaults an unrecognised protoCode to `ipfs://`, never mis-dispatches,
//! never panics on malformed input, and never fails blankly — an unsupported name
//! fails with a clear "points to <protocol>, not supported" message.
//!
//! RESOLVING the non-IPFS protocols is explicitly OUT OF SCOPE (spec Out of
//! Scope); DETECTING them and erroring clearly is the whole job. This module is
//! byte-in, typed-enum-out (no network, no seam), so the ENS resolution task can
//! consume [`decode_contenthash`]'s output and the URL-bar task can turn each
//! variant into a legible chrome failure.
//!
//! # The `ipfs-ns` CID canonicalization decision
//!
//! The `ipfs-ns` case parses the CID bytes that follow the protoCode with the
//! vetted [`Cid`](fetcher::Cid) crate RE-EXPORTED by the `fetcher` crate — the
//! SAME type the hash-verified path verifies against — and renders it with
//! [`Cid::to_string`], the crate's canonical multibase form (base32 for CIDv1,
//! base58btc for CIDv0). That is exactly the string
//! [`parse_ipfs_uri`](crate::ipfs::parse_ipfs_uri) accepts and
//! [`fetch_verified`](fetcher::ContentAddressedFetcher::fetch_verified) parses via
//! `Cid::try_from(&str)`, so the decoded `ipfs://<cid>` feeds the existing path
//! with NO version skew and NO hand-rolled byte layout. See the round-trip test
//! `an_ipfs_ns_cid_decodes_to_the_canonical_string_the_verified_path_consumes`.

use fetcher::Cid;

/// The multicodec protoCode that leads an ENSIP-7 contenthash, decoded into the
/// protocols werust names.
///
/// The leading varint of a contenthash names its protocol. This enum is the
/// closed set werust gives a specific NAME (so an unsupported name fails with
/// "points to <protocol>, not supported" rather than a blank code); any protoCode
/// NOT in this set is carried as [`ProtoCode::Unknown`] and reported by its raw
/// hex value. The values are the authoritative multicodec table codes
/// (`https://github.com/multiformats/multicodec`).
///
/// Only [`Ipfs`](ProtoCode::Ipfs) is supported in Phase 1; the rest exist so the
/// decoder can name the protocol it detected in a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtoCode {
    /// `ipfs-ns` (`0xe3`): an immutable IPFS CID follows. The one SUPPORTED code.
    Ipfs,
    /// `swarm-ns` (`0xe4`): a Swarm address.
    Swarm,
    /// `ipns-ns` (`0xe5`): a MUTABLE IPNS pointer. Now HANDLED (via resolution),
    /// so it decodes to [`DecodedContenthash::Ipns`] rather than an
    /// [`Unsupported`](DecodedContenthash::Unsupported) refusal; this variant
    /// remains only for the multicodec name table.
    Ipns,
    /// `zeronet` (`0xe6`): a ZeroNet site address.
    ZeroNet,
    /// `dnslink` (`0xe8`): a DNSLink path.
    DnsLink,
    /// `onion` (`0x01bc`): a Tor v2 onion address.
    Onion,
    /// `onion3` (`0x01bd`): a Tor v3 onion address.
    Onion3,
    /// `skynet-ns` (`0xb19910`): a Skynet address.
    Skynet,
    /// `arweave-ns` (`0xb29910`): an Arweave address.
    Arweave,
    /// A protoCode werust does not name, carried by its raw multicodec value so
    /// the refusal can still report it (`0x…`).
    Unknown {
        /// The raw multicodec protoCode value read from the contenthash.
        code: u64,
    },
}

impl ProtoCode {
    /// Classify a raw multicodec value into the named set, or
    /// [`Unknown`](ProtoCode::Unknown).
    #[must_use]
    fn from_code(code: u64) -> Self {
        match code {
            0xe3 => ProtoCode::Ipfs,
            0xe4 => ProtoCode::Swarm,
            0xe5 => ProtoCode::Ipns,
            0xe6 => ProtoCode::ZeroNet,
            0xe8 => ProtoCode::DnsLink,
            0x01bc => ProtoCode::Onion,
            0x01bd => ProtoCode::Onion3,
            0xb19910 => ProtoCode::Skynet,
            0xb29910 => ProtoCode::Arweave,
            other => ProtoCode::Unknown { code: other },
        }
    }

    /// The human-readable protocol name used in a "points to <protocol>" refusal.
    #[must_use]
    fn display_name(self) -> &'static str {
        match self {
            ProtoCode::Ipfs => "IPFS",
            ProtoCode::Swarm => "Swarm",
            ProtoCode::Ipns => "IPNS",
            ProtoCode::ZeroNet => "ZeroNet",
            ProtoCode::DnsLink => "DNSLink",
            ProtoCode::Onion => "Tor onion",
            ProtoCode::Onion3 => "Tor onion v3",
            ProtoCode::Skynet => "Skynet",
            ProtoCode::Arweave => "Arweave",
            ProtoCode::Unknown { .. } => "an unknown protocol",
        }
    }
}

/// A successfully decoded ENSIP-7 contenthash.
///
/// Phase 1 only ever produces [`Ipfs`](DecodedContenthash::Ipfs) as a SUPPORTED
/// success; every other well-formed protoCode decodes to
/// [`Unsupported`](DecodedContenthash::Unsupported), a typed refusal that NAMES
/// the protocol it detected. This is the "typed enum keyed by protoCode" the
/// consuming tasks dispatch on: `ipfs-ns` feeds the verified `ipfs://` path,
/// everything else becomes a legible chrome failure via [`reason`](DecodedContenthash::reason).
///
/// Undecodable input never reaches this type: it surfaces as a
/// [`ContenthashError`] instead (a [`decode_contenthash`] `Err`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodedContenthash {
    /// `ipfs-ns` (`0xe3`): the SUPPORTED case. Carries the `ipfs://<cid>`
    /// reference (the CID rendered as the canonical string the existing verified
    /// `ipfs://` path consumes).
    Ipfs {
        /// The `ipfs://<cid>` URI, ready to feed the verified `ipfs://` path.
        uri: String,
        /// The canonical CID string (the `<cid>` in the URI), for callers that
        /// want the identifier without the scheme.
        cid: String,
    },
    /// `ipns-ns` (`0xe5`): a MUTABLE IPNS pointer. NOT directly loadable like
    /// `ipfs-ns` — it must first be RESOLVED (a client-verified IPNS record maps
    /// the name to its current `/ipfs/<cid>`) before its CID feeds the verified
    /// `ipfs://` path. Carries the canonical libp2p-key IPNS `name` (a base36
    /// `k…` CIDv1, the string a trustless gateway accepts at `GET /ipns/{name}`),
    /// which the front door hands to [`crate::ipns::resolve_ipns_name`]. This is
    /// the ONCE-refused, now-handled case (`docs/adr/0007`): distinct from the
    /// immutable [`Ipfs`](DecodedContenthash::Ipfs) case so the resolved page can
    /// carry the honest MUTABLE-name trust posture.
    Ipns {
        /// The canonical libp2p-key IPNS name (a base36 `k…` CIDv1), ready to
        /// resolve via a verifiable IPNS record.
        name: String,
    },
    /// A well-formed contenthash whose protoCode is NOT `ipfs-ns`/`ipns-ns`:
    /// detected and named, but not supported (nor mis-dispatched to `ipfs://`).
    /// The [`ProtoCode`] carries which protocol it is (named or
    /// [`Unknown`](ProtoCode::Unknown)).
    Unsupported(ProtoCode),
}

impl DecodedContenthash {
    /// Whether this is the SUPPORTED `ipfs-ns` case (as opposed to a named-but-
    /// unsupported protocol).
    #[must_use]
    pub fn is_supported(&self) -> bool {
        matches!(self, DecodedContenthash::Ipfs { .. })
    }

    /// Whether this is the `ipns-ns` case: a MUTABLE IPNS name that must be
    /// RESOLVED (via a client-verified IPNS record) before its CID feeds the
    /// verified `ipfs://` path. Distinct from [`is_supported`](DecodedContenthash::is_supported)
    /// (the directly-loadable immutable `ipfs-ns` case): the front door dispatches
    /// an `Ipns` into [`crate::ipns::resolve_ipns_name`], not straight into the
    /// `ipfs://` load.
    #[must_use]
    pub fn is_ipns(&self) -> bool {
        matches!(self, DecodedContenthash::Ipns { .. })
    }

    /// A legible, protocol-named reason for an [`Unsupported`](DecodedContenthash::Unsupported)
    /// contenthash, for the URL bar / chrome to surface as the load failure.
    ///
    /// Returns `None` for the SUPPORTED [`Ipfs`](DecodedContenthash::Ipfs) case
    /// (there is nothing to refuse). For every unsupported protocol it NAMES the
    /// protocol ("points to Swarm, not supported"), and the mutable-pointer IPNS
    /// case says so specifically; an [`Unknown`](ProtoCode::Unknown) protoCode is
    /// reported by its raw hex value.
    #[must_use]
    pub fn reason(&self) -> Option<String> {
        match self {
            // The two HANDLED cases have no refusal: `ipfs-ns` loads directly,
            // `ipns-ns` is resolved (via a client-verified record) then loaded.
            DecodedContenthash::Ipfs { .. } | DecodedContenthash::Ipns { .. } => None,
            DecodedContenthash::Unsupported(ProtoCode::Unknown { code }) => Some(format!(
                "unsupported/unknown contenthash protocol ({code:#x})"
            )),
            DecodedContenthash::Unsupported(proto) => {
                Some(format!("points to {}, not supported", proto.display_name()))
            }
        }
    }
}

/// A contenthash that could NOT be decoded into a [`DecodedContenthash`].
///
/// Kept distinct from an [`Unsupported`](DecodedContenthash::Unsupported)
/// success: an unsupported contenthash was well-formed (we KNOW which protocol it
/// names, we just do not support it), whereas these are inputs we cannot decode
/// at all. Both fail the load with their own distinct message, but the caller can
/// tell "no site set for this name" ([`NoContenthash`](ContenthashError::NoContenthash))
/// from "the bytes are broken" ([`Malformed`](ContenthashError::Malformed)) from
/// "the IPFS CID itself is invalid" ([`InvalidCid`](ContenthashError::InvalidCid)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContenthashError {
    /// The contenthash is empty/absent: the name has no contenthash record set.
    /// A distinct, common case (an ENS name with no site) so the chrome can say
    /// "this name has no content set" rather than "broken".
    NoContenthash,
    /// The bytes are not a decodable ENSIP-7 contenthash: the leading protoCode
    /// varint is truncated/overlong, or the payload after a known protoCode is
    /// structurally undecodable.
    Malformed(String),
    /// The protoCode was `ipfs-ns` but the bytes that follow are not a valid CID:
    /// the one SUPPORTED protocol carried an unparseable identifier, so it cannot
    /// yield an `ipfs://<cid>`. Distinct from [`Malformed`](ContenthashError::Malformed)
    /// so an IPFS contenthash with a broken CID is legible as such.
    InvalidCid(String),
    /// The protoCode was `ipns-ns` but the bytes that follow are not a valid
    /// libp2p-key IPNS name: either they do not parse as a CID at all, or they
    /// parse as a CID whose codec is NOT `libp2p-key` (`0x72`) — so it cannot name
    /// a key to resolve a record against. Refused rather than guessed, and kept
    /// distinct from [`InvalidCid`](ContenthashError::InvalidCid) (the immutable
    /// `ipfs-ns` case) so a broken IPNS pointer is legible as such.
    InvalidIpnsName(String),
}

impl std::fmt::Display for ContenthashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContenthashError::NoContenthash => {
                write!(f, "this name has no contenthash set")
            }
            ContenthashError::Malformed(detail) => {
                write!(f, "malformed contenthash: {detail}")
            }
            ContenthashError::InvalidCid(detail) => {
                write!(f, "ipfs contenthash carries an invalid CID: {detail}")
            }
            ContenthashError::InvalidIpnsName(detail) => {
                write!(
                    f,
                    "ipns contenthash carries an invalid libp2p-key name: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for ContenthashError {}

/// The `ipfs` scheme the SUPPORTED `ipfs-ns` case decodes to (kept in sync with
/// [`crate::ipfs::IPFS_SCHEME`], which the produced URI feeds).
const IPFS_SCHEME: &str = "ipfs";

/// The `libp2p-key` IPLD multicodec (`0x72`): an IPNS name is a CIDv1 with this
/// codec, wrapping the key's multihash (the `PeerId`). A CID under any OTHER
/// codec is NOT an IPNS name and is refused (never resolved as if it were a key).
const LIBP2P_KEY_CODEC: u64 = 0x72;

/// Canonicalise an `ipns-ns` payload (the bytes after the `0xe5` protoCode) into
/// the base36 `k…` libp2p-key IPNS name a trustless gateway accepts at
/// `GET /ipns/{name}`.
///
/// The payload is a libp2p-key CID (parsed with the vetted `cid` crate, the SAME
/// lineage the verified path and the IPNS resolver use — no hand-rolled byte
/// layout). Its codec MUST be `libp2p-key` (`0x72`): a CID under any other codec
/// (e.g. a raw/ipfs CID) is refused as a distinct
/// [`ContenthashError::InvalidIpnsName`], never treated as a key. The name is
/// rendered case-insensitive base36 (the IPNS spec's suggested default), which
/// [`crate::ipns::resolve_ipns_name`] re-parses to the same key.
fn canonical_ipns_name(payload: &[u8]) -> Result<String, ContenthashError> {
    let cid =
        Cid::try_from(payload).map_err(|e| ContenthashError::InvalidIpnsName(e.to_string()))?;
    if cid.codec() != LIBP2P_KEY_CODEC {
        return Err(ContenthashError::InvalidIpnsName(format!(
            "cid codec {codec:#x} is not libp2p-key (0x72)",
            codec = cid.codec()
        )));
    }
    cid.to_string_of_base(cid::multibase::Base::Base36Lower)
        .map_err(|e| ContenthashError::InvalidIpnsName(e.to_string()))
}

/// Read one unsigned LEB128 varint (the multicodec protoCode) from the front of
/// `bytes`, returning the value and the number of bytes consumed.
///
/// The protoCode is a self-contained unsigned varint at the head of the
/// contenthash; the CID/address that follows is decoded separately (for
/// `ipfs-ns`, by the vetted `Cid` parser). We read only this ONE varint by hand
/// — it is a bounded, well-specified integer decode, not a cryptographic
/// primitive or byte layout that could drift — and delegate all CID parsing to
/// the `cid` crate. A truncated (input ends mid-varint) or overlong (more than
/// the 9 bytes a `u64` needs, i.e. a non-canonical/oversized value) varint is
/// rejected rather than guessed, so malformed input never panics or mis-decodes.
fn read_protocode_varint(bytes: &[u8]) -> Result<(u64, usize), ContenthashError> {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        // A u64 varint is at most 10 bytes; but the multicodec protoCodes we care
        // about all fit well under u64::MAX, and a shift past 63 bits is overlong.
        if shift >= 64 {
            return Err(ContenthashError::Malformed(
                "protoCode varint is overlong (exceeds u64)".to_string(),
            ));
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok((value, i + 1));
        }
        shift += 7;
    }
    // Ran off the end with the continuation bit still set: truncated.
    Err(ContenthashError::Malformed(
        "protoCode varint is truncated".to_string(),
    ))
}

/// Decode an ENSIP-7 / EIP-1577 contenthash byte string into a typed
/// [`DecodedContenthash`], dispatching by its OWN multicodec protoCode.
///
/// This is the pure heart of the task: byte-in, typed-enum-out, no network and no
/// seam. It reads the leading protoCode varint, classifies it ([`ProtoCode`]),
/// and:
///
/// * `ipfs-ns` (`0xe3`) -> parses the following bytes as a [`Cid`](fetcher::Cid)
///   (the vetted crate the verified path uses) and returns a
///   [`DecodedContenthash::Ipfs`] carrying `ipfs://<canonical-cid>` — the SUPPORTED
///   case. A protoCode of `ipfs-ns` with unparseable CID bytes is
///   [`ContenthashError::InvalidCid`], NOT a silent guess.
/// * `ipns-ns` (`0xe5`) -> canonicalises the following libp2p-key CID into a
///   base36 IPNS `name` and returns a [`DecodedContenthash::Ipns`], the MUTABLE
///   case the front door RESOLVES (via a client-verified IPNS record) before
///   loading. A payload that is not a valid libp2p-key CID is
///   [`ContenthashError::InvalidIpnsName`], NOT a silent guess.
/// * any OTHER protoCode -> [`DecodedContenthash::Unsupported`] naming the
///   protocol (the caller turns it into a "points to <protocol>, not supported"
///   load failure). It NEVER defaults to `ipfs://`.
///
/// Empty input is [`ContenthashError::NoContenthash`]; a truncated/overlong
/// protoCode varint is [`ContenthashError::Malformed`]. The function never panics
/// on malformed input.
pub fn decode_contenthash(bytes: &[u8]) -> Result<DecodedContenthash, ContenthashError> {
    // Empty/absent is its own case: the name simply has no contenthash set.
    if bytes.is_empty() {
        return Err(ContenthashError::NoContenthash);
    }

    let (code, consumed) = read_protocode_varint(bytes)?;
    let payload = &bytes[consumed..];

    match ProtoCode::from_code(code) {
        ProtoCode::Ipfs => {
            // The bytes after the protoCode ARE a CID; parse with the vetted crate
            // (no hand-rolled byte layout) and render its canonical string — the
            // exact form the verified `ipfs://` path consumes.
            let cid = Cid::try_from(payload)
                .map_err(|e| ContenthashError::InvalidCid(e.to_string()))?
                .to_string();
            let uri = format!("{IPFS_SCHEME}://{cid}");
            Ok(DecodedContenthash::Ipfs { uri, cid })
        }
        ProtoCode::Ipns => {
            // The bytes after the protoCode are a libp2p-key CID naming the IPNS
            // key. Canonicalise it to the base36 name a gateway accepts; the
            // front door hands it to `ipns::resolve_ipns_name` (which fetches +
            // client-verifies the record) rather than loading it directly. A
            // payload that is not a valid libp2p-key CID is a distinct
            // `InvalidIpnsName`, NOT a silent guess.
            let name = canonical_ipns_name(payload)?;
            Ok(DecodedContenthash::Ipns { name })
        }
        // Every other protoCode is detected and NAMED, never resolved and never
        // mis-dispatched to ipfs://. The payload is not inspected: Phase 1 only
        // needs to refuse it legibly, not parse a Swarm/Arweave/onion address.
        other => Ok(DecodedContenthash::Unsupported(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fetcher::cid_v1_raw_sha256;

    /// Build a contenthash: the protoCode as a leading varint, then the payload.
    /// The multicodec codes we test are all <= 3 bytes of varint; this encodes an
    /// unsigned LEB128 varint the way a real contenthash is prefixed, so the
    /// fixtures are the real on-the-wire shape, not a hand-waved stand-in.
    fn contenthash(proto_code: u64, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut code = proto_code;
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
        out.extend_from_slice(payload);
        out
    }

    /// A real CIDv1 (raw codec, sha2-256) over some bytes, both as its canonical
    /// string AND as the raw CID bytes an `ipfs-ns` contenthash carries — derived
    /// with the SAME helper the verified path uses, so the round-trip is honest.
    fn fixture_cid() -> (String, Vec<u8>) {
        let cid_str = cid_v1_raw_sha256(b"an immutable ipfs site").expect("derive fixture cid");
        let cid_bytes = Cid::try_from(cid_str.as_str())
            .expect("the derived cid parses")
            .to_bytes();
        (cid_str, cid_bytes)
    }

    #[test]
    fn an_ipfs_ns_cid_decodes_to_the_canonical_string_the_verified_path_consumes() {
        // Acceptance: `ipfs-ns` (0xe3) decodes to an `ipfs://<cid>` reference whose
        // CID string is EXACTLY what the existing verified `ipfs://` path consumes.
        // We derive the CID with the same `cid_v1_raw_sha256` helper the fetcher /
        // ipfs path uses, so the decoded string is provably the canonical form
        // `parse_ipfs_uri` / `fetch_verified` accept via `Cid::try_from(&str)`.
        let (cid_str, cid_bytes) = fixture_cid();
        let bytes = contenthash(0xe3, &cid_bytes);

        let decoded = decode_contenthash(&bytes).expect("a well-formed ipfs-ns contenthash");
        assert_eq!(
            decoded,
            DecodedContenthash::Ipfs {
                uri: format!("ipfs://{cid_str}"),
                cid: cid_str.clone(),
            }
        );
        assert!(decoded.is_supported());
        assert_eq!(decoded.reason(), None, "the supported case has no refusal");

        // Prove the decoded CID string round-trips through the EXACT parser the
        // verified path uses, so there is no version skew.
        let parsed = crate::ipfs::parse_ipfs_uri(&format!("ipfs://{cid_str}"))
            .expect("the decoded ipfs uri parses on the verified path");
        assert_eq!(parsed.cid, cid_str);
        assert!(Cid::try_from(parsed.cid.as_str()).is_ok());
    }

    /// A real libp2p-key IPNS name (a CIDv1, `libp2p-key` codec 0x72, over a
    /// sha2-256 multihash of some "public key" bytes) as both its canonical
    /// base36 string AND the raw CID bytes an `ipns-ns` contenthash carries —
    /// derived with the SAME `cid`/`multihash` crates the resolver uses, so the
    /// round-trip is honest. (The bytes are not a real key, but the decoder only
    /// canonicalises the CID; signature verification against the key is the
    /// resolver's job, exercised in the `ipns` module.)
    fn ipns_name_fixture() -> (String, Vec<u8>) {
        const LIBP2P_KEY_CODEC: u64 = 0x72;
        // Reuse the sha2-256 multihash the fetcher helper derives (a real
        // multihash, via the SAME `cid`/`multihash` crates), rewrapped under the
        // libp2p-key codec to make a real IPNS name CID.
        let sha256_cid = Cid::try_from(
            cid_v1_raw_sha256(b"an ipns public key")
                .expect("derive a sha2-256 cid")
                .as_str(),
        )
        .expect("the derived cid parses");
        let name_cid = Cid::new_v1(LIBP2P_KEY_CODEC, *sha256_cid.hash());
        let name_str = name_cid
            .to_string_of_base(cid::multibase::Base::Base36Lower)
            .expect("base36 name string");
        (name_str, name_cid.to_bytes())
    }

    #[test]
    fn ipns_ns_decodes_to_a_libp2p_key_name_that_routes_into_resolution() {
        // Acceptance: `ipns-ns` (0xe5) is no longer a hard refusal — it decodes to
        // a distinct `Ipns` variant carrying the canonical libp2p-key IPNS name
        // (a base36 `k…` CIDv1), which the front door routes into IPNS resolution.
        // It is NOT ipfs (not directly loadable) and NOT a named refusal.
        let (name_str, name_bytes) = ipns_name_fixture();
        let bytes = contenthash(0xe5, &name_bytes);
        let decoded = decode_contenthash(&bytes).expect("well-formed ipns-ns contenthash");
        assert_eq!(
            decoded,
            DecodedContenthash::Ipns {
                name: name_str.clone(),
            }
        );
        // It is not the directly-loadable ipfs case, and it has no refusal reason
        // (it is now handled, via resolution, not refused).
        assert!(!decoded.is_supported());
        assert!(decoded.is_ipns());
        assert_eq!(decoded.reason(), None);
        // The name is the canonical base36 libp2p-key form gateways accept at
        // `GET /ipns/{name}`.
        assert!(
            name_str.starts_with('k'),
            "a base36 libp2p-key name: {name_str}"
        );
    }

    #[test]
    fn an_ipns_ns_protocode_with_invalid_name_bytes_is_a_distinct_invalid_name_error() {
        // Fail-closed: an `ipns-ns` whose payload is not a valid libp2p-key CID is
        // its OWN distinct error (not Malformed, not a guess, not a panic).
        let bytes = contenthash(0xe5, &[0xff, 0xff, 0xff]);
        let err = decode_contenthash(&bytes).expect_err("bad ipns name bytes");
        assert!(
            matches!(err, ContenthashError::InvalidIpnsName(_)),
            "got: {err:?}"
        );
    }

    #[test]
    fn an_ipns_ns_carrying_a_non_libp2p_key_cid_is_refused() {
        // Fail-closed: an `ipns-ns` payload that parses as a CID but is NOT a
        // libp2p-key CID (e.g. a raw/ipfs CID) is refused as an invalid IPNS name,
        // never resolved as if it named a key.
        let raw_cid = cid_v1_raw_sha256(b"not a key").expect("derive a raw cid");
        let raw_bytes = Cid::try_from(raw_cid.as_str())
            .expect("cid parses")
            .to_bytes();
        let bytes = contenthash(0xe5, &raw_bytes);
        let err = decode_contenthash(&bytes).expect_err("a non-libp2p-key ipns name is refused");
        assert!(
            matches!(err, ContenthashError::InvalidIpnsName(_)),
            "got: {err:?}"
        );
    }

    #[test]
    fn swarm_ns_points_to_swarm_not_supported() {
        // Acceptance: `swarm-ns` (0xe4) is its own named refusal.
        let bytes = contenthash(0xe4, b"some swarm address bytes");
        let decoded = decode_contenthash(&bytes).expect("well-formed swarm-ns contenthash");
        assert_eq!(decoded, DecodedContenthash::Unsupported(ProtoCode::Swarm));
        assert_eq!(
            decoded.reason().as_deref(),
            Some("points to Swarm, not supported")
        );
    }

    #[test]
    fn arweave_ns_points_to_arweave_not_supported() {
        // Acceptance: Arweave (0xb29910) is named specifically.
        let bytes = contenthash(0xb29910, b"some arweave tx id bytes");
        let decoded = decode_contenthash(&bytes).expect("well-formed arweave-ns contenthash");
        assert_eq!(decoded, DecodedContenthash::Unsupported(ProtoCode::Arweave));
        assert_eq!(
            decoded.reason().as_deref(),
            Some("points to Arweave, not supported")
        );
    }

    #[test]
    fn onion_and_onion3_and_skynet_and_zeronet_and_dnslink_are_each_named() {
        // The other named protoCodes in the ENSIP-7 family each get their own
        // named refusal (a multi-byte varint code exercises the varint reader too).
        for (code, name) in [
            (0x01bcu64, "Tor onion"),
            (0x01bd, "Tor onion v3"),
            (0xb19910, "Skynet"),
            (0xe6, "ZeroNet"),
            (0xe8, "DNSLink"),
        ] {
            let bytes = contenthash(code, b"payload");
            let decoded =
                decode_contenthash(&bytes).unwrap_or_else(|e| panic!("well-formed {name}: {e}"));
            assert_eq!(
                decoded.reason().as_deref(),
                Some(format!("points to {name}, not supported").as_str()),
                "protoCode {code:#x} should be named {name}"
            );
        }
    }

    #[test]
    fn an_unknown_protocode_is_bucketed_and_reported_by_its_hex_value() {
        // Acceptance: an unknown protoCode is its own distinct variant, reported by
        // its raw hex — NEVER defaulted to ipfs://.
        // 0x99 is not a namespace protoCode we name.
        let bytes = contenthash(0x99, b"whatever");
        let decoded = decode_contenthash(&bytes).expect("well-formed but unknown protoCode");
        assert_eq!(
            decoded,
            DecodedContenthash::Unsupported(ProtoCode::Unknown { code: 0x99 })
        );
        assert_eq!(
            decoded.reason().as_deref(),
            Some("unsupported/unknown contenthash protocol (0x99)")
        );
        assert!(!decoded.is_supported(), "unknown is never treated as ipfs");
    }

    #[test]
    fn empty_bytes_are_a_distinct_no_contenthash_error() {
        // Acceptance: `NoContenthash` (empty) is a distinct variant with a distinct
        // message.
        let err = decode_contenthash(&[]).expect_err("empty is no-contenthash");
        assert_eq!(err, ContenthashError::NoContenthash);
        assert_eq!(err.to_string(), "this name has no contenthash set");
    }

    #[test]
    fn a_truncated_protocode_varint_is_malformed_not_a_panic() {
        // Acceptance: undecodable bytes are `Malformed`, distinct from the others,
        // and never panic. A lone continuation byte (0x80) is a varint that never
        // terminates.
        let err = decode_contenthash(&[0x80]).expect_err("a truncated varint is malformed");
        assert!(
            matches!(err, ContenthashError::Malformed(_)),
            "got: {err:?}"
        );
        assert!(err.to_string().starts_with("malformed contenthash:"));
    }

    #[test]
    fn an_ipfs_ns_protocode_with_invalid_cid_bytes_is_a_distinct_invalid_cid_error() {
        // The one SUPPORTED protocol carrying broken CID bytes is its OWN distinct
        // error (not Malformed, not a guess, not a panic): an IPFS contenthash
        // whose CID does not parse.
        let bytes = contenthash(0xe3, &[0xff, 0xff, 0xff]);
        let err = decode_contenthash(&bytes).expect_err("bad cid bytes after ipfs-ns");
        assert!(
            matches!(err, ContenthashError::InvalidCid(_)),
            "an ipfs-ns with unparseable cid is InvalidCid, got: {err:?}"
        );
    }

    #[test]
    fn the_decoder_never_defaults_a_non_ipfs_protocode_to_ipfs() {
        // The hard requirement stated three ways: swarm, arweave, and an unknown
        // code must all be Unsupported (never Ipfs), so nothing but a real
        // `ipfs-ns` ever yields a directly-loadable `ipfs://` reference. (ipns-ns
        // is now handled via RESOLUTION, its own `Ipns` variant — covered by
        // `ipns_ns_decodes_to_a_libp2p_key_name_that_routes_into_resolution` — and
        // is likewise never the ipfs case.)
        for code in [0xe4u64, 0xb29910, 0x99] {
            let decoded = decode_contenthash(&contenthash(code, b"payload"))
                .expect("well-formed non-ipfs contenthash");
            assert!(
                matches!(decoded, DecodedContenthash::Unsupported(_)),
                "protoCode {code:#x} must not be dispatched as ipfs, got: {decoded:?}"
            );
        }
        // ipns-ns decodes to the `Ipns` variant (routed into resolution), NEVER
        // the ipfs case.
        let (_name, name_bytes) = ipns_name_fixture();
        let ipns = decode_contenthash(&contenthash(0xe5, &name_bytes))
            .expect("a well-formed ipns-ns name");
        assert!(
            matches!(ipns, DecodedContenthash::Ipns { .. }),
            "ipns-ns must decode to Ipns, never ipfs, got: {ipns:?}"
        );
        assert!(
            !ipns.is_supported(),
            "ipns is not the directly-loadable ipfs case"
        );
    }

    #[test]
    fn a_cidv0_ipfs_ns_contenthash_also_decodes_to_its_canonical_string() {
        // A real-world `ipfs-ns` contenthash often carries a CIDv0 (the `Qm…`
        // base58btc form). It must decode to the canonical CIDv0 string the
        // verified path accepts. We derive a real sha2-256 multihash via the SAME
        // helper the verified path uses (a CIDv1 raw/sha2-256), then re-wrap that
        // exact multihash as a CIDv0 (dag-pb) — reusing only the `Cid` type the
        // `fetcher` crate re-exports, so no new dependency surface and no
        // hand-built digest.
        use fetcher::Cid;
        let v1 = Cid::try_from(
            cid_v1_raw_sha256(b"a dag-pb root")
                .expect("derive a v1 cid")
                .as_str(),
        )
        .expect("the derived v1 cid parses");
        // A CIDv0 shares the sha2-256 multihash but fixes the dag-pb codec and the
        // base58btc `Qm…` string form.
        let cid0 = Cid::new_v0(*v1.hash()).expect("a v0 cid from the same sha2-256 multihash");
        let cid0_str = cid0.to_string();
        assert!(cid0_str.starts_with("Qm"), "a v0 cid is base58btc Qm…");

        let bytes = contenthash(0xe3, &cid0.to_bytes());
        let decoded = decode_contenthash(&bytes).expect("a cidv0 ipfs-ns contenthash");
        assert_eq!(
            decoded,
            DecodedContenthash::Ipfs {
                uri: format!("ipfs://{cid0_str}"),
                cid: cid0_str,
            }
        );
    }
}
