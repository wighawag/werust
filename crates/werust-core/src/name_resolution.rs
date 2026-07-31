//! The ONE name-to-content resolution path: an ENS name in, the immutable
//! `ipfs://<cid>` werust would actually load out — FOLLOWING a mutable `ipns-ns`
//! contenthash through its CLIENT-VERIFIED IPNS record on the way.
//!
//! # Why this module exists
//!
//! The chain (namehash -> registry -> resolver -> ENSIP-7 decode -> and, for an
//! `ipns-ns` name, fetch + client-verify the signed record) used to live INSIDE
//! [`BrowserShell::navigate`](crate::BrowserShell::navigate)'s ENS front door, so
//! it was reachable only by driving a renderer. The headless CLI therefore
//! stopped one step short — it printed the `ipns://<name>` POINTER while the GUI
//! on the same name followed the record and rendered the site, i.e. the two
//! surfaces disagreed about what a name resolves to (task
//! `cli-resolve-follows-mutable-names-to-the-cid`). Lifting the name-to-CID step
//! here makes it callable with no renderer, and the shell now calls THIS: one
//! implementation, so a record that fails verification fails the same way in both
//! surfaces and neither can drift ahead of the other.
//!
//! # The trust posture is carried, not flattened (`docs/adr/0006`, `docs/adr/0007`)
//!
//! A followed `ipns-ns` name is a MUTABLE name: its controller can repoint it at
//! any time, so its resolved CID is "what it points at right now", never the
//! immutable identity an `ipfs-ns` name has. [`ResolvedName`] keeps the two cases
//! DISTINCT and carries BOTH facts for the mutable one (the pointer that was
//! followed AND the CID it currently resolves to), so no caller can report a
//! followed name as if it were plain content-verified: the shell flags the load
//! mutable-named, and the CLI says so in its output.
//!
//! # Fail-closed
//!
//! Nothing is guessed or partially resolved: every failure is the ENS core's or
//! the IPNS core's OWN typed reason, wrapped in [`NameResolutionError`] and
//! rendered by their own [`Display`](std::fmt::Display). In particular a record
//! whose signature / name-binding / validity does not check out is an error, never
//! a CID (`docs/adr/0007`).

use crate::contenthash::{DecodedContenthash, ProtoCode};
use crate::ens::ResolutionError;
use crate::ethereum::EthereumProvider;
use crate::ipns::{IpnsError, IpnsRecordSource};
use crate::LoadStep;

/// The `ipns` scheme a MUTABLE name's POINTER is reported under (the ENSIP-7
/// `ipns-ns` contenthash rendered as a reference: `ipns://<name>`).
///
/// Kept beside the resolution that produces it so the one spelling is minted
/// here rather than in each surface that prints it. Note that a bare `ipns://`
/// URL-bar ENTRY is still an unbuilt follow-on (`docs/adr/0007`, decision 4):
/// this string identifies the pointer that was followed, it is not yet something
/// `werust <url>` opens.
const IPNS_SCHEME: &str = "ipns";

/// What an ENS name resolves to: the immutable `ipfs://<cid>` werust loads, plus
/// whether a MUTABLE name was followed to get there.
///
/// The two cases are deliberately distinct rather than one "here is a CID"
/// answer: an `ipfs-ns` name IS its CID, whereas an `ipns-ns` name merely POINTS
/// at one today (`docs/adr/0006`). Flattening them would erase the mutability the
/// trust model rests on, so [`Mutable`](ResolvedName::Mutable) carries the
/// followed pointer alongside the CID and every consumer can report both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedName {
    /// An immutable `ipfs-ns` name: the contenthash IS the CID. No record fetch
    /// happened — this case makes NO network call beyond the ENS reads.
    Immutable {
        /// The `ipfs://<cid>` URI, ready to feed the verified `ipfs://` path.
        uri: String,
        /// The canonical CID string (the `<cid>` in the URI).
        cid: String,
    },
    /// A MUTABLE `ipns-ns` name, FOLLOWED through a client-verified IPNS record
    /// to the CID it currently points at.
    Mutable {
        /// The `ipns://<name>` pointer that was followed (the ENS contenthash
        /// itself), kept so a caller can report WHERE the CID came from.
        pointer: String,
        /// The `ipfs://<cid>` URI the record currently resolves to.
        uri: String,
        /// The canonical CID string the record currently resolves to.
        cid: String,
    },
}

