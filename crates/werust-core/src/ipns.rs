//! IPNS name resolution: turn a MUTABLE `ipns-ns` name into the immutable
//! `ipfs://<cid>` it CURRENTLY points at, via a client-VERIFIED IPNS record \u2014 no
//! IPFS node, and no hand-rolled signature crypto.
//!
//! This is the resolution CORE the task `ipns-name-resolution-and-render` (spec
//! `ens-to-ipfs-resolution-phase1-rpc-skeleton`) calls for: it closes the
//! `ipns-ns` gap the ENSIP-7 decoder once refused. An IPNS name is a libp2p-key
//! (a public-key hash); its current value is a SIGNED IPNS record (multicodec
//! `0x0300`) mapping the name to a `/ipfs/<cid>`, with a sequence number +
//! validity window. This module fetches that record from an UNTRUSTED trustless
//! endpoint and VERIFIES it client-side against the key \u2014 the SAME
//! untrusted-source-plus-client-verify discipline the CAR blocks use
//! (`docs/adr/0004`) \u2014 then hands the resolved CID to the existing verified
//! `ipfs://` render path.
//!
//! # The record is UNTRUSTED; verification is the whole point (`docs/adr/0007`)
//!
//! The endpoint that serves the record is a third party werust does not trust by
//! default. [`resolve_ipns_name`] treats the fetched bytes as untrusted and:
//!
//! 1. derives the [`PeerId`] from the IPNS `name` (a libp2p-key CID whose
//!    multihash IS the key/PeerId),
//! 2. fetches the raw record over the [`IpnsRecordSource`] seam
//!    (`GET /ipns/{name}?format=ipns-record`, `application/vnd.ipfs.ipns-record`),
//! 3. decodes it and FULLY VALIDATES it against the PeerId with the vetted
//!    [`rust_ipns`] crate ([`Record::verify`]): the name binding (the record's
//!    key matches the name), the V2 signature, AND that the EOL validity has not
//!    elapsed \u2014 the signature crypto is `libp2p-identity`, NEVER in-house
//!    (`docs/adr/0001`),
//! 4. extracts the record's `/ipfs/<cid>` value and canonicalises the CID into
//!    the `ipfs://<cid>` string the verified render path consumes.
//!
//! # Fail-closed is a hard requirement
//!
//! Every step that can fail is a DISTINCT [`IpnsError`] variant, never a partial
//! or guessed result: an unparseable name ([`InvalidName`](IpnsError::InvalidName)),
//! a record-fetch/transport failure ([`Source`](IpnsError::Source)), a record
//! that does not decode ([`MalformedRecord`](IpnsError::MalformedRecord)), a
//! record whose signature / name-binding / validity does not check out
//! ([`Unverifiable`](IpnsError::Unverifiable)), and a record whose target is not
//! a supported `/ipfs/<cid>` ([`UnsupportedTarget`](IpnsError::UnsupportedTarget) /
//! [`InvalidTarget`](IpnsError::InvalidTarget)) each surface distinctly. NOTHING
//! is resolved on a bad record or a bad target; the front door renders nothing on
//! a failure.
//!
//! # DNSLink is OUT of scope (a named follow-on)
//!
//! A `_dnslink` DNS-TXT name is a DIFFERENT trust story (a DNS lookup, not a
//! signed libp2p-key record). This module resolves libp2p-key IPNS names only;
//! DNSLink is the deferred follow-on the task names.

use fetcher::{Cid, Fetcher};
use libp2p_identity::PeerId;
use quick_protobuf::{BytesReader, MessageWrite, Writer};
use rust_ipns::Record;

/// The `/ipfs/` path prefix an IPNS record's value carries when it points at
/// immutable IPFS content.
///
/// The IPNS `Value` is a content path (`/ipfs/<cid>` for immutable content, or
/// `/ipns/<name>` for a chained mutable name). werust resolves the `/ipfs/<cid>`
/// case here; a `/ipns/` chain (a name pointing at another name) is a named
/// follow-on refused as [`IpnsError::UnsupportedTarget`], never blindly followed.
const IPFS_PATH_PREFIX: &str = "/ipfs/";

/// The `/ipns/` path prefix a chained IPNS record's value carries (a name that
/// points at ANOTHER mutable name). Detected so it can be refused as an explicit,
/// legible unsupported-target rather than mis-parsed.
const IPNS_PATH_PREFIX: &str = "/ipns/";

/// The `ipfs` scheme the resolved CID is rendered under (kept in sync with
/// [`crate::ipfs::IPFS_SCHEME`], whose verified path consumes the produced URI).
const IPFS_SCHEME: &str = "ipfs";

/// The trustless-gateway query the record fetch uses: the Trustless Gateway spec
/// exposes a verifiable IPNS record at `GET /ipns/{name}?format=ipns-record`
/// (`application/vnd.ipfs.ipns-record`).
const IPNS_RECORD_FORMAT_QUERY: &str = "?format=ipns-record";

/// The default trustless gateway IPNS records are fetched from.
///
/// The SAME public trustless gateway the CAR content path defaults to
/// ([`fetcher::DEFAULT_TRUSTLESS_GATEWAY`]) \u2014 an UNTRUSTED origin: it serves the
/// signed record over plain HTTP, and the client verifies the record's signature
/// against the key before any byte is used, so a hostile or buggy gateway cannot
/// misdirect the name. Overridable via
/// [`GatewayIpnsRecordSource::with_gateway`] (the existing `DEFAULT_*` +
/// `with_*()` pattern, no config subsystem) so it tracks the user's chosen
/// retrieval backend the same way the content path does.
pub use fetcher::DEFAULT_TRUSTLESS_GATEWAY as DEFAULT_IPNS_GATEWAY;

