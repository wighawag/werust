//! The [`ContentRetriever`] seam: retrieve the VERIFIED bytes for a resource
//! named by a CID (plus a path into its DAG), or a typed fail-closed failure.
//!
//! This is the seam that makes a REAL multi-block IPFS site legitimately
//! content-verified. The single-block [`ContentAddressedFetcher`](crate::ContentAddressedFetcher)
//! re-hashes a gateway's bytes against the root CID, which works ONLY for a
//! `raw`/leaf CID whose block bytes ARE the content; every real `.eth`/`ipfs://`
//! site is a multi-block UnixFS `dag-pb` DAG (a directory of `index.html` +
//! assets, or a chunked file) whose root CID names an IPLD node that LINKS to
//! child blocks, so re-hashing the reassembled bytes against the root CID
//! FAILS on every real site. The only honest proof is to WALK the DAG and
//! verify EACH block against its OWN CID, then reassemble locally
//! (`docs/adr/0004`).
//!
//! # The seam is the abstraction; the backend is a swap
//!
//! [`ContentRetriever`] is modelled exactly like the [`Fetcher`](crate::Fetcher)
//! / `EthereumProvider` / `Renderer` seams: the trait is the abstraction
//! ("given a CID + a path into the DAG, return the verified bytes for that
//! resource, or a typed failure"); the trust/transport is a swappable BACKEND.
//! One default backend ships now, [`TrustlessGatewayCarRetriever`] (a trustless
//! gateway CAR fetcher, NO IPFS node, NO async runtime). A delegated-routing
//! backend, an embedded-p2p (Phase-2 async) backend, and a user-supplied
//! gateway/node URL are all pure backend swaps behind this one seam; the
//! user-facing selector is its own task (`retrieval-backend-user-setting`).
//!
//! # Trust honesty: codec-gated, fail-closed, budgeted
//!
//! "content-verified" MUST mean EVERY byte was hash-checked (`docs/adr/0001`).
//! The backend discriminates by the CID's multicodec: a `raw` (0x55) CID's
//! block bytes ARE the content and are verified directly (a mismatch is a HARD
//! tamper failure that is NEVER served); a `dag-pb` (0x70) CID is a UnixFS DAG
//! root walked and verified block-by-block. Every failure is DISTINCT and
//! fail-closed (a mis-hashing block, a missing linked block, an incomplete CAR
//! stream [the Trustless Gateway spec's client obligation], an unresolved path,
//! an unsupported codec/hash), and a retrieval BUDGET (max total bytes / max
//! block count) refuses a runaway or hostile DAG so a malicious gateway cannot
//! stream forever. Nothing unverified is ever returned.

use std::collections::HashMap;
use std::fmt;

use ipld_dagpb::PbNode;
use quick_protobuf::BytesReader;
use rs_car_sync::{CarDecodeError, CarReader};

use crate::{Cid, FetchError, Fetcher};

/// The `raw` IPLD multicodec code: a block's bytes ARE the content.
const RAW_CODEC: u64 = 0x55;
/// The `dag-pb` IPLD multicodec code: a block is a MerkleDAG/UnixFS node that
/// LINKS to child blocks.
const DAG_PB_CODEC: u64 = 0x70;

/// The default per-retrieval budget: at most this many total block bytes.
///
/// A safe upper bound so a hostile gateway cannot stream forever. 32 MiB
/// comfortably covers a real static site (an `index.html` + css/js/images)
/// while refusing a runaway DAG. Overridable via
/// [`RetrievalBudget::with_max_bytes`].
const DEFAULT_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// The default per-retrieval budget: at most this many blocks.
///
/// Bounds the block COUNT independently of total size so a DAG of many tiny
/// blocks (a fan-out attack) is refused even under the byte budget. Overridable
/// via [`RetrievalBudget::with_max_blocks`].
const DEFAULT_MAX_BLOCKS: u64 = 100_000;

/// A retrieval budget: the hard ceilings a single retrieval may not cross.
///
/// The Trustless Gateway spec makes CAR completeness the CLIENT's obligation, so
/// the client must also refuse a gateway that streams a runaway or hostile DAG.
/// This budget bounds total block bytes AND block count; crossing either is a
/// fail-closed [`RetrieveError::BudgetExceeded`] (the retrieval is abandoned,
/// nothing partial is returned). Wall-clock is bounded by the underlying
/// [`Fetcher`](crate::Fetcher)'s own request timeout (a CAR fetch is a single
/// GET), so this budget owns the size/count ceilings and the transport owns the
/// time ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetrievalBudget {
    /// The maximum total block bytes a retrieval may consume.
    pub max_bytes: u64,
    /// The maximum number of blocks a retrieval may read.
    pub max_blocks: u64,
}

impl Default for RetrievalBudget {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
            max_blocks: DEFAULT_MAX_BLOCKS,
        }
    }
}

impl RetrievalBudget {
    /// Override the total-bytes ceiling.
    #[must_use]
    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// Override the block-count ceiling.
    #[must_use]
    pub fn with_max_blocks(mut self, max_blocks: u64) -> Self {
        self.max_blocks = max_blocks;
        self
    }
}

/// The verified bytes for a resolved resource, plus the codec of the CID they
/// were addressed under.
///
/// Returned by [`ContentRetriever::retrieve`] ONLY after every contributing
/// block hashed to its own CID. `codec` is the multicodec of the resource's own
/// CID (`raw` for a leaf, `dag-pb` for a UnixFS file/directory resource), so a
/// caller can reason about what it got without re-parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievedContent {
    /// The reassembled, fully hash-verified bytes of the resolved resource.
    pub bytes: Vec<u8>,
    /// The multicodec code of the CID the resolved resource was addressed under.
    pub codec: u64,
}

/// A fail-closed failure of a content retrieval, each cause DISTINCT.
///
/// Every way a retrieval can fail is its own variant so a caller (and the trust
/// indicator) can report a legible reason, and so NONE of them can be confused
/// with success. The load-bearing ones are [`BlockHashMismatch`](RetrieveError::BlockHashMismatch)
/// (a block did not hash to its CID: tamper, NEVER served), [`IncompleteCar`](RetrieveError::IncompleteCar)
/// / [`MissingBlock`](RetrieveError::MissingBlock) (the CAR was truncated or a
/// linked block was never delivered: the client's completeness obligation), and
/// [`BudgetExceeded`](RetrieveError::BudgetExceeded) (a runaway/hostile DAG).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrieveError {
    /// The CID string could not be parsed as a content identifier.
    InvalidCid(String),
    /// A block's bytes did NOT hash to the CID that named it: tamper. The bytes
    /// are rejected and NEVER contribute to the result.
    BlockHashMismatch {
        /// The CID whose block failed to verify (canonical string form).
        cid: String,
    },
    /// The CID (or a link inside the DAG) names a hash function this path does
    /// not verify (only `sha2-256` today). Refused, never trusted.
    UnsupportedHash {
        /// The multihash code encountered.
        code: u64,
    },
    /// The CID (or a link) uses an IPLD codec this backend does not resolve
    /// (only `raw` and `dag-pb`/UnixFS are in scope; `dag-cbor`/`dag-json` are
    /// named follow-ons). Refused rather than guessed.
    UnsupportedCodec {
        /// The multicodec code encountered.
        code: u64,
    },
    /// The CAR stream was truncated / incomplete: the client's completeness
    /// obligation was violated, so the retrieval fails closed.
    IncompleteCar(String),
    /// The CAR was well-formed but a block a link points to was never delivered,
    /// so the DAG cannot be reassembled. Fail-closed.
    MissingBlock {
        /// The linked CID that was expected but absent from the CAR.
        cid: String,
    },
    /// The requested path did not resolve to a resource in the DAG (no such
    /// directory entry, or a directory with no `index.html`).
    PathNotFound {
        /// The path that could not be resolved.
        path: String,
    },
    /// A UnixFS node could not be decoded (malformed dag-pb / UnixFS payload).
    MalformedDag(String),
    /// The retrieval crossed its [`RetrievalBudget`] (total bytes or block
    /// count): a runaway or hostile DAG, abandoned with nothing returned.
    BudgetExceeded(String),
    /// The underlying transport (the gateway fetch) failed.
    Source(FetchError),
}