impl ResolvedName {
    /// The `ipfs://<cid>` URI to load, whichever case this is.
    #[must_use]
    pub fn uri(&self) -> &str {
        match self {
            ResolvedName::Immutable { uri, .. } | ResolvedName::Mutable { uri, .. } => uri,
        }
    }

    /// The canonical CID string, whichever case this is.
    #[must_use]
    pub fn cid(&self) -> &str {
        match self {
            ResolvedName::Immutable { cid, .. } | ResolvedName::Mutable { cid, .. } => cid,
        }
    }

    /// Whether the CID came from a MUTABLE name (a followed `ipns-ns` pointer),
    /// so the load's honest posture is at most `MutableName` and never plain
    /// immutable content-verified (`docs/adr/0006`).
    #[must_use]
    pub fn is_mutable(&self) -> bool {
        matches!(self, ResolvedName::Mutable { .. })
    }

    /// The `ipns://<name>` pointer that was followed, for a mutable name; `None`
    /// for an immutable one.
    #[must_use]
    pub fn mutable_pointer(&self) -> Option<&str> {
        match self {
            ResolvedName::Immutable { .. } => None,
            ResolvedName::Mutable { pointer, .. } => Some(pointer),
        }
    }

    /// The ENSIP-7 protocol the NAME's contenthash used, so a machine-readable
    /// surface can print the one shared protocol vocabulary
    /// ([`ProtoCode::wire_name`]: `ipfs-ns` / `ipns-ns`) instead of minting its
    /// own spelling.
    #[must_use]
    pub fn proto_code(&self) -> ProtoCode {
        match self {
            ResolvedName::Immutable { .. } => ProtoCode::Ipfs,
            ResolvedName::Mutable { .. } => ProtoCode::Ipns,
        }
    }
}

/// A DISTINCT fail-closed failure of resolving a name all the way to content.
///
/// One wrapper over the two cores the path composes, so the caller gets ONE error
/// type while each step keeps its OWN typed reason (nothing is flattened into a
/// generic string): the chrome's error banner, the CLI's stderr line and any
/// later surface all print the same sentence for the same failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameResolutionError {
    /// The ENS read failed, or the name's contenthash names a protocol werust
    /// does not support (a NAMED refusal, never a mis-dispatch).
    Ens(ResolutionError),
    /// The name is a MUTABLE `ipns-ns` pointer whose record could not be fetched,
    /// decoded, VERIFIED, or whose target is not loadable. A record that does not
    /// verify lands here — it is never resolved to a CID (`docs/adr/0007`).
    Ipns(IpnsError),
}