/// The resolved target of an IPNS name: the immutable `ipfs://<cid>` its current
/// (verified) record points at.
///
/// Returned by [`resolve_ipns_name`] ONLY after the record's signature, name
/// binding, and validity all checked out against the key. The `uri` feeds the
/// existing verified `ipfs://` render path; `cid` is the identifier without the
/// scheme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedIpns {
    /// The `ipfs://<cid>` URI the IPNS name currently resolves to, ready to feed
    /// the verified `ipfs://` path.
    pub uri: String,
    /// The canonical CID string the name resolves to (the `<cid>` in the URI).
    pub cid: String,
}

/// A DISTINCT fail-closed failure of resolving an IPNS name to its current CID.
///
/// Every step that can fail has its OWN variant so the front door / chrome turns
/// each into a legible, specific load failure and never renders a guessed or
/// unverified result. The one non-error outcome of [`resolve_ipns_name`] is a
/// [`ResolvedIpns`] whose record VERIFIED against the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpnsError {
    /// The IPNS name could not be parsed into a libp2p-key / [`PeerId`]: it is not
    /// a valid libp2p-key CID, or its multihash is not a usable key hash. Rejected
    /// before any fetch \u2014 a name we cannot turn into a key has nothing to verify
    /// a record against.
    InvalidName {
        /// The name that failed to parse.
        name: String,
        /// The reason it could not be turned into a key.
        detail: String,
    },
    /// Fetching the record over the [`IpnsRecordSource`] seam failed (a transport
    /// error, a non-2xx gateway status, an empty body). The resolution simply did
    /// not complete; distinct from a record that came back but did not verify.
    Source(String),
    /// The fetched bytes are not a decodable IPNS record (malformed
    /// protobuf/dag-cbor, wrong shape). Distinct from a record that decoded but
    /// failed VERIFICATION.
    MalformedRecord(String),
    /// The record decoded but did NOT verify against the key: a bad V2 signature,
    /// a key that does not match the requested name, or an EOL validity that has
    /// elapsed (an expired record). The load-bearing trust failure \u2014 the record
    /// is rejected, never used to resolve the name.
    Unverifiable {
        /// The distinct verification reason (bad signature / name mismatch /
        /// expired), from the vetted verifier.
        detail: String,
    },
    /// The verified record's value is a well-formed path but NOT a supported
    /// target: a `/ipns/<name>` chain (a name pointing at another mutable name),
    /// which werust does not follow in this task (a named follow-on). Refused
    /// legibly rather than blindly chained.
    UnsupportedTarget {
        /// The record's raw value (e.g. `/ipns/<name>`).
        value: String,
    },
    /// The verified record's value is not a usable `/ipfs/<cid>` target: it does
    /// not start with `/ipfs/`, or the CID after it does not parse. The record
    /// verified, but it does not name loadable content, so the load fails closed.
    InvalidTarget {
        /// The record's raw value.
        value: String,
        /// Why it is not a usable `/ipfs/<cid>`.
        detail: String,
    },
}

impl std::fmt::Display for IpnsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpnsError::InvalidName { name, detail } => {
                write!(f, "'{name}' is not a valid IPNS name: {detail}")
            }
            IpnsError::Source(detail) => write!(f, "IPNS record fetch failed: {detail}"),
            IpnsError::MalformedRecord(detail) => {
                write!(f, "IPNS record is malformed: {detail}")
            }
            IpnsError::Unverifiable { detail } => {
                write!(f, "IPNS record did not verify: {detail}")
            }
            IpnsError::UnsupportedTarget { value } => {
                write!(
                    f,
                    "IPNS name points at another name ({value}), which is not supported"
                )
            }
            IpnsError::InvalidTarget { value, detail } => {
                write!(f, "IPNS record has an invalid target ({value}): {detail}")
            }
        }
    }
}

impl std::error::Error for IpnsError {}

/// Where a raw (untrusted) IPNS record's bytes come from, before client-side
/// verification.
///
/// Modelled like the [`ContentRetriever`](fetcher::ContentRetriever) /
/// [`Fetcher`](fetcher::Fetcher) seams: the trait is the abstraction ("given an
/// IPNS name, produce the candidate record bytes"); the transport is a swappable
/// BACKEND. The bytes are NOT trusted \u2014 [`resolve_ipns_name`] verifies them
/// against the key before using them \u2014 so a hostile source cannot misdirect a
/// name. One default backend ships now
/// ([`GatewayIpnsRecordSource`], a trustless-gateway fetch); a delegated-routing
/// or embedded-p2p source is a later backend swap behind this same seam.
pub trait IpnsRecordSource {
    /// Produce the candidate (UNVERIFIED) record bytes for `name`.
    ///
    /// `name` is the canonical libp2p-key IPNS name (a base36 `k\u2026` CIDv1). A
    /// miss / transport failure / empty body is surfaced as an [`IpnsError::Source`]
    /// so the caller fails the load closed; verification is NOT this seam's job.
    fn fetch_record(&self, name: &str) -> Result<Vec<u8>, IpnsError>;
}

/// The default [`IpnsRecordSource`]: fetch the signed IPNS record from a
/// trustless gateway over the bound HTTP [`Fetcher`](fetcher::Fetcher).
///
/// It GETs `<gateway>/ipns/{name}?format=ipns-record` and returns the raw record
/// bytes UNVERIFIED (verification happens above it, in [`resolve_ipns_name`]).
/// NO IPFS node, NO async runtime \u2014 a single GET over the sync seam, mirroring
/// the CAR backend. Generic over the [`Fetcher`](fetcher::Fetcher) so tests drive
/// it against a controlled loopback endpoint, off the live network.
pub struct GatewayIpnsRecordSource<F: Fetcher> {
    fetcher: F,
    gateway: String,
}