impl fmt::Display for RetrieveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RetrieveError::InvalidCid(c) => write!(f, "invalid content identifier: {c}"),
            RetrieveError::BlockHashMismatch { cid } => {
                write!(f, "block hash mismatch: bytes do not match cid {cid}")
            }
            RetrieveError::UnsupportedHash { code } => {
                write!(
                    f,
                    "unsupported content hash function (multihash code {code:#x})"
                )
            }
            RetrieveError::UnsupportedCodec { code } => {
                write!(f, "unsupported ipld codec (multicodec {code:#x})")
            }
            RetrieveError::IncompleteCar(m) => write!(f, "incomplete car stream: {m}"),
            RetrieveError::MissingBlock { cid } => {
                write!(f, "missing block: linked cid {cid} was not in the car")
            }
            RetrieveError::PathNotFound { path } => {
                write!(f, "path not found in dag: {path}")
            }
            RetrieveError::MalformedDag(m) => write!(f, "malformed unixfs/dag-pb: {m}"),
            RetrieveError::BudgetExceeded(m) => write!(f, "retrieval budget exceeded: {m}"),
            RetrieveError::Source(e) => write!(f, "content source error: {e}"),
        }
    }
}

impl std::error::Error for RetrieveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RetrieveError::Source(e) => Some(e),
            _ => None,
        }
    }
}

/// The content-retrieval seam: return the VERIFIED bytes for the resource a CID
/// (plus a path into its DAG) names, or a typed fail-closed failure.
///
/// The rest of werust retrieves content-addressed resources ONLY through this
/// trait, so the walk-and-verify can never be skipped by a caller. A `raw` CID
/// resolves to its verified block bytes (the path must be empty/root); a
/// `dag-pb` CID is walked as a UnixFS DAG, resolving the path (a directory to
/// its `index.html`, each `sub/resource` segment into the DAG) and reassembling
/// the leaf bytes, EVERY block hash-checked against its own CID. Nothing
/// unverified is ever returned.
pub trait ContentRetriever {
    /// Retrieve the verified bytes for `cid` at `path`.
    ///
    /// `cid` is a content identifier string (the `<cid>` in `ipfs://<cid>/…`);
    /// `path` is the remainder (e.g. `/style.css`, or empty/`"/"` for the root
    /// resource). On success the returned [`RetrievedContent`] carries the
    /// reassembled, fully hash-verified bytes. Any failure is a distinct
    /// fail-closed [`RetrieveError`]; none ever yields unverified bytes.
    fn retrieve(&self, cid: &str, path: &str) -> Result<RetrievedContent, RetrieveError>;
}

// ---------------------------------------------------------------------------
// The default backend: a trustless-gateway CAR fetcher.
// ---------------------------------------------------------------------------

/// The default IPFS trustless gateway the CAR backend fetches DAG blocks from.
///
/// A gateway is an UNTRUSTED origin: it serves the raw DAG blocks over plain
/// HTTP as a CAR stream (`GET /ipfs/{cid}?format=car`), and the client verifies
/// EACH block against its own CID before any byte is used, so a hostile or buggy
/// gateway cannot cause unverified bytes to be returned. `dweb.link` is a public
/// trustless gateway; the endpoint is overridable via
/// [`TrustlessGatewayCarRetriever::with_gateway`] (the existing `DEFAULT_*` +
/// `with_*()` pattern, no config subsystem), which is how a user-supplied
/// gateway/local-node URL becomes a backend swap.
pub const DEFAULT_TRUSTLESS_GATEWAY: &str = "https://dweb.link";

/// A [`ContentRetriever`] backend that fetches DAG blocks as a CAR stream from a
/// trustless gateway over the bound HTTP [`Fetcher`](crate::Fetcher), verifies
/// each block against its own CID, and reassembles/traverses the UnixFS DAG
/// client-side. NO IPFS node, NO async runtime.
///
/// Construct with [`new`](TrustlessGatewayCarRetriever::new) (the
/// [`DEFAULT_TRUSTLESS_GATEWAY`] and [`RetrievalBudget::default`]) or
/// [`with_gateway`](TrustlessGatewayCarRetriever::with_gateway) /
/// [`with_budget`](TrustlessGatewayCarRetriever::with_budget). It is generic
/// over the [`Fetcher`](crate::Fetcher) so tests drive it against a controlled
/// local endpoint, off the live network, exactly as the fetcher seam's own
/// tests do.
pub struct TrustlessGatewayCarRetriever<F: Fetcher> {
    fetcher: F,
    gateway: String,
    budget: RetrievalBudget,
}

impl<F: Fetcher> TrustlessGatewayCarRetriever<F> {
    /// A CAR retriever over the given HTTP [`Fetcher`](crate::Fetcher), using the
    /// [`DEFAULT_TRUSTLESS_GATEWAY`] and the [`RetrievalBudget::default`].
    pub fn new(fetcher: F) -> Self {
        Self::with_gateway(fetcher, DEFAULT_TRUSTLESS_GATEWAY)
    }

    /// A CAR retriever pointed at a specific trustless-gateway base URL (a local
    /// node, another gateway, or a test endpoint). A trailing `/` is tolerated.
    pub fn with_gateway(fetcher: F, gateway: &str) -> Self {
        Self {
            fetcher,
            gateway: gateway.trim_end_matches('/').to_string(),
            budget: RetrievalBudget::default(),
        }
    }

    /// Override the [`RetrievalBudget`].
    #[must_use]
    pub fn with_budget(mut self, budget: RetrievalBudget) -> Self {
        self.budget = budget;
        self
    }

    /// GET `<gateway>/ipfs/<cid>[/<path>]?format=car&dag-scope=entity` and return
    /// the raw CAR byte stream. The bytes are UNTRUSTED candidate blocks:
    /// verification happens as the CAR is parsed.
    ///
    /// PER-RESOURCE SCOPE (`docs/adr/0004`, IPIP-0402). The requested `path` goes
    /// in the URL and `dag-scope=entity` narrows the response to ONLY the blocks
    /// needed to traverse each path segment plus the terminating entity (a
    /// file's blocks, or a directory's listing), NOT the whole DAG under the
    /// root. This is the fix for the whole-DAG-per-resource refetch: a browser
    /// makes one request for the directory root AND a separate request for each
    /// sub-resource (css/js/images), so `dag-scope=all` meant fetching +
    /// verifying + reassembling the ENTIRE site once PER resource (N full-site
    /// downloads to render one page: slow, partial, timeout-prone). With
    /// `dag-scope=entity` each request pulls only that resource's blocks. Every
    /// returned block is still hash-verified; an incomplete scoped CAR still
    /// fails closed. (`entity-bytes` for range/large-file reads is a named
    /// follow-on; the whole-entity `dag-scope=entity` is the load-bearing fix.)
    /// The scope + directory-index + deferred-cache decisions are recorded in
    /// `docs/spikes/ipfs-per-resource-car-scope-not-whole-dag/DECISIONS.md`.
    fn fetch_car(&self, root_cid: &Cid, path: &str) -> Result<Vec<u8>, RetrieveError> {
        let suffix = encode_url_path(path);
        let url = format!(
            "{gateway}/ipfs/{cid}{suffix}?format=car&dag-scope=entity",
            gateway = self.gateway,
            cid = root_cid,
        );
        let response = self.fetcher.fetch(&url).map_err(RetrieveError::Source)?;
        if !response.is_success() {
            return Err(RetrieveError::Source(FetchError::Transport(format!(
                "gateway returned status {status} for {root_cid}{suffix}",
                status = response.status,
            ))));
        }
        Ok(response.body)
    }
}

impl<F: Fetcher> ContentRetriever for TrustlessGatewayCarRetriever<F> {
    fn retrieve(&self, cid: &str, path: &str) -> Result<RetrievedContent, RetrieveError> {
        let root = Cid::try_from(cid).map_err(|_| RetrieveError::InvalidCid(cid.to_string()))?;

        // The FIRST entity-scoped fetch: the blocks to traverse `path` plus the
        // terminating entity. For a file path this is complete; for a directory
        // terminus (the bare root, or a `.../` directory) it returns only the
        // directory listing, so the walk asks for `<path>/index.html` next.
        let mut store = CarBlockStore::read_and_verify(&self.fetch_car(&root, path)?, self.budget)?;

        // Resolve within the verified blocks. When the walk lands on a directory
        // and must read its `index.html`, it calls back here to fetch that
        // entity's OWN scoped CAR; `resolve_in_dag` merges those verified blocks
        // into `store` itself, so a directory root resolves index.html by
        // fetching only what it needs, not the whole tree. (The closure returns
        // raw CAR bytes rather than touching `store`, so there is no aliasing
        // with the `&mut store` the resolver holds.)
        let mut fetch_more = |entity_path: &str| -> Result<Vec<u8>, RetrieveError> {
            self.fetch_car(&root, entity_path)
        };
        resolve_in_dag(&mut store, &root, path, self.budget, &mut fetch_more)
    }
}