impl std::fmt::Display for NameResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Forward verbatim: each core already writes the legible, protocol-named
        // sentence its surfaces show, and a second wording here would be a copy to
        // drift from.
        match self {
            NameResolutionError::Ens(e) => write!(f, "{e}"),
            NameResolutionError::Ipns(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for NameResolutionError {}

/// Resolve `name` to the content werust would actually load: the ENS read, then
/// — for a MUTABLE `ipns-ns` contenthash — the client-verified IPNS record that
/// says which CID it points at right now.
///
/// This is the callable form of the browser's own front-door step, for callers
/// with no renderer and no chrome (the headless `werust resolve`). The shell uses
/// [`resolve_name_with_progress`], the same implementation with a step callback.
///
/// `provider` and `ipns_source` are `&dyn` for the same reason the underlying
/// cores take them that way: the trusted Phase-1 RPC and the trustless-gateway
/// record source today, a Phase-2 light client and another record backend later,
/// and in-process fixtures in tests (so this path is exercised off the live
/// network).
pub fn resolve_name(
    provider: &dyn EthereumProvider,
    ipns_source: &dyn IpnsRecordSource,
    name: &str,
) -> Result<ResolvedName, NameResolutionError> {
    resolve_name_with_progress(provider, ipns_source, name, &mut |_| {})
}

/// [`resolve_name`], reporting each pipeline stage it enters through `on_step`.
///
/// The stages are the chrome's OWN [`LoadStep`] vocabulary
/// ([`ResolvingName`](LoadStep::ResolvingName), then
/// [`FetchingRecord`](LoadStep::FetchingRecord) for a mutable name only) rather
/// than a second progress enum minted here: the shell pins exactly these values
/// so the load indicator names the stage a resolution is at, and a duplicate
/// vocabulary would be one more pair of names to keep in step. Only the
/// RESOLUTION stages are reported — the content fetch that follows belongs to the
/// backend, not to this function.
///
/// Resolution is synchronous, so the callback fires strictly before the step's
/// work; a caller that only wants the result uses [`resolve_name`].
pub fn resolve_name_with_progress(
    provider: &dyn EthereumProvider,
    ipns_source: &dyn IpnsRecordSource,
    name: &str,
    on_step: &mut dyn FnMut(LoadStep),
) -> Result<ResolvedName, NameResolutionError> {
    // Step 1: the ENS read (namehash -> registry -> resolver -> ENSIP-7 decode).
    on_step(LoadStep::ResolvingName);
    let decoded = crate::ens::resolve(provider, name).map_err(NameResolutionError::Ens)?;
    match decoded {
        // The immutable `ipfs-ns` case: the contenthash IS the CID. No record
        // fetch, no second network hop.
        DecodedContenthash::Ipfs { uri, cid } => Ok(ResolvedName::Immutable { uri, cid }),
        // The MUTABLE `ipns-ns` case: follow the pointer through a signed record
        // that is fetched from an UNTRUSTED source and VERIFIED client-side
        // against the key before its CID is used at all (`docs/adr/0007`).
        DecodedContenthash::Ipns { name: ipns_name } => {
            // Step 2 (mutable names only): fetch + client-verify the record.
            on_step(LoadStep::FetchingRecord);
            let resolved = crate::ipns::resolve_ipns_name(ipns_source, &ipns_name)
                .map_err(NameResolutionError::Ipns)?;
            Ok(ResolvedName::Mutable {
                pointer: format!("{IPNS_SCHEME}://{ipns_name}"),
                uri: resolved.uri,
                cid: resolved.cid,
            })
        }
        // A well-formed but unsupported contenthash. `ens::resolve` already maps
        // this to `Err(UnsupportedContenthash)`, so it does not normally arrive
        // here; dispatching on the decoded kind's OWN shape means a contract
        // change cannot turn it into a fake success.
        DecodedContenthash::Unsupported(proto) => Err(NameResolutionError::Ens(
            ResolutionError::UnsupportedContenthash(proto),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ethereum::{EthCall, ProviderError};
    use chrono::{Duration as ChronoDuration, Utc};
    use fetcher::{cid_v1_raw_sha256, Cid};
    use libp2p_identity::Keypair;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    /// An in-process [`EthereumProvider`] double answering each `eth_call` in
    /// order from a queue of canned results — the ENS reads run off the live
    /// network (the same shape the `ens` / shell tests use).
    struct ScriptedProvider {
        answers: RefCell<VecDeque<Result<Vec<u8>, ProviderError>>>,
    }

    impl ScriptedProvider {
        fn new(answers: Vec<Result<Vec<u8>, ProviderError>>) -> Self {
            Self {
                answers: RefCell::new(answers.into_iter().collect()),
            }
        }
    }

    impl EthereumProvider for ScriptedProvider {
        fn eth_call(&self, _call: &EthCall) -> Result<Vec<u8>, ProviderError> {
            self.answers
                .borrow_mut()
                .pop_front()
                .expect("the scripted provider ran out of canned answers")
        }
    }

    /// A 32-byte ABI word holding a right-aligned 20-byte address (a
    /// `resolver(node)` return).
    fn address_word(addr20: &[u8; 20]) -> Vec<u8> {
        let mut word = vec![0u8; 32];
        word[12..32].copy_from_slice(addr20);
        word
    }

    /// ABI-encode a dynamic `bytes` return (a `contenthash(node)` result).
    fn abi_bytes_return(payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut offset = [0u8; 32];
        offset[31] = 0x20;
        out.extend_from_slice(&offset);
        let mut len = [0u8; 32];
        len[24..32].copy_from_slice(&(payload.len() as u64).to_be_bytes());
        out.extend_from_slice(&len);
        out.extend_from_slice(payload);
        let pad = (32 - payload.len() % 32) % 32;
        out.extend(std::iter::repeat_n(0u8, pad));
        out
    }

    /// Encode a multicodec protoCode as an unsigned LEB128 varint (the real
    /// on-the-wire contenthash prefix).
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

    /// The raw ENSIP-7 `ipfs-ns` contenthash bytes for a fixture site, plus the
    /// canonical `ipfs://<cid>` URI they decode to.
    fn ipfs_contenthash_fixture(bytes: &[u8]) -> (Vec<u8>, String, String) {
        let cid_str = cid_v1_raw_sha256(bytes).expect("derive fixture cid");
        let cid_bytes = Cid::try_from(cid_str.as_str())
            .expect("cid parses")
            .to_bytes();
        let mut ch = varint(0xe3);
        ch.extend_from_slice(&cid_bytes);
        (ch, format!("ipfs://{cid_str}"), cid_str)
    }

    /// A minted ed25519 keypair + the canonical base36 IPNS name it corresponds
    /// to, PLUS the raw `ipns-ns` contenthash bytes an ENS resolver returns for
    /// it. A REAL key, so the record it signs genuinely verifies against the name
    /// the ENSIP-7 decoder produces — all off the live network.
    struct IpnsKeyFixture {
        keypair: Keypair,
        name: String,
        contenthash: Vec<u8>,
    }

    impl IpnsKeyFixture {
        fn new() -> Self {
            let keypair = Keypair::generate_ed25519();
            let peer_id = keypair.public().to_peer_id();
            const LIBP2P_KEY_CODEC: u64 = 0x72;
            let mh = cid::multihash::Multihash::from_bytes(&peer_id.to_bytes())
                .expect("peer id is a multihash");
            let name_cid = Cid::new_v1(LIBP2P_KEY_CODEC, mh);
            let name = name_cid
                .to_string_of_base(cid::multibase::Base::Base36Lower)
                .expect("base36 name");
            let mut contenthash = varint(0xe5);
            contenthash.extend_from_slice(&name_cid.to_bytes());
            Self {
                keypair,
                name,
                contenthash,
            }
        }

        /// Sign a record pointing the name at `/ipfs/<cid>`, valid 24h, seq 1.
        fn signed_record_for(&self, ipfs_cid: &str) -> Vec<u8> {
            let record = rust_ipns::Record::new(
                &self.keypair,
                format!("/ipfs/{ipfs_cid}").as_bytes(),
                Utc::now() + ChronoDuration::hours(24),
                1,
                std::time::Duration::from_secs(3600),
            )
            .expect("sign an ipns record");
            record.encode().expect("encode the signed record")
        }
    }

    /// A pinned in-memory [`IpnsRecordSource`] that also COUNTS its fetches, so a
    /// test can assert the immutable path makes no record call at all.
    struct CountingRecordSource {
        records: std::collections::HashMap<String, Vec<u8>>,
        fetches: Cell<usize>,
    }

    impl CountingRecordSource {
        fn empty() -> Self {
            Self {
                records: std::collections::HashMap::new(),
                fetches: Cell::new(0),
            }
        }

        fn with_record(name: &str, record: Vec<u8>) -> Self {
            let mut source = Self::empty();
            source.records.insert(name.to_string(), record);
            source
        }
    }

    impl IpnsRecordSource for CountingRecordSource {
        fn fetch_record(&self, name: &str) -> Result<Vec<u8>, IpnsError> {
            self.fetches.set(self.fetches.get() + 1);
            self.records
                .get(name)
                .cloned()
                .ok_or_else(|| IpnsError::Source(format!("no record pinned for {name}")))
        }
    }

    #[test]
    fn a_mutable_ipns_name_is_followed_through_its_verified_record_to_the_cid() {
        // Acceptance: an ENS name whose contenthash is an `ipns-ns` pointer
        // resolves all the way to the `ipfs://<cid>` the browser would load, by
        // fetching and CLIENT-VERIFYING the signed record first — and the mutable
        // fact is NOT lost: the followed pointer rides along with the CID.
        let key = IpnsKeyFixture::new();
        let target_cid =
            cid_v1_raw_sha256(b"the site the name points at today").expect("derive the target cid");
        let source =
            CountingRecordSource::with_record(&key.name, key.signed_record_for(&target_cid));
        let provider = ScriptedProvider::new(vec![
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&key.contenthash)),
        ]);

        let resolved =
            resolve_name(&provider, &source, "ronan.eth").expect("the name follows through");

        assert_eq!(
            resolved,
            ResolvedName::Mutable {
                pointer: format!("ipns://{}", key.name),
                uri: format!("ipfs://{target_cid}"),
                cid: target_cid.clone(),
            }
        );
        // Both facts are available to a caller, and the protocol vocabulary is the
        // core's ENSIP-7 spelling.
        assert!(resolved.is_mutable());
        assert_eq!(
            resolved.mutable_pointer(),
            Some(format!("ipns://{}", key.name).as_str())
        );
        assert_eq!(resolved.cid(), target_cid);
        assert_eq!(resolved.proto_code().wire_name(), "ipns-ns");
        assert_eq!(source.fetches.get(), 1, "exactly one record fetch");
    }

    #[test]
    fn an_immutable_ipfs_name_resolves_with_no_record_fetch() {
        // Acceptance: the immutable `ipfs-ns` case behaves as it always did — one
        // CID, no record fetch, no extra network call.
        let (contenthash, uri, cid) = ipfs_contenthash_fixture(b"an immutable site");
        let source = CountingRecordSource::empty();
        let provider = ScriptedProvider::new(vec![
            Ok(address_word(&[0x22u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);

        let resolved = resolve_name(&provider, &source, "example.eth").expect("an ipfs-ns name");

        assert_eq!(
            resolved,
            ResolvedName::Immutable {
                uri: uri.clone(),
                cid: cid.clone(),
            }
        );
        assert!(!resolved.is_mutable());
        assert_eq!(resolved.mutable_pointer(), None);
        assert_eq!(resolved.proto_code().wire_name(), "ipfs-ns");
        assert_eq!(
            source.fetches.get(),
            0,
            "an immutable name must not touch the IPNS record source"
        );
    }

    #[test]
    fn a_record_that_does_not_verify_fails_closed_with_the_ipns_core_reason() {
        // Acceptance: the verification is the SAME core path the GUI walks, so a
        // record signed by a DIFFERENT key than the name is refused here exactly
        // as it is in the browser — a typed `Unverifiable`, never a CID.
        let key = IpnsKeyFixture::new();
        let impostor = IpnsKeyFixture::new();
        let target_cid = cid_v1_raw_sha256(b"a site an impostor claims").expect("derive a cid");
        // The record is served UNDER `key`'s name but signed by the impostor's key.
        let source =
            CountingRecordSource::with_record(&key.name, impostor.signed_record_for(&target_cid));
        let provider = ScriptedProvider::new(vec![
            Ok(address_word(&[0x33u8; 20])),
            Ok(abi_bytes_return(&key.contenthash)),
        ]);

        let err = resolve_name(&provider, &source, "ronan.eth")
            .expect_err("an unverifiable record must not resolve");

        assert!(
            matches!(err, NameResolutionError::Ipns(_)),
            "the IPNS core's own typed failure: {err:?}"
        );
        assert!(
            err.to_string().contains("IPNS record"),
            "the reason is the core's own sentence: {err}"
        );
    }

    #[test]
    fn an_unsupported_contenthash_is_a_named_refusal_not_a_reference() {
        // Fail-closed: a well-formed contenthash for a protocol werust does not
        // support is the decoder's OWN protocol-named refusal, never a resolved
        // reference.
        let mut contenthash = varint(0xe4); // swarm-ns
        contenthash.extend_from_slice(b"a swarm address");
        let source = CountingRecordSource::empty();
        let provider = ScriptedProvider::new(vec![
            Ok(address_word(&[0x44u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);

        let err = resolve_name(&provider, &source, "swarmy.eth").expect_err("swarm is refused");

        assert!(matches!(
            err,
            NameResolutionError::Ens(ResolutionError::UnsupportedContenthash(ProtoCode::Swarm))
        ));
        assert!(err.to_string().contains("Swarm"), "named: {err}");
    }

    #[test]
    fn the_reported_steps_are_the_chromes_own_resolution_stages() {
        // The progress the shell pins is the pipeline's real stages: an immutable
        // name reports only `ResolvingName`, a mutable one also reports
        // `FetchingRecord` before the record hop.
        let (contenthash, _uri, _cid) = ipfs_contenthash_fixture(b"immutable");
        let source = CountingRecordSource::empty();
        let provider = ScriptedProvider::new(vec![
            Ok(address_word(&[0x55u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);
        let mut steps = Vec::new();
        resolve_name_with_progress(&provider, &source, "example.eth", &mut |step| {
            steps.push(step);
        })
        .expect("an ipfs-ns name");
        assert_eq!(steps, vec![LoadStep::ResolvingName]);

        let key = IpnsKeyFixture::new();
        let target_cid = cid_v1_raw_sha256(b"mutable target").expect("derive a cid");
        let source =
            CountingRecordSource::with_record(&key.name, key.signed_record_for(&target_cid));
        let provider = ScriptedProvider::new(vec![
            Ok(address_word(&[0x66u8; 20])),
            Ok(abi_bytes_return(&key.contenthash)),
        ]);
        let mut steps = Vec::new();
        resolve_name_with_progress(&provider, &source, "ronan.eth", &mut |step| {
            steps.push(step);
        })
        .expect("an ipns-ns name");
        assert_eq!(
            steps,
            vec![LoadStep::ResolvingName, LoadStep::FetchingRecord]
        );
    }
}