impl<F: Fetcher> GatewayIpnsRecordSource<F> {
    /// A record source over the given HTTP [`Fetcher`](fetcher::Fetcher), using
    /// the [`DEFAULT_IPNS_GATEWAY`].
    pub fn new(fetcher: F) -> Self {
        Self::with_gateway(fetcher, DEFAULT_IPNS_GATEWAY)
    }

    /// A record source pointed at a specific trustless-gateway base URL (a local
    /// node, another gateway, or a test endpoint). A trailing `/` is tolerated.
    pub fn with_gateway(fetcher: F, gateway: &str) -> Self {
        Self {
            fetcher,
            gateway: gateway.trim_end_matches('/').to_string(),
        }
    }
}

impl<F: Fetcher> IpnsRecordSource for GatewayIpnsRecordSource<F> {
    fn fetch_record(&self, name: &str) -> Result<Vec<u8>, IpnsError> {
        let url = format!(
            "{gateway}/ipns/{name}{query}",
            gateway = self.gateway,
            query = IPNS_RECORD_FORMAT_QUERY,
        );
        let response = self
            .fetcher
            .fetch(&url)
            .map_err(|e| IpnsError::Source(e.to_string()))?;
        if !response.is_success() {
            return Err(IpnsError::Source(format!(
                "gateway returned status {status} for {name}",
                status = response.status,
            )));
        }
        if response.body.is_empty() {
            return Err(IpnsError::Source(format!(
                "gateway returned an empty record for {name}"
            )));
        }
        Ok(response.body)
    }
}

/// Derive the [`PeerId`] an IPNS `name` (a libp2p-key CID) names.
///
/// An IPNS name is a CIDv1 whose multihash IS the key (the PeerId): the multihash
/// bytes are exactly what [`PeerId::from_bytes`] validates. Parsing goes through
/// the vetted `cid` + `libp2p-identity` crates (no hand-rolled key layout); a
/// name that is not a CID, or whose multihash is not a usable PeerId, is a
/// distinct [`IpnsError::InvalidName`] rejected before any fetch.
fn peer_id_for_name(name: &str) -> Result<PeerId, IpnsError> {
    let cid = Cid::try_from(name).map_err(|e| IpnsError::InvalidName {
        name: name.to_string(),
        detail: e.to_string(),
    })?;
    // The IPNS name's multihash IS the PeerId. Hand the multihash bytes to the
    // vetted `libp2p-identity` parser (bytes, not a typed multihash, so the two
    // crates' multihash lineages need not match at the boundary).
    PeerId::from_bytes(&cid.hash().to_bytes()).map_err(|e| IpnsError::InvalidName {
        name: name.to_string(),
        detail: e.to_string(),
    })
}

/// Turn a verified record's `/ipfs/<cid>` value into the `ipfs://<cid>` the
/// verified render path consumes.
///
/// The IPNS value is a content PATH. Only `/ipfs/<cid>` is loadable here: a
/// `/ipns/<name>` chain is refused as [`IpnsError::UnsupportedTarget`] (a named
/// follow-on, never blindly chained), and anything else \u2014 or a `/ipfs/` whose
/// CID does not parse \u2014 is [`IpnsError::InvalidTarget`]. The CID is parsed +
/// re-rendered with the vetted `cid` crate to its canonical string, exactly the
/// form [`crate::ipfs::parse_ipfs_uri`] / the verified fetch accept.
fn ipfs_uri_for_value(value: &[u8]) -> Result<ResolvedIpns, IpnsError> {
    // The value is a UTF-8 content path. A non-UTF-8 value cannot be a
    // `/ipfs/<cid>` path, so it is an invalid target (never guessed).
    let value_str = std::str::from_utf8(value).map_err(|e| IpnsError::InvalidTarget {
        value: format!("{value:?}"),
        detail: format!("value is not utf-8: {e}"),
    })?;

    if let Some(rest) = value_str.strip_prefix(IPFS_PATH_PREFIX) {
        // `/ipfs/<cid>[/subpath]`: take the CID authority (the first segment) and
        // canonicalise it. Any sub-path within the record value is not part of the
        // task (the record points at a CID); a follow-on may thread it.
        let cid_str = rest.split('/').next().unwrap_or("");
        let cid = Cid::try_from(cid_str).map_err(|e| IpnsError::InvalidTarget {
            value: value_str.to_string(),
            detail: format!("target cid does not parse: {e}"),
        })?;
        let cid = cid.to_string();
        Ok(ResolvedIpns {
            uri: format!("{IPFS_SCHEME}://{cid}"),
            cid,
        })
    } else if value_str.starts_with(IPNS_PATH_PREFIX) {
        // A name that points at ANOTHER name. Refused legibly (a named follow-on),
        // never blindly chained.
        Err(IpnsError::UnsupportedTarget {
            value: value_str.to_string(),
        })
    } else {
        Err(IpnsError::InvalidTarget {
            value: value_str.to_string(),
            detail: "value is not an /ipfs/<cid> path".to_string(),
        })
    }
}

/// The dag-cbor `Data` map an IPNS record's `data` field (protobuf field 9)
/// carries: the AUTHORITATIVE, V2-signature-covered values. The field NAMES are
/// the reserved CBOR keys from the IPNS spec (`Value`/`ValidityType`/`Validity`/
/// `Sequence`/`TTL`); `#[serde(deny_unknown_fields)]` is deliberately NOT set so
/// a record carrying extra non-standard metadata keys still decodes (they are
/// covered by the same V2 signature and are irrelevant to the mirror fields).
///
/// This mirrors `rust_ipns`'s own private `Data` struct: werust only reads the
/// five reserved fields it copies into the deprecated protobuf mirror, never
/// re-implements any crypto.
#[derive(serde::Deserialize)]
struct IpnsCborData {
    #[serde(rename = "Value", with = "serde_bytes")]
    value: Vec<u8>,
    #[serde(rename = "ValidityType")]
    validity_type: i32,
    #[serde(rename = "Validity", with = "serde_bytes")]
    validity: Vec<u8>,
    #[serde(rename = "Sequence")]
    sequence: u64,
    #[serde(rename = "TTL")]
    ttl: u64,
}