/// Join a resolved parent `path` with a child `name` into the content path the
/// gateway resolves for the child entity (e.g. the directory root `"/"` + child
/// `"index.html"` -> `"/index.html"`). The result is fed to [`encode_url_path`],
/// so it need not be pre-encoded.
fn join_path(path: &str, name: &str) -> String {
    let base = path.trim_end_matches('/');
    format!("{base}/{name}")
}

/// Percent-encode a resource path into the `[/<seg>...]` suffix that follows
/// `/ipfs/<cid>` in a gateway URL, so the gateway resolves the SAME path werust
/// resolves locally.
///
/// Each non-empty segment is encoded conservatively (anything outside the
/// unreserved set + `.`/`-`/`_`/`~` is `%`-escaped), so a segment containing a
/// space, `?`, `#`, or `/` cannot break out of its path component or spill into
/// the query string. An empty/root path yields an empty suffix (`/ipfs/<cid>`).
fn encode_url_path(path: &str) -> String {
    let mut out = String::new();
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        out.push('/');
        for byte in segment.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    out.push(byte as char);
                }
                other => {
                    out.push('%');
                    out.push(
                        char::from_digit((other >> 4) as u32, 16)
                            .unwrap()
                            .to_ascii_uppercase(),
                    );
                    out.push(
                        char::from_digit((other & 0xf) as u32, 16)
                            .unwrap()
                            .to_ascii_uppercase(),
                    );
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// CAR parse + per-block verify (bound to `rs-car-sync`) into a verified store.
// ---------------------------------------------------------------------------

/// The blocks of a CAR stream, each ALREADY verified against its own CID.
///
/// [`read_and_verify`](CarBlockStore::read_and_verify) parses the CAR with
/// `rs-car-sync` (which re-hashes each block against its CID as it reads, so a
/// mis-hashing block is rejected at parse time) under the [`RetrievalBudget`],
/// and indexes the verified blocks by CID. The DAG walk then reads blocks ONLY
/// from this store, so nothing that failed its per-block CID check can reach the
/// reassembly.
struct CarBlockStore {
    blocks: HashMap<Cid, Vec<u8>>,
    /// The CUMULATIVE bytes of all blocks read into this store across every
    /// scoped CAR merged so far, so the [`RetrievalBudget`] bounds the WHOLE
    /// per-resource retrieval, not each scoped fetch in isolation.
    total_bytes: u64,
    /// The cumulative block COUNT across every merged CAR (same rationale).
    block_count: u64,
}

impl CarBlockStore {
    /// Parse `car`, verifying each block against its CID and enforcing `budget`,
    /// into a CID-indexed store of verified blocks.
    fn read_and_verify(car: &[u8], budget: RetrievalBudget) -> Result<Self, RetrieveError> {
        let mut store = Self {
            blocks: HashMap::new(),
            total_bytes: 0,
            block_count: 0,
        };
        store.merge_and_verify(car, budget)?;
        Ok(store)
    }

    /// Parse an ADDITIONAL scoped `car` into this store, verifying each block
    /// against its CID and enforcing `budget` against the CUMULATIVE totals.
    ///
    /// A directory root's `index.html` (and any future scoped follow-on) lives
    /// in a SEPARATE `dag-scope=entity` CAR; its verified blocks are merged in
    /// here. Because blocks are content-addressed, re-seeing an already-present
    /// CID (e.g. the directory node returned by both the directory fetch and the
    /// index.html fetch) is a harmless idempotent overwrite with identical
    /// verified bytes; the budget still counts every block streamed so a hostile
    /// gateway cannot evade the ceiling by splitting a runaway DAG across fetches.
    fn merge_and_verify(
        &mut self,
        car: &[u8],
        budget: RetrievalBudget,
    ) -> Result<(), RetrieveError> {
        let mut cursor = std::io::Cursor::new(car);
        // `validate_block_hash = true`: rs-car-sync re-hashes each block against
        // its CID as it reads. A mis-hashing block surfaces as
        // `CarDecodeError::BlockDigestMismatch`, which we map to a distinct
        // fail-closed tamper error below.
        let mut reader = CarReader::new(&mut cursor, true).map_err(map_car_error)?;

        for item in reader.by_ref() {
            let (cid, block) = item.map_err(map_car_error)?;

            self.block_count += 1;
            if self.block_count > budget.max_blocks {
                return Err(RetrieveError::BudgetExceeded(format!(
                    "block count exceeded {} blocks",
                    budget.max_blocks
                )));
            }
            self.total_bytes += block.len() as u64;
            if self.total_bytes > budget.max_bytes {
                return Err(RetrieveError::BudgetExceeded(format!(
                    "total block bytes exceeded {} bytes",
                    budget.max_bytes
                )));
            }

            self.blocks.insert(cid, block);
        }

        Ok(())
    }

    /// Fetch a verified block by CID, or the distinct [`RetrieveError::MissingBlock`]
    /// if a link pointed at a block the CAR never delivered.
    fn get(&self, cid: &Cid) -> Result<&[u8], RetrieveError> {
        self.blocks
            .get(cid)
            .map(Vec::as_slice)
            .ok_or_else(|| RetrieveError::MissingBlock {
                cid: cid.to_string(),
            })
    }
}

/// Map a [`CarDecodeError`] onto the distinct fail-closed [`RetrieveError`],
/// keeping tamper (a block that did not hash to its CID) and incompleteness (a
/// truncated stream) as SEPARATE, legible causes.
fn map_car_error(err: CarDecodeError) -> RetrieveError {
    match err {
        // A block whose bytes did not hash to its CID: tamper. This is the
        // load-bearing per-block verify failure.
        CarDecodeError::BlockDigestMismatch(m) => {
            // rs-car-sync's message embeds the offending CID; surface it as our
            // distinct tamper error so the reason is legible.
            RetrieveError::BlockHashMismatch { cid: m }
        }
        // rs-car-sync's `HashCode` wrapper is not re-exported at its crate root,
        // so the inner code cannot be destructured by type; recover it from the
        // Debug rendering so the distinct unsupported-hash refusal keeps its
        // legible code. The variant means a block CID named a hash function the
        // CAR reader does not verify: refused, never trusted.
        ref e @ CarDecodeError::UnsupportedHashCode(_) => RetrieveError::UnsupportedHash {
            code: parse_hash_code_from_debug(&format!("{e:?}")),
        },
        // A truncated block header / mid-block EOF is an incomplete CAR: the
        // client's completeness obligation was violated.
        CarDecodeError::IoError(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            RetrieveError::IncompleteCar(e.to_string())
        }
        CarDecodeError::InvalidBlockHeader(m) => RetrieveError::IncompleteCar(m),
        other => RetrieveError::MalformedDag(other.to_string()),
    }
}

/// Best-effort extraction of a multihash code from `rs-car-sync`'s
/// `UnsupportedHashCode` Debug string (its `HashCode` wrapper is not exported,
/// so the numeric code is recovered from the rendered `Code(<n>)`). Falls back
/// to `0` (unknown) if the shape ever changes; the retrieval still fails closed.
fn parse_hash_code_from_debug(debug: &str) -> u64 {
    debug
        .split_once("Code(")
        .and_then(|(_, rest)| rest.split_once(')'))
        .and_then(|(num, _)| num.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// UnixFS DAG walk: path resolution + local reassembly over verified blocks.
// ---------------------------------------------------------------------------

/// The UnixFS node types this backend understands (the `Data.Type` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnixFsType {
    Raw,
    Directory,
    File,
    Metadata,
    Symlink,
    HamtShard,
    /// A value outside the known enum (e.g. a future/unknown type).
    Other(i32),
}

impl From<i32> for UnixFsType {
    fn from(i: i32) -> Self {
        match i {
            0 => UnixFsType::Raw,
            1 => UnixFsType::Directory,
            2 => UnixFsType::File,
            3 => UnixFsType::Metadata,
            4 => UnixFsType::Symlink,
            5 => UnixFsType::HamtShard,
            other => UnixFsType::Other(other),
        }
    }
}

/// The decoded UnixFS `Data` message fields this backend needs.
///
/// Decoded with a BOUND [`quick_protobuf`] read against the canonical
/// `unixfs.proto` field tags (Type=1, Data=2, filesize=3, blocksizes=4,
/// fanout=6): a bound protobuf decode, not hand-rolled block layout.
struct UnixFsData {
    node_type: UnixFsType,
    /// The inline leaf data (for a `File`/`Raw` leaf), if present.
    data: Option<Vec<u8>>,
}

/// Decode the inner UnixFS `Data` message out of a dag-pb node's `Data` field.
fn decode_unixfs_data(data: &[u8]) -> Result<UnixFsData, RetrieveError> {
    let mut reader = BytesReader::from_bytes(data);
    let mut node_type: i32 = 0;
    let mut inline: Option<Vec<u8>> = None;
    while !reader.is_eof() {
        match reader
            .next_tag(data)
            .map_err(|e| RetrieveError::MalformedDag(e.to_string()))?
        {
            // field 1 (Type), wire type 0 (varint): tag = 8
            8 => {
                node_type = reader
                    .read_int32(data)
                    .map_err(|e| RetrieveError::MalformedDag(e.to_string()))?;
            }
            // field 2 (Data), wire type 2 (bytes): tag = 18
            18 => {
                let bytes = reader
                    .read_bytes(data)
                    .map_err(|e| RetrieveError::MalformedDag(e.to_string()))?;
                inline = Some(bytes.to_vec());
            }
            other => {
                reader
                    .read_unknown(data, other)
                    .map_err(|e| RetrieveError::MalformedDag(e.to_string()))?;
            }
        }
    }
    Ok(UnixFsData {
        node_type: UnixFsType::from(node_type),
        data: inline,
    })
}

/// A decoded UnixFS/dag-pb node: its type + inline data + named links.
struct UnixFsNode {
    data: UnixFsData,
    /// The child links, as `(name, cid)`. Names are empty for a chunked file's
    /// data links, and the entry name for a directory's children.
    links: Vec<(String, Cid)>,
}

/// Decode a verified block into a [`UnixFsNode`] according to the codec of the
/// CID it was addressed under.
///
/// A `raw` (0x55) block is a leaf whose bytes ARE the content (no dag-pb shell).
/// A `dag-pb` (0x70) block is decoded to its links + inner UnixFS `Data`. Any
/// other codec is refused as [`RetrieveError::UnsupportedCodec`] (never guessed).
fn decode_node(cid: &Cid, block: &[u8]) -> Result<UnixFsNode, RetrieveError> {
    match cid.codec() {
        RAW_CODEC => Ok(UnixFsNode {
            data: UnixFsData {
                node_type: UnixFsType::Raw,
                data: Some(block.to_vec()),
            },
            links: Vec::new(),
        }),
        DAG_PB_CODEC => {
            let node = PbNode::from_bytes(bytes::Bytes::copy_from_slice(block))
                .map_err(|e| RetrieveError::MalformedDag(e.to_string()))?;
            let links = node
                .links
                .into_iter()
                .map(|l| (l.name.unwrap_or_default(), l.cid))
                .collect();
            // The UnixFS `Data` message lives inside the dag-pb `Data` field. A
            // dag-pb node with no `Data` is treated as an empty directory shell.
            let data = match node.data {
                Some(d) => decode_unixfs_data(&d)?,
                None => UnixFsData {
                    node_type: UnixFsType::Directory,
                    data: None,
                },
            };
            Ok(UnixFsNode { data, links })
        }
        code => Err(RetrieveError::UnsupportedCodec { code }),
    }
}

/// Resolve `path` starting at `root` within the verified block `store`, returning
/// the reassembled, fully-verified bytes of the addressed resource.
///
/// A `raw` root resolves to its verified block bytes (the path must be
/// root/empty). A `dag-pb` root is walked: each non-empty path segment selects a
/// directory child by name (HAMT-sharded directories are traversed by their
/// hash-prefixed shard links); a directory reached with an empty remaining path
/// resolves its `index.html` child (served-page parity); a file node is
/// reassembled by concatenating its leaf blocks depth-first.
fn resolve_in_dag(
    store: &mut CarBlockStore,
    root: &Cid,
    path: &str,
    budget: RetrievalBudget,
    fetch_more: &mut dyn FnMut(&str) -> Result<Vec<u8>, RetrieveError>,
) -> Result<RetrievedContent, RetrieveError> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    let mut current = *root;
    let mut node = decode_node(&current, store.get(&current)?)?;

    for (idx, segment) in segments.iter().enumerate() {
        match node.data.node_type {
            UnixFsType::Directory => {
                let child =
                    find_dir_entry(&node, segment)?.ok_or_else(|| RetrieveError::PathNotFound {
                        path: path.to_string(),
                    })?;
                current = child;
                node = decode_node(&current, store.get(&current)?)?;
            }
            UnixFsType::HamtShard => {
                let child = resolve_hamt_entry(store, &node, segment)?.ok_or_else(|| {
                    RetrieveError::PathNotFound {
                        path: path.to_string(),
                    }
                })?;
                current = child;
                node = decode_node(&current, store.get(&current)?)?;
            }
            UnixFsType::File | UnixFsType::Raw => {
                // A path segment tried to descend into a file: no such resource.
                let _ = idx;
                return Err(RetrieveError::PathNotFound {
                    path: path.to_string(),
                });
            }
            UnixFsType::Symlink => {
                // Symlinks are an explicit out-of-scope follow-on: refuse rather
                // than guess a target.
                return Err(RetrieveError::UnsupportedCodec { code: DAG_PB_CODEC });
            }
            UnixFsType::Metadata | UnixFsType::Other(_) => {
                return Err(RetrieveError::MalformedDag(format!(
                    "unsupported unixfs node type resolving {path}"
                )));
            }
        }
    }

    // The path is consumed. If we landed on a directory (either a bare `ipfs://
    // <cid>` root or a `.../` directory path), resolve its `index.html` for
    // served-page parity.
    if matches!(
        node.data.node_type,
        UnixFsType::Directory | UnixFsType::HamtShard
    ) {
        let index = match node.data.node_type {
            UnixFsType::HamtShard => resolve_hamt_entry(store, &node, "index.html")?,
            _ => find_dir_entry(&node, "index.html")?,
        };
        let index_cid = index.ok_or_else(|| RetrieveError::PathNotFound {
            path: format!("{path} (directory has no index.html)"),
        })?;
        // Under per-resource scope (`dag-scope=entity`), the directory's scoped
        // CAR carried only its listing, NOT index.html's file blocks. Fetch the
        // index.html entity's OWN scoped CAR and merge its verified blocks in
        // before reassembling: the directory root resolves index.html by
        // fetching only what it needs, not the whole tree. (If index.html's
        // blocks are already present, e.g. a whole-DAG fixture, the extra fetch
        // is idempotent.)
        current = index_cid;
        if store.get(&current).is_err() {
            let index_path = join_path(path, "index.html");
            let car = fetch_more(&index_path)?;
            store.merge_and_verify(&car, budget)?;
        }
        node = decode_node(&current, store.get(&current)?)?;
    }

    // Reassemble the resolved resource (a file: its leaf blocks concatenated;
    // a raw leaf: its bytes).
    let bytes = reassemble_file(store, &current, &node, 0)?;
    Ok(RetrievedContent {
        bytes,
        codec: current.codec(),
    })
}

/// The maximum DAG depth the reassembly will descend, a guard against a cyclic
/// or pathologically deep DAG (independent of the byte/block budget).
const MAX_DAG_DEPTH: usize = 64;

/// Reassemble a file/leaf node's bytes by concatenating its leaf blocks
/// depth-first, reading every block from the VERIFIED store.
fn reassemble_file(
    store: &CarBlockStore,
    cid: &Cid,
    node: &UnixFsNode,
    depth: usize,
) -> Result<Vec<u8>, RetrieveError> {
    if depth > MAX_DAG_DEPTH {
        return Err(RetrieveError::BudgetExceeded(format!(
            "dag depth exceeded {MAX_DAG_DEPTH} resolving {cid}"
        )));
    }

    if node.links.is_empty() {
        // A leaf: its inline UnixFS `Data` (a `File`/`Raw` leaf), or the raw
        // block bytes for a `raw`-codec leaf (already captured as `data`).
        return Ok(node.data.data.clone().unwrap_or_default());
    }

    // An intermediate file node: concatenate its children's reassembled bytes in
    // link order (UnixFS file chunk order).
    let mut out = Vec::new();
    for (_name, child_cid) in &node.links {
        let child_block = store.get(child_cid)?;
        let child_node = decode_node(child_cid, child_block)?;
        let child_bytes = reassemble_file(store, child_cid, &child_node, depth + 1)?;
        out.extend_from_slice(&child_bytes);
    }
    Ok(out)
}

/// Find a plain (non-sharded) directory entry by name.
fn find_dir_entry(node: &UnixFsNode, name: &str) -> Result<Option<Cid>, RetrieveError> {
    Ok(node
        .links
        .iter()
        .find(|(link_name, _)| link_name == name)
        .map(|(_, cid)| *cid))
}

/// Resolve a directory entry by name in a HAMT-sharded directory.
///
/// A HAMT-sharded UnixFS directory splits its entries across shard nodes: each
/// link name is a 2-hex-digit shard prefix followed by the entry name (for a
/// direct entry) or just the 2-hex-digit prefix (for a link to a deeper shard
/// node). This walks the shards by matching the entry name's suffix, descending
/// into deeper shard nodes as needed, reading every shard block from the
/// VERIFIED store.
///
/// This is a pragmatic lookup that finds an entry by its FULL name across the
/// shard tree (sufficient for `index.html` + relative assets); it does not
/// re-derive the exact HAMT bucket, so it scans matching shard links, which is
/// correct though not the minimal-read path.
fn resolve_hamt_entry(
    store: &CarBlockStore,
    node: &UnixFsNode,
    name: &str,
) -> Result<Option<Cid>, RetrieveError> {
    // A shard link name is a fixed-width hex prefix (commonly 2 chars) + the
    // entry name. A pure-prefix link (name == just the hex prefix) points at a
    // deeper shard node. Detect the prefix width from the shortest link name.
    resolve_hamt_entry_inner(store, node, name, 0)
}

/// The maximum HAMT shard depth traversed, a guard against a cyclic/deep shard
/// tree.
const MAX_HAMT_DEPTH: usize = 16;

fn resolve_hamt_entry_inner(
    store: &CarBlockStore,
    node: &UnixFsNode,
    name: &str,
    depth: usize,
) -> Result<Option<Cid>, RetrieveError> {
    if depth > MAX_HAMT_DEPTH {
        return Err(RetrieveError::BudgetExceeded(format!(
            "hamt shard depth exceeded {MAX_HAMT_DEPTH} resolving {name}"
        )));
    }
    // A HAMT shard link name is a fixed-width hex prefix (the go-ipfs default
    // fanout is 256, i.e. TWO hex digits per level) followed by the entry name
    // for a direct entry, or JUST the hex prefix for a link to a deeper shard
    // node. The prefix width is the fanout's hex-digit count, which for the
    // universal fanout-256 default is 2.
    const HAMT_PREFIX_WIDTH: usize = 2;

    for (link_name, cid) in &node.links {
        if link_name.len() < HAMT_PREFIX_WIDTH {
            continue;
        }
        let (prefix, suffix) = link_name.split_at(HAMT_PREFIX_WIDTH);
        // A pure-prefix link (name is EXACTLY the hex prefix) is a deeper shard
        // node: descend and search it too.
        if suffix.is_empty() && prefix.chars().all(|c| c.is_ascii_hexdigit()) {
            let shard_block = store.get(cid)?;
            let shard_node = decode_node(cid, shard_block)?;
            if matches!(shard_node.data.node_type, UnixFsType::HamtShard) {
                if let Some(found) = resolve_hamt_entry_inner(store, &shard_node, name, depth + 1)?
                {
                    return Ok(Some(found));
                }
            }
            continue;
        }
        // A direct entry: the link name is prefix + entry name.
        if suffix == name {
            return Ok(Some(*cid));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FetchError, Response};
    use cid::multihash::Multihash;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;

    const SHA2_256: u64 = 0x12;

    // -----------------------------------------------------------------------
    // Fixture builders: synthesize a REAL dag-pb/UnixFS DAG and CAR offline.
    //
    // These bind the SAME vetted crates the production path does (ipld-dagpb to
    // encode the dag-pb node shell, quick-protobuf to encode the inner UnixFS
    // `Data`, serde_ipld_dagcbor + unsigned-varint for the CAR framing), so the
    // fixtures are byte-identical to what a trustless gateway would serve, with
    // NO live network.
    // -----------------------------------------------------------------------

    /// The CIDv1 (given codec, sha2-256) that addresses `bytes`.
    fn cid_for(codec: u64, bytes: &[u8]) -> Cid {
        let digest = Sha256::digest(bytes);
        let mh = Multihash::<64>::wrap(SHA2_256, &digest).expect("sha2-256 multihash");
        Cid::new_v1(codec, mh)
    }

    /// Encode a UnixFS `Data` message (the inner payload of a dag-pb node).
    fn unixfs_data(
        node_type: i32,
        data: Option<&[u8]>,
        blocksizes: &[u64],
        filesize: u64,
    ) -> Vec<u8> {
        use quick_protobuf::Writer;
        let mut out = Vec::new();
        {
            let mut w = Writer::new(&mut out);
            w.write_with_tag(8, |w| w.write_enum(node_type)).unwrap();
            if let Some(d) = data {
                w.write_with_tag(18, |w| w.write_bytes(d)).unwrap();
            }
            if filesize > 0 {
                w.write_with_tag(24, |w| w.write_uint64(filesize)).unwrap();
            }
            for b in blocksizes {
                w.write_with_tag(32, |w| w.write_uint64(*b)).unwrap();
            }
        }
        out
    }

    /// Encode a dag-pb node with the given inner `Data` and named links.
    fn dagpb_node(data: Option<Vec<u8>>, links: &[(String, Cid)]) -> Vec<u8> {
        use ipld_core::ipld::Ipld;
        let mut node = BTreeMap::<String, Ipld>::new();
        if let Some(d) = data {
            node.insert("Data".into(), Ipld::Bytes(d));
        }
        let link_ipld: Vec<Ipld> = links
            .iter()
            .map(|(name, cid)| {
                let mut l = BTreeMap::<String, Ipld>::new();
                l.insert("Hash".into(), Ipld::Link(*cid));
                l.insert("Name".into(), Ipld::String(name.clone()));
                l.insert("Tsize".into(), Ipld::Integer(0));
                Ipld::Map(l)
            })
            .collect();
        node.insert("Links".into(), Ipld::List(link_ipld));
        ipld_dagpb::from_ipld(&Ipld::Map(node)).expect("encode dag-pb node")
    }

    /// A raw (0x55) leaf block: bytes ARE the content, CID is over the bytes.
    fn raw_block(bytes: &[u8]) -> (Cid, Vec<u8>) {
        (cid_for(RAW_CODEC, bytes), bytes.to_vec())
    }

    /// A dag-pb UnixFS `File` leaf block holding `content` inline.
    fn file_leaf(content: &[u8]) -> (Cid, Vec<u8>) {
        let data = unixfs_data(2 /* File */, Some(content), &[], content.len() as u64);
        let block = dagpb_node(Some(data), &[]);
        (cid_for(DAG_PB_CODEC, &block), block)
    }

    /// A dag-pb UnixFS `File` node linking to leaf chunks (a chunked file).
    fn chunked_file(chunks: &[(Cid, Vec<u8>)], total: u64) -> (Cid, Vec<u8>) {
        let blocksizes: Vec<u64> = chunks.iter().map(|(_, b)| b.len() as u64).collect();
        let links: Vec<(String, Cid)> = chunks.iter().map(|(c, _)| (String::new(), *c)).collect();
        let data = unixfs_data(2 /* File */, None, &blocksizes, total);
        let block = dagpb_node(Some(data), &links);
        (cid_for(DAG_PB_CODEC, &block), block)
    }

    /// A dag-pb UnixFS `Directory` node with named entries.
    fn directory(entries: &[(String, Cid)]) -> (Cid, Vec<u8>) {
        let data = unixfs_data(1 /* Directory */, None, &[], 0);
        let block = dagpb_node(Some(data), entries);
        (cid_for(DAG_PB_CODEC, &block), block)
    }

    fn varint(n: u64) -> Vec<u8> {
        let mut buf = unsigned_varint::encode::u64_buffer();
        unsigned_varint::encode::u64(n, &mut buf).to_vec()
    }

    /// Frame `blocks` into a CARv1 byte stream rooted at `root`.
    fn build_car(root: &Cid, blocks: &[(Cid, Vec<u8>)]) -> Vec<u8> {
        #[derive(serde::Serialize)]
        struct Header {
            roots: Vec<Cid>,
            version: u64,
        }
        let header = Header {
            roots: vec![*root],
            version: 1,
        };
        let header_bytes = serde_ipld_dagcbor::to_vec(&header).expect("encode car header");
        let mut out = Vec::new();
        out.extend(varint(header_bytes.len() as u64));
        out.extend(header_bytes);
        for (cid, block) in blocks {
            let cid_bytes = cid.to_bytes();
            let len = cid_bytes.len() + block.len();
            out.extend(varint(len as u64));
            out.extend(cid_bytes);
            out.extend(block);
        }
        out
    }

    /// A [`Fetcher`] double that returns a canned CAR body for every GET,
    /// isolated from the live network. It records the last requested URL so the
    /// test can assert the `?format=car` request shape.
    struct CannedCarFetcher {
        car: Vec<u8>,
        status: u16,
        last_url: std::cell::RefCell<String>,
    }

    impl CannedCarFetcher {
        fn new(car: Vec<u8>) -> Self {
            Self {
                car,
                status: 200,
                last_url: std::cell::RefCell::new(String::new()),
            }
        }
    }

    impl Fetcher for CannedCarFetcher {
        fn fetch(&self, url: &str) -> Result<Response, FetchError> {
            *self.last_url.borrow_mut() = url.to_string();
            Ok(Response {
                status: self.status,
                content_type: Some("application/vnd.ipld.car".into()),
                body: self.car.clone(),
                final_url: url.to_string(),
            })
        }
    }

    fn retriever(car: Vec<u8>) -> TrustlessGatewayCarRetriever<CannedCarFetcher> {
        TrustlessGatewayCarRetriever::with_gateway(
            CannedCarFetcher::new(car),
            "http://gateway.test",
        )
    }

    /// A [`Fetcher`] double that serves a DISTINCT canned CAR per requested URL
    /// PATH (the `/ipfs/<cid>[/<path>]` portion, query stripped), and records
    /// EVERY requested URL in order. This is how a scoped-CAR test proves the
    /// backend requests only the blocks for the specific resource: each
    /// `dag-scope=entity` fetch of `<cid>/<path>` gets back a CAR containing
    /// ONLY that path's traversal + terminal-entity blocks, so a resource whose
    /// scoped CAR omits an unrelated resource's blocks still resolves. Isolated
    /// from the live network.
    struct PathScopedCarFetcher {
        /// Keyed by the URL path (`/ipfs/<cid>` or `/ipfs/<cid>/<sub>`), the CAR
        /// a `dag-scope=entity` request for that path returns.
        cars: HashMap<String, Vec<u8>>,
        /// Every requested URL, in call order (query string included).
        requested: std::cell::RefCell<Vec<String>>,
    }

    impl PathScopedCarFetcher {
        fn new(cars: HashMap<String, Vec<u8>>) -> Self {
            Self {
                cars,
                requested: std::cell::RefCell::new(Vec::new()),
            }
        }

        /// The path portion of a `<gateway>/ipfs/...` URL, query stripped.
        fn path_of(url: &str) -> String {
            let no_query = url.split('?').next().unwrap_or(url);
            match no_query.find("/ipfs/") {
                Some(i) => no_query[i..].to_string(),
                None => no_query.to_string(),
            }
        }
    }

    impl Fetcher for PathScopedCarFetcher {
        fn fetch(&self, url: &str) -> Result<Response, FetchError> {
            self.requested.borrow_mut().push(url.to_string());
            let key = Self::path_of(url);
            let body = self.cars.get(&key).cloned().unwrap_or_default();
            // A path the gateway has no scoped CAR for is a 404 (root block not
            // in this scoped response), exactly as a trustless gateway signals a
            // missing terminus.
            let status = if self.cars.contains_key(&key) {
                200
            } else {
                404
            };
            Ok(Response {
                status,
                content_type: Some("application/vnd.ipld.car".into()),
                body,
                final_url: url.to_string(),
            })
        }
    }

    // -----------------------------------------------------------------------
    // Acceptance: the seam + default CAR backend.
    // -----------------------------------------------------------------------

    #[test]
    fn a_single_raw_block_still_verifies_and_returns_its_bytes() {
        // No regression: a single-block raw/leaf sha2-256 CID retrieves its
        // verified bytes through the new seam, at parity with the old path.
        let content = b"<!doctype html><title>raw</title><h1>content-addressed</h1>";
        let (cid, block) = raw_block(content);
        let car = build_car(&cid, &[(cid, block)]);

        let got = retriever(car)
            .retrieve(&cid.to_string(), "/")
            .expect("a raw block verifies and returns its bytes");
        assert_eq!(got.bytes, content);
        assert_eq!(got.codec, RAW_CODEC);
    }

    #[test]
    fn a_raw_block_that_does_not_hash_to_its_cid_is_a_hard_tamper_failure_never_served() {
        // The load-bearing gate: a raw CID whose CAR block does NOT hash to it is
        // rejected as a distinct tamper failure at CAR-parse time, never served.
        let honest = b"the content this cid actually names";
        let cid = cid_for(RAW_CODEC, honest);
        // Frame TAMPERED bytes under the honest CID.
        let car = build_car(
            &cid,
            &[(cid, b"tampered bytes not matching the cid".to_vec())],
        );

        let err = retriever(car)
            .retrieve(&cid.to_string(), "/")
            .expect_err("tampered raw block must hard-fail");
        assert!(
            matches!(err, RetrieveError::BlockHashMismatch { .. }),
            "expected a distinct tamper failure, got: {err:?}"
        );
    }

    #[test]
    fn a_real_multi_block_directory_site_renders_index_and_sub_resources_all_verified() {
        // Acceptance: a real multi-block UnixFS DIRECTORY (index.html + a css
        // sub-resource, the css itself a CHUNKED file) resolves end to end, every
        // block hash-checked, reassembled locally.
        let index_html =
            b"<!doctype html><title>site</title><link rel=stylesheet href=style.css><h1>hi</h1>";
        let (index_cid, index_block) = file_leaf(index_html);

        // A chunked css file: two raw leaf chunks under a dag-pb File node.
        let css_a = b"body { color: red; }\n";
        let css_b = b".hero { font-size: 2rem; }\n";
        let (chunk_a_cid, chunk_a) = raw_block(css_a);
        let (chunk_b_cid, chunk_b) = raw_block(css_b);
        let (css_cid, css_block) = chunked_file(
            &[
                (chunk_a_cid, chunk_a.clone()),
                (chunk_b_cid, chunk_b.clone()),
            ],
            (css_a.len() + css_b.len()) as u64,
        );

        let (dir_cid, dir_block) = directory(&[
            ("index.html".into(), index_cid),
            ("style.css".into(), css_cid),
        ]);

        let blocks = vec![
            (dir_cid, dir_block),
            (index_cid, index_block),
            (css_cid, css_block),
            (chunk_a_cid, chunk_a),
            (chunk_b_cid, chunk_b),
        ];
        let car = build_car(&dir_cid, &blocks);
        let r = retriever(car);

        // The directory root resolves to index.html (served-page parity).
        let index = r
            .retrieve(&dir_cid.to_string(), "/")
            .expect("directory root resolves index.html");
        assert_eq!(index.bytes, index_html);

        // A bare cid (empty path) resolves index.html too.
        let bare = retriever(build_car(
            &dir_cid,
            &[
                (
                    dir_cid,
                    directory(&[
                        ("index.html".into(), index_cid),
                        ("style.css".into(), css_cid),
                    ])
                    .1,
                ),
                file_leaf(index_html),
                {
                    let (cc, cb) = chunked_file(
                        &[
                            (cid_for(RAW_CODEC, css_a), css_a.to_vec()),
                            (cid_for(RAW_CODEC, css_b), css_b.to_vec()),
                        ],
                        (css_a.len() + css_b.len()) as u64,
                    );
                    (cc, cb)
                },
                raw_block(css_a),
                raw_block(css_b),
            ],
        ))
        .retrieve(&dir_cid.to_string(), "")
        .expect("bare cid resolves index.html");
        assert_eq!(bare.bytes, index_html);

        // The relative sub-resource path resolves the chunked css, reassembled.
        let css = r
            .retrieve(&dir_cid.to_string(), "/style.css")
            .expect("sub-resource resolves into the verified dag");
        let mut expected = Vec::new();
        expected.extend_from_slice(css_a);
        expected.extend_from_slice(css_b);
        assert_eq!(css.bytes, expected);
    }

    #[test]
    fn the_request_is_a_format_car_get() {
        // The backend fetches the DAG as a CAR (?format=car), the trustless
        // gateway contract, not a reassembled-bytes GET.
        let (cid, block) = raw_block(b"x");
        let fetcher = CannedCarFetcher::new(build_car(&cid, &[(cid, block)]));
        let r = TrustlessGatewayCarRetriever::with_gateway(fetcher, "http://gw.test");
        let _ = r.retrieve(&cid.to_string(), "/");
        let url = r.fetcher.last_url.borrow().clone();
        assert!(
            url.contains("format=car"),
            "expected a ?format=car GET, got: {url}"
        );
        assert!(url.contains(&cid.to_string()));
    }

    #[test]
    fn the_request_scopes_to_the_entity_not_the_whole_dag() {
        // The scope fix: a resource request fetches the SPECIFIC entity's blocks
        // (`dag-scope=entity`), NOT the whole DAG (`dag-scope=all`). Requesting
        // the whole DAG per resource is what made real sites do N full-site
        // downloads to render one page.
        let (cid, block) = raw_block(b"x");
        let fetcher = CannedCarFetcher::new(build_car(&cid, &[(cid, block)]));
        let r = TrustlessGatewayCarRetriever::with_gateway(fetcher, "http://gw.test");
        let _ = r.retrieve(&cid.to_string(), "/");
        let url = r.fetcher.last_url.borrow().clone();
        assert!(
            url.contains("dag-scope=entity"),
            "expected a dag-scope=entity GET, got: {url}"
        );
        assert!(
            !url.contains("dag-scope=all"),
            "must NOT fetch the whole DAG per resource, got: {url}"
        );
    }

    #[test]
    fn a_sub_resource_request_puts_its_path_in_the_url_scoped_to_that_entity() {
        // A resource at `ipfs://<cid>/style.css` is fetched as
        // `/ipfs/<cid>/style.css?format=car&dag-scope=entity`: the PATH goes in
        // the URL so the gateway returns only the blocks to traverse that path
        // plus the terminal entity, per the Trustless Gateway spec.
        let index_html = b"<!doctype html><title>site</title>";
        // index.html is a sibling in the directory listing but is NOT in the
        // css resource's scoped CAR, proving per-entity scope.
        let (index_cid, _index_block) = file_leaf(index_html);
        let css = b"body{color:red}";
        let (css_cid, css_block) = file_leaf(css);
        let (dir_cid, dir_block) = directory(&[
            ("index.html".into(), index_cid),
            ("style.css".into(), css_cid),
        ]);

        // The scoped CAR for `<dir>/style.css`: the dir node (to traverse the
        // path segment) + the css entity, and NOTHING of index.html.
        let mut cars = HashMap::new();
        cars.insert(
            format!("/ipfs/{dir_cid}/style.css"),
            build_car(
                &dir_cid,
                &[(dir_cid, dir_block.clone()), (css_cid, css_block.clone())],
            ),
        );
        let r = TrustlessGatewayCarRetriever::with_gateway(
            PathScopedCarFetcher::new(cars),
            "http://gw.test",
        );

        let got = r
            .retrieve(&dir_cid.to_string(), "/style.css")
            .expect("a scoped sub-resource CAR resolves that resource");
        assert_eq!(got.bytes, css);

        let requested = r.fetcher.requested.borrow().clone();
        assert_eq!(
            requested.len(),
            1,
            "exactly one scoped fetch: {requested:?}"
        );
        let url = &requested[0];
        assert!(
            url.contains(&format!("/ipfs/{dir_cid}/style.css")),
            "path must be in the url, got: {url}"
        );
        assert!(url.contains("dag-scope=entity"), "got: {url}");
    }

    #[test]
    fn a_directory_root_resolves_index_html_by_fetching_only_what_it_needs() {
        // The directory root resolves to index.html by fetching only what it
        // needs, NOT the entire tree. A `dag-scope=entity` fetch of the directory
        // returns just the directory listing; werust then fetches
        // `<cid>/index.html` (also entity-scoped) for the index entity's blocks.
        // A large SIBLING asset is present in the tree but must NEVER be fetched
        // to render the root.
        let index_html = b"<!doctype html><title>root</title><h1>hi</h1>";
        let (index_cid, index_block) = file_leaf(index_html);
        let heavy = vec![0u8; 4096];
        let (heavy_cid, _heavy_block) = file_leaf(&heavy);
        // dag-pb requires links in sorted name order (`heavy.bin` < `index.html`).
        let (dir_cid, dir_block) = directory(&[
            ("heavy.bin".into(), heavy_cid),
            ("index.html".into(), index_cid),
        ]);

        let mut cars = HashMap::new();
        // The directory entity: just the directory listing block.
        cars.insert(
            format!("/ipfs/{dir_cid}"),
            build_car(&dir_cid, &[(dir_cid, dir_block.clone())]),
        );
        // The index.html entity: dir traversal block + the index leaf. The heavy
        // sibling is deliberately absent from BOTH scoped CARs.
        cars.insert(
            format!("/ipfs/{dir_cid}/index.html"),
            build_car(
                &dir_cid,
                &[
                    (dir_cid, dir_block.clone()),
                    (index_cid, index_block.clone()),
                ],
            ),
        );
        // `heavy.bin` is present in the tree but appears in NO scoped CAR.

        let r = TrustlessGatewayCarRetriever::with_gateway(
            PathScopedCarFetcher::new(cars),
            "http://gw.test",
        );
        let got = r
            .retrieve(&dir_cid.to_string(), "/")
            .expect("directory root resolves index.html from scoped CARs");
        assert_eq!(got.bytes, index_html);

        // It fetched the directory then index.html, and NEVER the heavy sibling
        // (no whole-tree fetch).
        let requested = r.fetcher.requested.borrow().clone();
        assert!(
            requested.iter().any(|u| u.contains("/index.html")),
            "expected an index.html scoped fetch, got: {requested:?}"
        );
        assert!(
            requested.iter().all(|u| !u.contains("heavy.bin")),
            "must not fetch the heavy sibling to render the root, got: {requested:?}"
        );
        assert!(
            requested.iter().all(|u| !u.contains("dag-scope=all")),
            "must not fall back to whole-DAG scope, got: {requested:?}"
        );
    }

    #[test]
    fn a_scoped_car_missing_the_resource_block_fails_closed() {
        // Verification unchanged under scoping: if a scoped CAR omits a block the
        // resource needs (a truncated/incomplete scoped response), the retrieval
        // fails closed as MissingBlock, never a partial render.
        let a = b"first-";
        let b = b"second";
        let (ca, ba) = raw_block(a);
        let (cb, _bb) = raw_block(b);
        let (file_cid, file_block) = chunked_file(
            &[(ca, ba.clone()), (cb, b.to_vec())],
            (a.len() + b.len()) as u64,
        );
        // The scoped CAR delivers the file node + first chunk but DROPS the
        // second chunk (an incomplete entity response).
        let mut cars = HashMap::new();
        cars.insert(
            format!("/ipfs/{file_cid}"),
            build_car(&file_cid, &[(file_cid, file_block), (ca, ba)]),
        );
        let r = TrustlessGatewayCarRetriever::with_gateway(
            PathScopedCarFetcher::new(cars),
            "http://gw.test",
        );
        let err = r
            .retrieve(&file_cid.to_string(), "/")
            .expect_err("an incomplete scoped entity must fail closed");
        assert!(
            matches!(err, RetrieveError::MissingBlock { .. }),
            "expected a distinct missing-block failure, got: {err:?}"
        );
    }

    #[test]
    fn a_mis_hashing_block_inside_the_dag_is_a_distinct_tamper_failure() {
        // A directory whose index.html leaf block has been tampered: the block
        // does not hash to its link CID, so the walk fails closed as a tamper.
        let index_html = b"<h1>honest</h1>";
        let index_cid = file_leaf(index_html).0;
        let tampered_block = dagpb_node(
            Some(unixfs_data(2, Some(b"<h1>tampered</h1>"), &[], 17)),
            &[],
        );
        let (dir_cid, dir_block) = directory(&[("index.html".into(), index_cid)]);
        // Frame the TAMPERED block under the honest index CID.
        let car = build_car(
            &dir_cid,
            &[(dir_cid, dir_block), (index_cid, tampered_block)],
        );

        let err = retriever(car)
            .retrieve(&dir_cid.to_string(), "/")
            .expect_err("a mis-hashing dag block must fail closed");
        assert!(
            matches!(err, RetrieveError::BlockHashMismatch { .. }),
            "expected a distinct tamper failure, got: {err:?}"
        );
    }

    #[test]
    fn a_missing_linked_block_is_a_distinct_failure() {
        // A directory links index.html, but the CAR never delivers that block:
        // the DAG cannot reassemble, a distinct MissingBlock failure.
        let index_cid = file_leaf(b"<h1>gone</h1>").0;
        let (dir_cid, dir_block) = directory(&[("index.html".into(), index_cid)]);
        // Only the directory block is in the CAR; the index leaf is absent.
        let car = build_car(&dir_cid, &[(dir_cid, dir_block)]);

        let err = retriever(car)
            .retrieve(&dir_cid.to_string(), "/")
            .expect_err("a missing linked block must fail closed");
        assert!(
            matches!(err, RetrieveError::MissingBlock { .. }),
            "expected a distinct missing-block failure, got: {err:?}"
        );
    }

    #[test]
    fn an_incomplete_truncated_car_is_a_distinct_failure() {
        // A CAR truncated mid-block violates the client's completeness
        // obligation: a distinct IncompleteCar failure, never a partial pass.
        let content = b"a reasonably sized raw leaf block for truncation";
        let (cid, block) = raw_block(content);
        let mut car = build_car(&cid, &[(cid, block)]);
        // Chop the tail so the last block is truncated mid-stream.
        car.truncate(car.len() - 8);

        let err = retriever(car)
            .retrieve(&cid.to_string(), "/")
            .expect_err("a truncated car must fail closed");
        assert!(
            matches!(err, RetrieveError::IncompleteCar(_)),
            "expected a distinct incomplete-car failure, got: {err:?}"
        );
    }

    #[test]
    fn the_byte_budget_refuses_a_runaway_dag() {
        // A CAR larger than the byte budget is refused before reassembly.
        let big = vec![0u8; 4096];
        let (cid, block) = raw_block(&big);
        let car = build_car(&cid, &[(cid, block)]);
        let r = TrustlessGatewayCarRetriever::with_gateway(
            CannedCarFetcher::new(car),
            "http://gw.test",
        )
        .with_budget(RetrievalBudget::default().with_max_bytes(1024));

        let err = r
            .retrieve(&cid.to_string(), "/")
            .expect_err("a car over the byte budget must be refused");
        assert!(
            matches!(err, RetrieveError::BudgetExceeded(_)),
            "expected a distinct budget failure, got: {err:?}"
        );
    }

    #[test]
    fn the_block_budget_refuses_a_fan_out_dag() {
        // Many tiny blocks under the byte budget still trip the block-count
        // ceiling: a fan-out attack is refused.
        let chunks: Vec<(Cid, Vec<u8>)> = (0..10u8).map(|i| raw_block(&[i])).collect();
        let total: u64 = chunks.iter().map(|(_, b)| b.len() as u64).sum();
        let (file_cid, file_block) = chunked_file(&chunks, total);
        let mut blocks = vec![(file_cid, file_block)];
        blocks.extend(chunks.iter().cloned());
        let car = build_car(&file_cid, &blocks);

        let r = TrustlessGatewayCarRetriever::with_gateway(
            CannedCarFetcher::new(car),
            "http://gw.test",
        )
        .with_budget(RetrievalBudget::default().with_max_blocks(3));
        let err = r
            .retrieve(&file_cid.to_string(), "/")
            .expect_err("a car over the block budget must be refused");
        assert!(
            matches!(err, RetrieveError::BudgetExceeded(_)),
            "expected a distinct budget failure, got: {err:?}"
        );
    }

    #[test]
    fn an_unresolved_path_is_a_distinct_failure() {
        // A path to a non-existent directory entry fails closed as PathNotFound,
        // never a silent wrong render.
        let index_cid = file_leaf(b"<h1>hi</h1>").0;
        let (dir_cid, dir_block) = directory(&[("index.html".into(), index_cid)]);
        let car = build_car(&dir_cid, &[(dir_cid, dir_block), file_leaf(b"<h1>hi</h1>")]);

        let err = retriever(car)
            .retrieve(&dir_cid.to_string(), "/does-not-exist.js")
            .expect_err("an unresolved path must fail closed");
        assert!(
            matches!(err, RetrieveError::PathNotFound { .. }),
            "expected a distinct path-not-found failure, got: {err:?}"
        );
    }

    #[test]
    fn a_directory_without_index_html_fails_closed() {
        // A directory reached at its root with no index.html cannot be rendered
        // as a page: a distinct PathNotFound, never a guessed listing.
        let only_cid = file_leaf(b"data").0;
        let (dir_cid, dir_block) = directory(&[("readme.txt".into(), only_cid)]);
        let car = build_car(&dir_cid, &[(dir_cid, dir_block), file_leaf(b"data")]);

        let err = retriever(car)
            .retrieve(&dir_cid.to_string(), "/")
            .expect_err("a directory with no index.html must fail closed");
        assert!(
            matches!(err, RetrieveError::PathNotFound { .. }),
            "expected a distinct path-not-found failure, got: {err:?}"
        );
    }

    #[test]
    fn an_invalid_cid_is_rejected_before_fetching() {
        let err = retriever(Vec::new())
            .retrieve("not-a-valid-cid", "/")
            .expect_err("a malformed cid is rejected");
        assert!(matches!(err, RetrieveError::InvalidCid(_)));
    }

    #[test]
    fn a_gateway_transport_failure_surfaces_as_a_source_error() {
        struct FailingFetcher;
        impl Fetcher for FailingFetcher {
            fn fetch(&self, _url: &str) -> Result<Response, FetchError> {
                Err(FetchError::Transport("connection refused".into()))
            }
        }
        let (cid, _) = raw_block(b"x");
        let r = TrustlessGatewayCarRetriever::with_gateway(FailingFetcher, "http://gw.test");
        let err = r
            .retrieve(&cid.to_string(), "/")
            .expect_err("a transport failure surfaces");
        assert!(matches!(
            err,
            RetrieveError::Source(FetchError::Transport(_))
        ));
    }

    /// A HAMT-sharded directory node with the given shard links (name = hex
    /// prefix + entry name for a direct entry, or just the hex prefix for a
    /// deeper shard link).
    fn hamt_shard(entries: &[(String, Cid)]) -> (Cid, Vec<u8>) {
        // fanout 256 (2 hex digits per level), the go-ipfs default.
        let mut data = Vec::new();
        {
            use quick_protobuf::Writer;
            let mut w = Writer::new(&mut data);
            w.write_with_tag(8, |w| w.write_enum(5 /* HAMTShard */))
                .unwrap();
            w.write_with_tag(40, |w| w.write_uint64(1)).unwrap(); // hashType
            w.write_with_tag(48, |w| w.write_uint64(256)).unwrap(); // fanout
        }
        let block = dagpb_node(Some(data), entries);
        (cid_for(DAG_PB_CODEC, &block), block)
    }

    #[test]
    fn a_hamt_sharded_directory_resolves_index_and_entries() {
        // Acceptance (UnixFS scope IN): a HAMT-sharded directory resolves its
        // index.html and a named entry through the shard tree, every block
        // verified. One direct entry at the top shard, one entry behind a deeper
        // shard node.
        let index_html = b"<!doctype html><title>hamt</title><h1>sharded</h1>";
        let (index_cid, index_block) = file_leaf(index_html);
        let asset = b"console.log('app');";
        let (asset_cid, asset_block) = file_leaf(asset);

        // A deeper shard node holding the asset entry under prefix "AB".
        let (deep_cid, deep_block) = hamt_shard(&[("ABapp.js".into(), asset_cid)]);
        // The top shard: index.html as a direct entry (prefix "00"), and a
        // pure-prefix link "CD" to the deeper shard node.
        let (top_cid, top_block) =
            hamt_shard(&[("00index.html".into(), index_cid), ("CD".into(), deep_cid)]);

        let car = build_car(
            &top_cid,
            &[
                (top_cid, top_block),
                (deep_cid, deep_block),
                (index_cid, index_block),
                (asset_cid, asset_block),
            ],
        );
        let r = retriever(car);

        // The sharded directory root resolves index.html.
        let index = r
            .retrieve(&top_cid.to_string(), "/")
            .expect("hamt directory root resolves index.html");
        assert_eq!(index.bytes, index_html);

        // A named entry behind a deeper shard resolves too.
        let js = r
            .retrieve(&top_cid.to_string(), "/app.js")
            .expect("hamt entry behind a deeper shard resolves");
        assert_eq!(js.bytes, asset);
    }

    #[test]
    fn a_chunked_file_reassembles_in_link_order() {
        // A multi-block file directly (not under a directory) reassembles its
        // leaf chunks in link order, every block verified.
        let a = b"first-";
        let b = b"second";
        let (ca, ba) = raw_block(a);
        let (cb, bb) = raw_block(b);
        let (file_cid, file_block) = chunked_file(
            &[(ca, ba.clone()), (cb, bb.clone())],
            (a.len() + b.len()) as u64,
        );
        let car = build_car(&file_cid, &[(file_cid, file_block), (ca, ba), (cb, bb)]);

        let got = retriever(car)
            .retrieve(&file_cid.to_string(), "/")
            .expect("a chunked file reassembles");
        assert_eq!(got.bytes, b"first-second");
    }
}