/// Normalise a raw IPNS record's bytes so `rust-ipns 0.9.0`'s `Record::verify`
/// accepts a modern **V2-only** record, WITHOUT touching any cryptography.
///
/// # Why this shim exists (the real `ronan.eth` failure)
///
/// A V2-only IPNS record (IPIP-428, the shape modern gateways like dweb.link /
/// trustless-gateway.link serve) OMITS the deprecated top-level protobuf fields
/// (`value`/`signatureV1`/`validityType`/`validity`/`sequence`/`ttl`) and carries
/// ONLY the V2 signature (field 8) and the dag-cbor `data` map (field 9). But
/// `rust-ipns 0.9.0`'s `Record::verify` -> `verify_signature` calls `data()`,
/// which REQUIRES those absent protobuf fields to be present and equal to the
/// CBOR data, and so rejects a perfectly valid record with a scary
/// `DataMismatch` ("dag-cbor data does not match the protobuf fields"). This is
/// exactly why `ronan.eth` failed to load. See the finding
/// `work/notes/findings/ronan-eth-ipns-v2-only-record-verify-bug-2026-07-23.md`
/// and `docs/spikes/prominent-load-failure-and-ipns-resolution-diagnosis/`.
///
/// # What it does (and does NOT do)
///
/// If the protobuf `value` field is EMPTY but a `data` (CBOR) field is present
/// (the V2-only shape), it decodes the signed dag-cbor `Data` map and re-encodes
/// the `IpnsEntry` protobuf with fields 1/3/4/5/6 populated FROM that CBOR data,
/// preserving `pubKey` (7) / `signatureV2` (8) / `data` (9) VERBATIM. The result
/// is the V1+V2-shaped record `data()` accepts, and the crate then runs the REAL
/// V2 signature + name-binding + validity check unchanged.
///
/// This copies only values the V2 signature ALREADY covers (the CBOR `data` is
/// preserved byte-for-byte and the signature is verified over it), so the shim
/// cannot smuggle an unsigned value past verification. A record that already
/// carries the protobuf `value` field (a V1+V2 or legacy record) is returned
/// untouched, so nothing changes for records the crate already handled. On any
/// parse hiccup it returns the original bytes unchanged and lets the crate's own
/// decode/verify produce the distinct error, never a guess.
///
/// ALL cryptography stays in `rust-ipns` / `libp2p-identity` (`docs/adr/0001`):
/// this is a pure wire-format reshape, no signature or hashing logic.
fn normalize_ipns_record_bytes(bytes: &[u8]) -> Vec<u8> {
    // Read the `value` (1) and `data` (9) fields to detect the V2-only shape.
    // Any parse hiccup falls back to the original bytes (the crate reports the
    // real decode error), never a guess.
    let (value_present, data_field) = match scan_value_and_data(bytes) {
        Some(pair) => pair,
        None => return bytes.to_vec(),
    };
    // Already carries the protobuf `value` (a V1+V2 / legacy record), or has no
    // CBOR `data` to mirror from: nothing to normalise.
    if value_present || data_field.is_empty() {
        return bytes.to_vec();
    }
    // Decode the signed dag-cbor `Data` the mirror fields are copied from.
    let data: IpnsCborData = match serde_ipld_dagcbor::from_slice(&data_field) {
        Ok(d) => d,
        // Not decodable as the reserved-field map: leave it for the crate's
        // decode/verify to reject distinctly.
        Err(_) => return bytes.to_vec(),
    };
    // Re-encode the `IpnsEntry` with the deprecated protobuf mirror fields
    // populated from the CBOR data, preserving pubKey/signatureV2/data verbatim.
    let entry = IpnsEntryNormalized {
        value: data.value,
        validity_type: data.validity_type,
        validity: data.validity,
        sequence: data.sequence,
        ttl: data.ttl,
        pub_key: read_field_bytes(bytes, 7).unwrap_or_default(),
        signature_v2: read_field_bytes(bytes, 8).unwrap_or_default(),
        data: data_field,
    };
    let mut out = Vec::with_capacity(bytes.len() + 96);
    {
        let mut writer = Writer::new(&mut out);
        // On the vanishingly unlikely write failure, fall back to the original.
        if entry.write_message(&mut writer).is_err() {
            return bytes.to_vec();
        }
    }
    out
}

/// Whether the record carries a non-empty protobuf `value` (field 1), and the raw
/// `data` (field 9) bytes. Returns [`None`] only if the protobuf does not parse at
/// all (the caller then leaves the bytes for the crate to reject).
fn scan_value_and_data(bytes: &[u8]) -> Option<(bool, Vec<u8>)> {
    let mut reader = BytesReader::from_bytes(bytes);
    let mut value_present = false;
    let mut data_field = Vec::new();
    while !reader.is_eof() {
        let tag = reader.next_tag(bytes).ok()?;
        let field = tag >> 3;
        let wire = tag & 0x7;
        if wire == 2 {
            let chunk = reader.read_bytes(bytes).ok()?;
            match field {
                1 => value_present = !chunk.is_empty(),
                9 => data_field = chunk.to_vec(),
                _ => {}
            }
        } else {
            reader.read_unknown(bytes, tag).ok()?;
        }
    }
    Some((value_present, data_field))
}

/// Read a single length-delimited protobuf field's bytes (`pubKey` 7 /
/// `signatureV2` 8), or [`None`] if absent / unparseable — the caller defaults to
/// empty, matching an omitted optional field.
fn read_field_bytes(bytes: &[u8], want: u32) -> Option<Vec<u8>> {
    let mut reader = BytesReader::from_bytes(bytes);
    while !reader.is_eof() {
        let tag = reader.next_tag(bytes).ok()?;
        let field = tag >> 3;
        let wire = tag & 0x7;
        if wire == 2 {
            let chunk = reader.read_bytes(bytes).ok()?;
            if field == want {
                return Some(chunk.to_vec());
            }
        } else {
            reader.read_unknown(bytes, tag).ok()?;
        }
    }
    None
}

/// A minimal writer-only mirror of the IPNS `IpnsEntry` protobuf, carrying the
/// fields werust re-emits when normalising a V2-only record: the deprecated
/// mirror fields (1/3/4/5/6) populated from the CBOR data, plus `pubKey` (7) /
/// `signatureV2` (8) / `data` (9) preserved verbatim.
///
/// `signatureV1` (field 2) is deliberately NOT emitted: a V2-only record has no
/// V1 signature, and `rust-ipns`'s `verify` only checks V2. Field numbers match
/// `rust-ipns`'s `ipns_pb.proto`.
struct IpnsEntryNormalized {
    value: Vec<u8>,
    validity_type: i32,
    validity: Vec<u8>,
    sequence: u64,
    ttl: u64,
    pub_key: Vec<u8>,
    signature_v2: Vec<u8>,
    data: Vec<u8>,
}

impl MessageWrite for IpnsEntryNormalized {
    fn write_message<W: quick_protobuf::writer::WriterBackend>(
        &self,
        w: &mut Writer<W>,
    ) -> quick_protobuf::Result<()> {
        w.write_with_tag(10, |w| w.write_bytes(&self.value))?; // 1: value
        w.write_with_tag(24, |w| w.write_int32(self.validity_type))?; // 3: validityType
        w.write_with_tag(34, |w| w.write_bytes(&self.validity))?; // 4: validity
        w.write_with_tag(40, |w| w.write_uint64(self.sequence))?; // 5: sequence
        w.write_with_tag(48, |w| w.write_uint64(self.ttl))?; // 6: ttl
        if !self.pub_key.is_empty() {
            w.write_with_tag(58, |w| w.write_bytes(&self.pub_key))?; // 7: pubKey
        }
        w.write_with_tag(66, |w| w.write_bytes(&self.signature_v2))?; // 8: signatureV2
        w.write_with_tag(74, |w| w.write_bytes(&self.data))?; // 9: data
        Ok(())
    }
}

/// Resolve an IPNS `name` to the immutable `ipfs://<cid>` its CURRENT
/// (client-verified) record points at, through the [`IpnsRecordSource`] seam, or
/// a DISTINCT fail-closed [`IpnsError`].
///
/// The composed path (see the module docs): derive the [`PeerId`] from the name,
/// fetch the raw record over the seam, decode it, VERIFY it against the PeerId
/// (name binding + V2 signature + EOL validity, via the vetted `rust-ipns` /
/// `libp2p-identity` crates), then extract + canonicalise its `/ipfs/<cid>`
/// value.
///
/// On success returns a [`ResolvedIpns`] whose record VERIFIED \u2014 an
/// `ipfs://<cid>` ready to feed the existing verified `ipfs://` render path. The
/// resulting page is honestly a MUTABLE name (the key holder can publish a new
/// record), so the front door marks it [`renderer::TrustPosture::MutableName`],
/// NEVER immutable `ContentVerified`. Every failure step is its own variant
/// (fail-closed), never a guessed or partial result.
///
/// `source` is `&dyn` so the SAME resolution drives the default trustless-gateway
/// source today and a delegated-routing / embedded-p2p source later, unchanged.
pub fn resolve_ipns_name(
    source: &dyn IpnsRecordSource,
    name: &str,
) -> Result<ResolvedIpns, IpnsError> {
    // 1. The name -> the key/PeerId the record must verify against (fail-closed
    //    before any fetch).
    let peer_id = peer_id_for_name(name)?;

    // 2. Fetch the raw, UNTRUSTED record bytes over the seam.
    let record_bytes = source.fetch_record(name)?;

    // 3. Normalise a modern V2-only record (which OMITS the deprecated top-level
    //    protobuf fields) into the V1+V2 shape `rust-ipns 0.9.0`'s verifier
    //    requires, then decode. This is a pure wire reshape (no crypto): without
    //    it, a valid V2-only record (e.g. ronan.eth's) is wrongly rejected as
    //    `DataMismatch`. A record that does not decode is distinct from one that
    //    decodes but fails verification.
    let normalized = normalize_ipns_record_bytes(&record_bytes);
    let record =
        Record::decode(&normalized).map_err(|e| IpnsError::MalformedRecord(e.to_string()))?;

    // 4. FULLY VERIFY against the key: name binding + V2 signature + EOL validity.
    //    This is the trust boundary \u2014 an unverifiable record is REJECTED, never
    //    used to resolve the name (the signature crypto is `libp2p-identity`, not
    //    in-house).
    record
        .verify(peer_id)
        .map_err(|e| IpnsError::Unverifiable {
            detail: e.to_string(),
        })?;

    // 5. Extract + canonicalise the verified `/ipfs/<cid>` target.
    ipfs_uri_for_value(record.value())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDuration, Utc};
    use fetcher::{FetchError, Response};
    use libp2p_identity::Keypair;
    use std::cell::RefCell;
    use std::time::Duration;

    /// A minted ed25519 keypair + the canonical base36 IPNS name (the libp2p-key
    /// CID) it corresponds to. This is a REAL key: the record it signs verifies
    /// against this exact name, off the live network.
    struct KeyFixture {
        keypair: Keypair,
        name: String,
    }

    impl KeyFixture {
        fn new() -> Self {
            let keypair = Keypair::generate_ed25519();
            let peer_id = keypair.public().to_peer_id();
            // The IPNS name is the libp2p-key CIDv1 over the PeerId's multihash,
            // rendered base36 \u2014 exactly what the ENSIP-7 decoder produces and what
            // `peer_id_for_name` re-parses.
            const LIBP2P_KEY_CODEC: u64 = 0x72;
            let mh = cid::multihash::Multihash::from_bytes(&peer_id.to_bytes())
                .expect("peer id is a multihash");
            let name = Cid::new_v1(LIBP2P_KEY_CODEC, mh)
                .to_string_of_base(cid::multibase::Base::Base36Lower)
                .expect("base36 name");
            Self { keypair, name }
        }

        /// Sign a record pointing `name` at `value` (e.g. `/ipfs/<cid>`), valid
        /// for `valid_for` from now, at sequence `seq`, and return its encoded
        /// bytes \u2014 the exact wire form a trustless gateway would serve.
        fn sign(&self, value: &str, seq: u64, valid_for: ChronoDuration) -> Vec<u8> {
            let eol = Utc::now() + valid_for;
            let record = Record::new(
                &self.keypair,
                value.as_bytes(),
                eol,
                seq,
                Duration::from_secs(3600),
            )
            .expect("sign an ipns record");
            record.encode().expect("encode the signed record")
        }

        /// Sign a record and RE-EMIT it as a modern **V2-only** record: the exact
        /// wire form dweb.link / trustless-gateway.link serve for `ronan.eth`,
        /// carrying ONLY the V2 signature (protobuf field 8) and the dag-cbor
        /// `data` (field 9), with the deprecated top-level protobuf mirror fields
        /// (value/signatureV1/validityType/validity/sequence/ttl) OMITTED.
        ///
        /// This is exactly the shape `rust-ipns 0.9.0`'s `verify` wrongly rejects
        /// as `DataMismatch` unless `resolve_ipns_name` normalises it first, so it
        /// reproduces the real `ronan.eth` failure offline. Built by parsing the
        /// crate's own signed encoding and keeping only fields 8 + 9, so the V2
        /// signature over the dag-cbor data is genuine.
        fn sign_v2_only(&self, value: &str, seq: u64, valid_for: ChronoDuration) -> Vec<u8> {
            let full = self.sign(value, seq, valid_for);
            v2_only_record(&full)
        }
    }

    /// Re-emit a full (V1+V2) IPNS record's bytes as a **V2-only** record: keep
    /// ONLY protobuf field 8 (`signatureV2`) and field 9 (`data`), dropping every
    /// deprecated top-level mirror field. Mirrors what modern gateways serve.
    fn v2_only_record(full: &[u8]) -> Vec<u8> {
        use quick_protobuf::{BytesReader, Writer};
        let mut sig_v2 = Vec::new();
        let mut data = Vec::new();
        let mut reader = BytesReader::from_bytes(full);
        while !reader.is_eof() {
            let tag = reader.next_tag(full).expect("tag");
            let field = tag >> 3;
            let wire = tag & 0x7;
            if wire == 2 {
                let chunk = reader.read_bytes(full).expect("bytes").to_vec();
                match field {
                    8 => sig_v2 = chunk,
                    9 => data = chunk,
                    _ => {}
                }
            } else {
                reader.read_unknown(full, tag).expect("skip");
            }
        }
        assert!(
            !sig_v2.is_empty() && !data.is_empty(),
            "a V2 record has 8 + 9"
        );
        let mut out = Vec::new();
        {
            let mut w = Writer::new(&mut out);
            w.write_with_tag(66, |w| w.write_bytes(&sig_v2))
                .expect("write 8");
            w.write_with_tag(74, |w| w.write_bytes(&data))
                .expect("write 9");
        }
        out
    }

    /// A pinned, in-memory [`IpnsRecordSource`] double, isolated from the live
    /// network, returning pre-registered record bytes for a name, or a chosen
    /// source failure.
    #[derive(Default)]
    struct PinnedRecordSource {
        records: std::collections::HashMap<String, Vec<u8>>,
        fail: Option<IpnsError>,
    }

    impl PinnedRecordSource {
        fn put(&mut self, name: &str, record: Vec<u8>) {
            self.records.insert(name.to_string(), record);
        }

        fn failing(err: IpnsError) -> Self {
            Self {
                records: std::collections::HashMap::new(),
                fail: Some(err),
            }
        }
    }

    impl IpnsRecordSource for PinnedRecordSource {
        fn fetch_record(&self, name: &str) -> Result<Vec<u8>, IpnsError> {
            if let Some(err) = &self.fail {
                return Err(err.clone());
            }
            self.records
                .get(name)
                .cloned()
                .ok_or_else(|| IpnsError::Source(format!("no record pinned for {name}")))
        }
    }

    /// A real, verifiable CID string for an `/ipfs/<cid>` target, via the SAME
    /// helper the verified path uses.
    fn ipfs_cid(bytes: &[u8]) -> String {
        fetcher::cid_v1_raw_sha256(bytes).expect("derive a target cid")
    }

    #[test]
    fn a_libp2p_key_name_resolves_via_a_verified_record_to_its_current_cid() {
        // Acceptance (the DONE bar, offline): a libp2p-key IPNS name resolves to
        // its current CID via a VERIFIABLE record \u2014 the record fetched from an
        // untrusted source, its signature + name-binding + validity verified
        // client-side against the key \u2014 with NO IPFS node. The resolved
        // `ipfs://<cid>` is exactly what the verified render path consumes.
        let key = KeyFixture::new();
        let cid = ipfs_cid(b"the ipns site's current immutable content");
        let record = key.sign(&format!("/ipfs/{cid}"), 1, ChronoDuration::hours(24));

        let mut source = PinnedRecordSource::default();
        source.put(&key.name, record);

        let resolved = resolve_ipns_name(&source, &key.name).expect("the name resolves");
        assert_eq!(
            resolved,
            ResolvedIpns {
                uri: format!("ipfs://{cid}"),
                cid: cid.clone(),
            }
        );
        // The resolved uri round-trips through the EXACT parser the verified path
        // uses, so there is no skew.
        let parsed = crate::ipfs::parse_ipfs_uri(&resolved.uri)
            .expect("the resolved ipfs uri parses on the verified path");
        assert_eq!(parsed.cid, cid);
    }

    #[test]
    fn a_modern_v2_only_record_resolves_the_ronan_eth_failure_offline() {
        // The real `ronan.eth` failure, reproduced + fixed OFFLINE: modern
        // gateways (dweb.link / trustless-gateway.link) serve a V2-ONLY IPNS
        // record that OMITS the deprecated top-level protobuf fields (only the V2
        // signature + the dag-cbor `data` are on the wire). `rust-ipns 0.9.0`'s
        // `verify` wrongly rejects that valid record as `DataMismatch` unless
        // `resolve_ipns_name` first normalises it (a pure wire reshape, no
        // crypto). This proves a genuine V2-only record now RESOLVES to its
        // signed target rather than failing closed. See the finding
        // `ronan-eth-ipns-v2-only-record-verify-bug-2026-07-23`.
        let key = KeyFixture::new();
        let cid = ipfs_cid(b"ronan.eth's current immutable site, served as a V2-only record");
        let v2_only = key.sign_v2_only(&format!("/ipfs/{cid}"), 4, ChronoDuration::hours(24));

        // Sanity: the fixture really is V2-only (no protobuf `value` field 1), so
        // it exercises the normalization, not the already-handled path.
        assert!(
            scan_value_and_data(&v2_only)
                .is_some_and(|(value_present, data)| { !value_present && !data.is_empty() }),
            "the fixture must be a V2-only record (no protobuf value, has data)"
        );
        // And the crate REJECTS it un-normalized, proving the shim is load-bearing
        // (this is the exact `ronan.eth` failure).
        assert!(
            Record::decode(&v2_only)
                .expect("decodes")
                .verify(peer_id_for_name(&key.name).unwrap())
                .is_err(),
            "rust-ipns rejects the un-normalized V2-only record (the ronan.eth bug)"
        );

        let mut source = PinnedRecordSource::default();
        source.put(&key.name, v2_only);
        let resolved = resolve_ipns_name(&source, &key.name)
            .expect("a modern V2-only record now resolves (ronan.eth fixed)");
        assert_eq!(resolved.uri, format!("ipfs://{cid}"));
    }

    #[test]
    fn a_v2_only_record_signed_by_the_wrong_key_still_fails_closed() {
        // The normalization must NOT weaken verification: a V2-only record signed
        // by a DIFFERENT key than the requested name still fails closed (a
        // misdirecting source cannot repoint a name it does not hold the key for).
        // The shim only reshapes the wire form; the V2 signature check is
        // unchanged.
        let key = KeyFixture::new();
        let attacker = KeyFixture::new();
        let cid = ipfs_cid(b"content the attacker wants to serve");
        let forged = attacker.sign_v2_only(&format!("/ipfs/{cid}"), 1, ChronoDuration::hours(24));

        let mut source = PinnedRecordSource::default();
        source.put(&key.name, forged);
        let err = resolve_ipns_name(&source, &key.name)
            .expect_err("a wrong-key V2-only record must not verify");
        assert!(
            matches!(err, IpnsError::Unverifiable { .. }),
            "got: {err:?}"
        );
    }

    #[test]
    fn an_invalid_ipns_name_is_a_distinct_failure_before_any_fetch() {
        // Fail-closed: a name that is not a valid libp2p-key CID is rejected as a
        // distinct InvalidName, before any record fetch.
        let source = PinnedRecordSource::default();
        let err = resolve_ipns_name(&source, "not-a-valid-ipns-name")
            .expect_err("an unparseable name fails closed");
        assert!(matches!(err, IpnsError::InvalidName { .. }), "got: {err:?}");
    }

    #[test]
    fn a_record_fetch_failure_is_a_distinct_source_error() {
        // Fail-closed: a source/transport failure surfaces as a distinct Source
        // error, never a guessed resolution.
        let key = KeyFixture::new();
        let source = PinnedRecordSource::failing(IpnsError::Source("connection refused".into()));
        let err = resolve_ipns_name(&source, &key.name).expect_err("a fetch failure fails closed");
        assert!(matches!(err, IpnsError::Source(_)), "got: {err:?}");
    }

    #[test]
    fn a_malformed_record_is_a_distinct_failure() {
        // Fail-closed: bytes that are not a decodable IPNS record are a distinct
        // MalformedRecord, never verified against nothing.
        let key = KeyFixture::new();
        let mut source = PinnedRecordSource::default();
        source.put(&key.name, b"not a valid ipns record".to_vec());
        let err =
            resolve_ipns_name(&source, &key.name).expect_err("a malformed record fails closed");
        assert!(matches!(err, IpnsError::MalformedRecord(_)), "got: {err:?}");
    }

    #[test]
    fn a_record_signed_by_the_wrong_key_does_not_verify() {
        // The load-bearing trust failure: a record correctly signed, but by a
        // DIFFERENT key than the requested name, must NOT verify \u2014 a misdirecting
        // source cannot repoint a name it does not hold the key for.
        let key = KeyFixture::new();
        let other = KeyFixture::new();
        let cid = ipfs_cid(b"content the attacker wants to serve");
        // `other` signs a record, but we ask to resolve `key`'s name with it.
        let forged = other.sign(&format!("/ipfs/{cid}"), 1, ChronoDuration::hours(24));

        let mut source = PinnedRecordSource::default();
        source.put(&key.name, forged);

        let err = resolve_ipns_name(&source, &key.name)
            .expect_err("a record signed by the wrong key must not verify");
        assert!(
            matches!(err, IpnsError::Unverifiable { .. }),
            "got: {err:?}"
        );
    }

    #[test]
    fn an_expired_record_does_not_verify() {
        // Fail-closed: a record whose EOL validity has already elapsed must NOT
        // verify \u2014 a stale record cannot resolve the name.
        let key = KeyFixture::new();
        let cid = ipfs_cid(b"stale content");
        // Signed valid until an hour AGO.
        let expired = key.sign(&format!("/ipfs/{cid}"), 1, ChronoDuration::hours(-1));

        let mut source = PinnedRecordSource::default();
        source.put(&key.name, expired);

        let err =
            resolve_ipns_name(&source, &key.name).expect_err("an expired record must not verify");
        assert!(
            matches!(err, IpnsError::Unverifiable { .. }),
            "got: {err:?}"
        );
    }

    #[test]
    fn a_verified_record_pointing_at_another_name_is_an_unsupported_target() {
        // Fail-closed: a verified record whose value is a `/ipns/<name>` chain is
        // refused as an explicit unsupported target (a named follow-on), never
        // blindly chained.
        let key = KeyFixture::new();
        let other = KeyFixture::new();
        let record = key.sign(
            &format!("/ipns/{}", other.name),
            1,
            ChronoDuration::hours(24),
        );

        let mut source = PinnedRecordSource::default();
        source.put(&key.name, record);

        let err =
            resolve_ipns_name(&source, &key.name).expect_err("an ipns chain target is unsupported");
        assert!(
            matches!(err, IpnsError::UnsupportedTarget { .. }),
            "got: {err:?}"
        );
    }

    #[test]
    fn a_verified_record_with_a_broken_ipfs_target_is_an_invalid_target() {
        // Fail-closed: a verified record whose `/ipfs/<cid>` CID does not parse is
        // a distinct InvalidTarget \u2014 the record verified, but it names no loadable
        // content.
        let key = KeyFixture::new();
        let record = key.sign("/ipfs/not-a-real-cid", 1, ChronoDuration::hours(24));

        let mut source = PinnedRecordSource::default();
        source.put(&key.name, record);

        let err =
            resolve_ipns_name(&source, &key.name).expect_err("a broken ipfs target fails closed");
        assert!(
            matches!(err, IpnsError::InvalidTarget { .. }),
            "got: {err:?}"
        );
    }

    #[test]
    fn a_verified_record_with_a_non_path_target_is_an_invalid_target() {
        // Fail-closed: a verified record whose value is neither /ipfs/ nor /ipns/
        // is a distinct InvalidTarget, never guessed.
        let key = KeyFixture::new();
        let record = key.sign("https://example.com/", 1, ChronoDuration::hours(24));

        let mut source = PinnedRecordSource::default();
        source.put(&key.name, record);

        let err =
            resolve_ipns_name(&source, &key.name).expect_err("a non-path target fails closed");
        assert!(
            matches!(err, IpnsError::InvalidTarget { .. }),
            "got: {err:?}"
        );
    }

    // -- The default gateway record source, off the live network. -------------

    /// A [`Fetcher`] double that returns a canned record body for every GET,
    /// isolated from the live network. Records the last URL so the test can
    /// assert the `?format=ipns-record` request shape.
    struct CannedRecordFetcher {
        body: Vec<u8>,
        status: u16,
        last_url: RefCell<String>,
    }

    impl Fetcher for CannedRecordFetcher {
        fn fetch(&self, url: &str) -> Result<Response, FetchError> {
            *self.last_url.borrow_mut() = url.to_string();
            Ok(Response {
                status: self.status,
                content_type: Some("application/vnd.ipfs.ipns-record".into()),
                body: self.body.clone(),
                final_url: url.to_string(),
            })
        }
    }

    #[test]
    fn the_gateway_source_requests_the_ipns_record_format_and_resolves() {
        // The default backend fetches `GET /ipns/{name}?format=ipns-record` (the
        // trustless-gateway contract) and, wired into the resolver, resolves the
        // name end to end off the live network.
        let key = KeyFixture::new();
        let cid = ipfs_cid(b"content served as a canned record over the gateway seam");
        let record = key.sign(&format!("/ipfs/{cid}"), 1, ChronoDuration::hours(24));

        let fetcher = CannedRecordFetcher {
            body: record,
            status: 200,
            last_url: RefCell::new(String::new()),
        };
        let source = GatewayIpnsRecordSource::with_gateway(fetcher, "http://gw.test");

        let resolved = resolve_ipns_name(&source, &key.name).expect("resolves over the seam");
        assert_eq!(resolved.cid, cid);

        let url = source.fetcher.last_url.borrow().clone();
        assert!(
            url.contains("format=ipns-record"),
            "expected a ?format=ipns-record GET, got: {url}"
        );
        assert!(
            url.contains(&key.name),
            "the request names the ipns name: {url}"
        );
        assert!(
            url.starts_with("http://gw.test/ipns/"),
            "the /ipns/ path: {url}"
        );
    }

    #[test]
    fn a_non_2xx_gateway_status_is_a_distinct_source_failure() {
        // Fail-closed: a gateway that answers non-2xx (no record) is a distinct
        // Source failure, never a guessed resolution.
        let key = KeyFixture::new();
        let fetcher = CannedRecordFetcher {
            body: b"not found".to_vec(),
            status: 404,
            last_url: RefCell::new(String::new()),
        };
        let source = GatewayIpnsRecordSource::with_gateway(fetcher, "http://gw.test");
        let err = resolve_ipns_name(&source, &key.name).expect_err("a 404 fails closed");
        assert!(matches!(err, IpnsError::Source(_)), "got: {err:?}");
    }
}
