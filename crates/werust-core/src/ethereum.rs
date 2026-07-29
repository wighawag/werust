//! The `EthereumProvider` seam: werust's INTERNAL trusted-read boundary for
//! Ethereum, with its one Phase-1 backend, [`RpcProvider`] (a plain JSON-RPC
//! `eth_call` skeleton over a configured HTTP endpoint).
//!
//! This is the trust boundary for Ethereum READS that ENS resolution calls
//! (`CONTEXT.md`, spec `ens-to-ipfs-resolution-phase1-rpc-skeleton`): "given a
//! contract address + calldata (+ a block tag), return the `eth_call` return
//! bytes", plus only the couple of reads ENS needs. It is modelled EXACTLY like
//! the sibling seams already in the tree — [`Fetcher`](fetcher::Fetcher), the
//! [`ContentSource`](fetcher::ContentSource) trait, and the `Renderer` seam —
//! where the interface is the abstraction and the concrete trust level is a
//! SWAPPABLE BACKEND. Phase 2 swaps a trustless async light client (Helios)
//! behind this same seam; Phase 1 ships a TRUSTED [`RpcProvider`] so the whole
//! `name -> CID -> verified render` path is real and honestly labelled while the
//! light client is built.
//!
//! # NOT the page-facing EIP-1193 provider
//!
//! This is DISTINCT from the glossary's `EIP-1193 provider` (`CONTEXT.md`, the
//! [`provider`](crate::provider) module): that one is the Ethereum provider
//! INJECTED INTO PAGES via the `Renderer` script bridge for a page's dapp JS.
//! THIS seam is werust's OWN internal trusted-read path that ENS resolution
//! calls directly. They do not share a type or a trust story; do not conflate
//! them.
//!
//! # Trust honesty
//!
//! [`RpcProvider`] is TRUSTED: the `eth_call` result is taken on the configured
//! RPC's word. An ENS-resolved page is therefore labelled "content-verified,
//! name via TRUSTED RPC" (never "verified") upstream — the seam exists so Phase
//! 2's light client removes that trust as a pure backend swap, not a rewrite.
//!
//! # Async accommodation (load-bearing for Phase 2)
//!
//! Phase 1's call is plain synchronous, but the trait is shaped so a later
//! ASYNC (tokio) backend fits behind it, bridged async->sync AT the seam
//! boundary. Concretely: [`EthereumProvider::eth_call`] takes owned/borrowed
//! inputs and returns an OWNED [`Vec<u8>`] (or a typed [`ProviderError`]) by
//! value — it does NOT hand back a borrowed reference tied to a synchronous call
//! stack that an async bridge (which would `block_on` an internal runtime and
//! then return owned bytes) could not satisfy. A Phase-2 backend can run its
//! async client to completion inside the method and return the owned result;
//! nothing in the signature structurally blocks that.
//!
//! # Transport is bound, not hand-rolled, and lives behind the seam
//!
//! A JSON-RPC `eth_call` is an HTTP POST carrying a JSON request body — the
//! existing [`Fetcher`](fetcher::Fetcher) seam is GET-ONLY (`fetch(&self, url)`,
//! no method/body; `HttpFetcher` does `agent.get(url).call()`) and cannot send
//! that body. Rather than widen the `Fetcher` seam's surface for this single
//! consumer, [`RpcProvider`] binds its OWN minimal synchronous JSON-RPC HTTP
//! transport behind a small [`RpcTransport`] seam (default backend
//! [`UreqRpcTransport`], which binds `ureq`'s POST exactly as `HttpFetcher`
//! binds its GET — bound, not hand-rolled, and off any async runtime). The
//! transport double-purposes the seam so tests can assert the OUTGOING request
//! actually carried the `eth_call` JSON body (method + params), not merely that
//! some bytes came back. See the transport-path decision in
//! `docs/spikes/ethereum-provider-seam-and-trusted-rpc-backend/`.
//!
//! # Failures are typed seam errors, never panics, never silent empties
//!
//! Every failure — a transport/connection error, a non-2xx HTTP status, a
//! JSON-RPC error object in the response, or an unparseable/`0x`-malformed result
//! — surfaces as a typed [`ProviderError`]; the seam never panics and never
//! returns empty bytes as if the call succeeded.

use std::fmt;
use std::time::Duration;

use serde_json::{json, Value};

/// The default trusted JSON-RPC endpoint [`RpcProvider`] calls when constructed
/// with [`RpcProvider::new`].
///
/// This is a TRUSTED origin: the `eth_call` result is taken on this endpoint's
/// word (Phase 1 has no light client — the trustless backend is the Phase-2
/// swap). `https://mainnet.infura.io/v3/9aa3d95b3bc440fa88ea12eaa4456161` is a public, keyless mainnet RPC used as the
/// labelled default (the previous default, `ethereum-rpc.publicnode.com`, then `https://1rpc.io/eth`, were
/// observed DNS-blocked by home routers and TLS-broken behind captive portals);
/// the durable endpoint policy (which RPC, or a local node) is not this task's
/// concern and is overridden by constructing with
/// [`RpcProvider::with_endpoint`], and the `WERUST_RPC_URL` environment
/// variable (when set and non-empty) takes precedence over this default at
/// session construction — see [`rpc_endpoint`]. Mirrors the
/// `GatewayContentSource::new` / `with_gateway` pair, so there is NO config
/// subsystem to chase.
pub const DEFAULT_RPC_ENDPOINT: &str =
    "https://mainnet.infura.io/v3/9aa3d95b3bc440fa88ea12eaa4456161";

/// The environment variable that overrides [`DEFAULT_RPC_ENDPOINT`] for a
/// session: the opt-in lever for pointing ENS resolution at a private endpoint
/// (e.g. a local node) without committing its URL. Fits the `WERUST_*` lever
/// namespace established by `WERUST_VERSION` (build.rs) and
/// `WERUST_SETTINGS_DIR` (retrieval.rs).
const RPC_URL_ENV: &str = "WERUST_RPC_URL";

/// The endpoint [`RpcProvider::new`] points at: the `WERUST_RPC_URL` env lever
/// when set and non-empty (whitespace-trimmed), else [`DEFAULT_RPC_ENDPOINT`].
///
/// Returns an owned `String` because `std::env::var` owns its data;
/// [`RpcProvider::with_endpoint`] takes `&str`, so this composes without a
/// leak. The env is read ONCE per session — `RpcProvider::new` is called once
/// at session construction (the same one-shot boundary at which
/// `WERUST_VERSION` is resolved at build time), never per request — so a live
/// change requires a relaunch, the same constraint as the existing in-app
/// settings. The precedence decision itself is pure and env-free
/// ([`resolve_rpc_endpoint`]) so tests exercise it WITHOUT mutating the
/// process-global env (which would race under parallel `cargo test`).
fn rpc_endpoint() -> String {
    resolve_rpc_endpoint(std::env::var(RPC_URL_ENV).ok().as_deref())
}

/// The pure precedence core behind [`rpc_endpoint`]: a non-empty
/// (whitespace-trimmed) env value wins, trimmed; an unset, empty, or
/// whitespace-only value falls back to [`DEFAULT_RPC_ENDPOINT`]. The
/// empty-falls-back rule is load-bearing for CI: the release workflow always
/// exports `WERUST_RPC_URL` from the OPTIONAL repository secret, which
/// substitutes an empty string when unconfigured.
fn resolve_rpc_endpoint(env_value: Option<&str>) -> String {
    match env_value {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => DEFAULT_RPC_ENDPOINT.to_string(),
    }
}

/// The default block tag an [`eth_call`](EthereumProvider::eth_call) uses when
/// the caller passes [`BlockTag::Latest`].
///
/// `latest` is the standard "the most recent block" tag for a read; ENS
/// resolution reads current state, so `latest` is the sensible default. The tag
/// is a first-class argument (see [`BlockTag`]) rather than hard-coded, so a
/// caller that needs a pinned block can pass one — but the common ENS read path
/// asks for `latest`.
const LATEST_BLOCK_TAG: &str = "latest";

/// The connect timeout the default RPC transport applies (mirrors the
/// [`Fetcher`](fetcher::Fetcher) seam's default) so a call to an unreachable or
/// silently-dropping endpoint fails promptly as a [`ProviderError`] instead of
/// hanging on the OS connect budget.
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// The whole-request timeout the default RPC transport applies (connect + TLS +
/// read), a safe upper bound so a call cannot hang indefinitely.
const DEFAULT_GLOBAL_TIMEOUT: Duration = Duration::from_secs(30);

/// The block a read is evaluated against.
///
/// A `eth_call` names the block it reads state at. ENS resolution reads current
/// state, so [`Latest`](BlockTag::Latest) (the [`LATEST_BLOCK_TAG`]) is the
/// common case; [`Number`](BlockTag::Number) pins a specific block for a
/// reproducible read. Kept as a small enum rather than a bare string so a caller
/// cannot pass an arbitrary/mistyped tag, while leaving room to grow (e.g. a
/// `Finalized` tag) without a breaking signature change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockTag {
    /// The most recent block (`"latest"`), the default ENS read target.
    Latest,
    /// A specific block height, serialized as the `0x`-prefixed hex quantity the
    /// JSON-RPC spec requires.
    Number(u64),
}

impl BlockTag {
    /// The JSON-RPC block-parameter string for this tag (`"latest"` or a
    /// `0x`-prefixed hex quantity), as it appears as the second `eth_call`
    /// param.
    fn as_rpc_param(self) -> String {
        match self {
            BlockTag::Latest => LATEST_BLOCK_TAG.to_string(),
            BlockTag::Number(n) => format!("{n:#x}"),
        }
    }
}

/// A typed failure from an [`EthereumProvider`] read.
///
/// Every way an `eth_call` can fail lands in one of these variants so a caller
/// pattern-matches instead of catching a panic or interpreting an empty result.
/// The load-bearing distinction is [`Rpc`](ProviderError::Rpc) — the endpoint
/// answered with a JSON-RPC error OBJECT (a valid response that reports the call
/// failed, e.g. reverted / bad params), carrying the RPC's own numeric code and
/// message — versus a [`Transport`](ProviderError::Transport) failure (the
/// request never got a well-formed answer) and a
/// [`Decode`](ProviderError::Decode) failure (the answer was not shaped like a
/// JSON-RPC `eth_call` result). None of them ever yields bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// The request was malformed before it could be sent (e.g. an unusable
    /// endpoint URL, or calldata that is not `0x`-hex). Rejected without a
    /// network round-trip.
    InvalidRequest(String),
    /// The connection failed, the transport errored, or the endpoint answered a
    /// non-2xx HTTP status. The call never got a well-formed JSON-RPC answer.
    Transport(String),
    /// The endpoint answered with a JSON-RPC error OBJECT: a syntactically valid
    /// response reporting the call itself failed (a revert, bad params, an
    /// unsupported method, ...). Carries the RPC's own `code`/`message` so the
    /// caller sees the endpoint's reason rather than an opaque failure.
    Rpc {
        /// The JSON-RPC error code the endpoint returned.
        code: i64,
        /// The human-readable message the endpoint returned.
        message: String,
    },
    /// The response was reachable and 2xx but not a well-formed JSON-RPC
    /// `eth_call` result: not JSON, missing/duplicated `result`+`error`, or a
    /// `result` that is not a `0x`-prefixed hex byte string. Refused rather than
    /// guessed — an unparseable result is never returned as empty bytes.
    Decode(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::InvalidRequest(m) => write!(f, "invalid eth_call request: {m}"),
            ProviderError::Transport(m) => write!(f, "rpc transport error: {m}"),
            ProviderError::Rpc { code, message } => {
                write!(f, "rpc error (code {code}): {message}")
            }
            ProviderError::Decode(m) => write!(f, "malformed rpc result: {m}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// A single read-only Ethereum contract call: the `to` address plus the
/// ABI-encoded `data` (calldata), evaluated at a [`BlockTag`].
///
/// This is the only shape ENS resolution needs (registry `resolver(node)`,
/// resolver `contenthash(node)`, ...): an address + calldata + block, returning
/// ABI-encoded return bytes. Writes/transactions/subscriptions are deliberately
/// out of scope (spec Out of Scope) — this is NOT a general dapp-RPC surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthCall {
    /// The `0x`-prefixed 20-byte contract address the call targets (the `to`
    /// field of the JSON-RPC call object).
    pub to: String,
    /// The `0x`-prefixed ABI-encoded calldata (the `data` field): the function
    /// selector plus arguments.
    pub data: String,
    /// The block the read is evaluated against.
    pub block: BlockTag,
}

impl EthCall {
    /// A `latest`-block call to `to` with ABI-encoded `data`, the common ENS read
    /// shape.
    #[must_use]
    pub fn new(to: &str, data: &str) -> Self {
        Self {
            to: to.to_string(),
            data: data.to_string(),
            block: BlockTag::Latest,
        }
    }

    /// The same call pinned to a specific block.
    #[must_use]
    pub fn at_block(mut self, block: BlockTag) -> Self {
        self.block = block;
        self
    }
}

/// The Ethereum trusted-read seam: perform an `eth_call` and return its raw
/// (ABI-encoded) return bytes, or a typed [`ProviderError`].
///
/// ENS resolution (and any other werust-internal Ethereum read) calls ONLY
/// through this trait, so the concrete backend — the Phase-1 [`RpcProvider`], or
/// a Phase-2 trustless light client — never leaks past the seam. It is the whole
/// point of the seam: the trust level is a backend, not a rewrite.
///
/// # Shaped for a later async backend
///
/// [`eth_call`](EthereumProvider::eth_call) returns OWNED bytes by value (no
/// borrow tied to a sync call stack), so a Phase-2 async backend can drive its
/// async client to completion internally (bridged `block_on` at this boundary)
/// and return the owned result without the signature fighting it. The method is
/// `&self` (shared, not `&mut`), so a backend that pools connections behind
/// interior mutability — as an async client would — fits too.
///
/// Implementations MUST surface every failure as a [`ProviderError`] (never a
/// panic, never an empty-bytes "success").
pub trait EthereumProvider {
    /// Execute `call` and return its ABI-encoded return bytes.
    ///
    /// On success the returned [`Vec<u8>`] is the decoded `0x`-hex `result` of
    /// the JSON-RPC `eth_call` (empty only if the contract genuinely returned no
    /// bytes). A JSON-RPC error object surfaces as [`ProviderError::Rpc`]; a
    /// transport/non-2xx failure as [`ProviderError::Transport`]; a
    /// malformed/unparseable answer as [`ProviderError::Decode`]; a request bad
    /// before it is sent as [`ProviderError::InvalidRequest`].
    fn eth_call(&self, call: &EthCall) -> Result<Vec<u8>, ProviderError>;
}

/// A raw JSON-RPC HTTP transport: POST a JSON request body to an endpoint and
/// return the response body bytes.
///
/// This is the narrow transport boundary [`RpcProvider`] sits on. Keeping it a
/// trait (rather than calling `ureq` inline) does two jobs: it keeps the bound
/// HTTP client from leaking past a seam — exactly as `HttpFetcher` keeps `ureq`
/// behind the [`Fetcher`](fetcher::Fetcher) seam — and it lets tests substitute
/// an in-process transport double that CAPTURES the outgoing request body, so a
/// test can assert the `eth_call` JSON (method + params) was actually sent (a
/// GET-only transport that dropped the body could not pass). The default backend
/// is [`UreqRpcTransport`].
///
/// A transport surfaces a connection/protocol failure or a non-2xx status as a
/// [`ProviderError::Transport`]; it does NOT interpret the JSON-RPC envelope
/// (that is [`RpcProvider`]'s job).
pub trait RpcTransport {
    /// POST `request_body` (a serialized JSON-RPC request) to the configured
    /// endpoint as `application/json` and return the raw response body bytes.
    fn post_json(&self, request_body: &[u8]) -> Result<Vec<u8>, ProviderError>;
}

/// The default [`RpcTransport`]: a minimal synchronous JSON-RPC HTTP POST over a
/// bound HTTP+TLS stack ([`ureq`] with a rustls backend, the same stack the
/// [`Fetcher`](fetcher::Fetcher) seam binds).
///
/// TLS is delegated entirely to the bound rustls stack (never hand-written, per
/// `CONTEXT.md` / `docs/adr/0001`); this only adds the POST-with-a-JSON-body
/// shape the `Fetcher` seam's GET surface cannot express. Construct implicitly
/// via [`RpcProvider::new`] / [`RpcProvider::with_endpoint`]; it is cheap to
/// [`Clone`] (the underlying `ureq::Agent` pools connections internally).
#[derive(Clone)]
pub struct UreqRpcTransport {
    agent: ureq::Agent,
    endpoint: String,
}

impl UreqRpcTransport {
    /// A transport that POSTs to `endpoint`, with the same bounded connect/global
    /// timeouts and non-2xx-as-data configuration the `Fetcher` seam uses (so a
    /// non-2xx status is inspected and mapped to a [`ProviderError::Transport`]
    /// here rather than raised deep in `ureq`).
    fn new(endpoint: &str) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(DEFAULT_CONNECT_TIMEOUT))
            .timeout_global(Some(DEFAULT_GLOBAL_TIMEOUT))
            .build()
            .into();
        Self {
            agent,
            endpoint: endpoint.to_string(),
        }
    }
}

impl RpcTransport for UreqRpcTransport {
    fn post_json(&self, request_body: &[u8]) -> Result<Vec<u8>, ProviderError> {
        let mut response = self
            .agent
            .post(&self.endpoint)
            .content_type("application/json")
            .send(request_body)
            .map_err(|e| ProviderError::Transport(e.to_string()))?;

        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(ProviderError::Transport(format!(
                "rpc endpoint returned status {status}"
            )));
        }
        response
            .body_mut()
            .read_to_vec()
            .map_err(|e| ProviderError::Transport(e.to_string()))
    }
}

/// The Phase-1 TRUSTED [`EthereumProvider`] backend: a plain JSON-RPC `eth_call`
/// against a configured HTTP endpoint.
///
/// It builds the JSON-RPC request envelope, POSTs it over an [`RpcTransport`]
/// (the bound [`UreqRpcTransport`] by default), and decodes the `eth_call`
/// result into raw return bytes — surfacing every failure as a typed
/// [`ProviderError`]. It is TRUSTED (the result is taken on the endpoint's word,
/// see the module docs); Phase 2's trustless light client is a backend swap
/// behind the same [`EthereumProvider`] seam.
///
/// The endpoint is user-overridable with a labelled default: [`new`](RpcProvider::new)
/// resolves it via [`rpc_endpoint`] (the `WERUST_RPC_URL` env lever when set and
/// non-empty, else [`DEFAULT_RPC_ENDPOINT`]); [`with_endpoint`](RpcProvider::with_endpoint)
/// overrides it outright — mirroring `GatewayContentSource::new` / `with_gateway`,
/// with NO config subsystem.
///
/// It is generic over the [`RpcTransport`] so tests drive it against an
/// in-process transport double (capturing the request body, off the live
/// network), exactly as the fetcher/ipfs seams do.
pub struct RpcProvider<T: RpcTransport = UreqRpcTransport> {
    transport: T,
}

impl RpcProvider<UreqRpcTransport> {
    /// An RPC provider over the bound transport, pointed at the endpoint
    /// [`rpc_endpoint`] resolves for this session: the `WERUST_RPC_URL` env
    /// lever when set and non-empty, else the labelled [`DEFAULT_RPC_ENDPOINT`]
    /// (a trusted origin).
    #[must_use]
    pub fn new() -> Self {
        Self::with_endpoint(&rpc_endpoint())
    }

    /// An RPC provider over the bound transport, pointed at a specific `endpoint`
    /// (e.g. a local node, a different trusted RPC, or a test endpoint). This is
    /// the user-override of the default, with no config subsystem.
    #[must_use]
    pub fn with_endpoint(endpoint: &str) -> Self {
        Self {
            transport: UreqRpcTransport::new(endpoint),
        }
    }
}

impl Default for RpcProvider<UreqRpcTransport> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: RpcTransport> RpcProvider<T> {
    /// An RPC provider over a caller-supplied [`RpcTransport`].
    ///
    /// This is how a test substitutes an in-process transport double (to capture
    /// the outgoing request body and answer with a canned JSON-RPC response),
    /// keeping the whole path off the live network. Production callers use
    /// [`new`](RpcProvider::new) / [`with_endpoint`](RpcProvider::with_endpoint).
    pub fn with_transport(transport: T) -> Self {
        Self { transport }
    }
}

/// Build the JSON-RPC `eth_call` request envelope for `call` (the exact body
/// that goes on the wire).
///
/// The params are the standard positional `[{ to, data }, <block>]`: the call
/// object then the block tag. `id` is a fixed `1` — this seam issues one request
/// per call and does not multiplex, so a monotonic id would buy nothing.
fn build_eth_call_request(call: &EthCall) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_call",
        "params": [
            { "to": call.to, "data": call.data },
            call.block.as_rpc_param(),
        ],
    })
}

/// Decode a `0x`-prefixed hex byte string (an `eth_call` `result`) into bytes.
///
/// The empty result `"0x"` decodes to no bytes (a contract that returned
/// nothing). A missing `0x` prefix or a non-hex/odd-length body is refused as a
/// [`ProviderError::Decode`] rather than guessed.
fn decode_hex_result(hex: &str) -> Result<Vec<u8>, ProviderError> {
    let body = hex
        .strip_prefix("0x")
        .ok_or_else(|| ProviderError::Decode(format!("result is not 0x-prefixed hex: {hex}")))?;
    if body.len() % 2 != 0 {
        return Err(ProviderError::Decode(format!(
            "result hex has an odd length: {hex}"
        )));
    }
    (0..body.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&body[i..i + 2], 16)
                .map_err(|_| ProviderError::Decode(format!("result is not valid hex: {hex}")))
        })
        .collect()
}

/// Parse a JSON-RPC response body into the `eth_call` return bytes, mapping a
/// JSON-RPC error object onto [`ProviderError::Rpc`].
///
/// A well-formed response carries EITHER a `result` (the `0x`-hex return bytes)
/// or an `error` object (`{ code, message }`), never both. An `error` becomes a
/// [`ProviderError::Rpc`] with the endpoint's own code/message; a `result`
/// decodes via [`decode_hex_result`]; anything else (not JSON, neither field,
/// both fields, a non-string result) is a [`ProviderError::Decode`] — never a
/// silent empty success.
fn parse_eth_call_response(body: &[u8]) -> Result<Vec<u8>, ProviderError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|e| ProviderError::Decode(format!("response is not json: {e}")))?;

    let has_error = value.get("error").is_some_and(|e| !e.is_null());
    let has_result = value.get("result").is_some_and(|r| !r.is_null());

    // A JSON-RPC error object reports the call itself failed: surface the RPC's
    // own code/message so the caller sees the endpoint's reason.
    if has_error {
        let err = &value["error"];
        let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
        let message = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown rpc error")
            .to_string();
        return Err(ProviderError::Rpc { code, message });
    }

    if !has_result {
        return Err(ProviderError::Decode(
            "response has neither a result nor an error".to_string(),
        ));
    }

    let result = value["result"]
        .as_str()
        .ok_or_else(|| ProviderError::Decode("result is not a hex string".to_string()))?;
    decode_hex_result(result)
}

impl<T: RpcTransport> EthereumProvider for RpcProvider<T> {
    fn eth_call(&self, call: &EthCall) -> Result<Vec<u8>, ProviderError> {
        let request = build_eth_call_request(call);
        let body = serde_json::to_vec(&request)
            .map_err(|e| ProviderError::InvalidRequest(e.to_string()))?;
        let response = self.transport.post_json(&body)?;
        parse_eth_call_response(&response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;

    /// An in-process [`RpcTransport`] double that CAPTURES the outgoing request
    /// body and answers with a canned response — off the live network, and the
    /// tool that proves the `eth_call` JSON body was actually sent.
    ///
    /// A GET-only transport (or any transport that dropped the body) would leave
    /// `last_request` empty, so a test can assert the captured body carried the
    /// `eth_call` method + params, not merely that some bytes came back.
    struct CapturingTransport {
        response: Result<Vec<u8>, ProviderError>,
        last_request: RefCell<Option<Vec<u8>>>,
    }

    impl CapturingTransport {
        fn answering(body: &[u8]) -> Self {
            Self {
                response: Ok(body.to_vec()),
                last_request: RefCell::new(None),
            }
        }

        fn failing(err: ProviderError) -> Self {
            Self {
                response: Err(err),
                last_request: RefCell::new(None),
            }
        }

        /// The captured outgoing request body, parsed as JSON (panics if nothing
        /// was sent — the point of the assertion).
        fn captured_json(&self) -> Value {
            let bytes = self
                .last_request
                .borrow()
                .clone()
                .expect("the transport must have received an outgoing request body");
            serde_json::from_slice(&bytes).expect("the outgoing request body is json")
        }
    }

    impl RpcTransport for CapturingTransport {
        fn post_json(&self, request_body: &[u8]) -> Result<Vec<u8>, ProviderError> {
            *self.last_request.borrow_mut() = Some(request_body.to_vec());
            self.response.clone()
        }
    }

    fn ok_result_body(result_hex: &str) -> Vec<u8> {
        json!({ "jsonrpc": "2.0", "id": 1, "result": result_hex })
            .to_string()
            .into_bytes()
    }

    // -----------------------------------------------------------------------
    // The `WERUST_RPC_URL` endpoint precedence (task
    // `configurable-rpc-endpoint-via-env`). Network-isolated and env-mutation-
    // free: the precedence decision lives in the PURE `resolve_rpc_endpoint`
    // core, so no test touches `std::env::set_var` (an unsafe setter in Rust
    // 2024 that would race under parallel `cargo test`).
    // -----------------------------------------------------------------------

    #[test]
    fn the_labelled_default_endpoint_is_1rpc() {
        // The human-triggered default swap: the labelled default is now the
        // public, keyless `1rpc.io/eth` (publicnode.com was observed DNS- and
        // TLS-blocked on home networks).
        assert_eq!(
            DEFAULT_RPC_ENDPOINT,
            "https://mainnet.infura.io/v3/9aa3d95b3bc440fa88ea12eaa4456161"
        );
    }

    #[test]
    fn rpc_endpoint_falls_back_to_the_default_when_the_env_is_unset() {
        // No `WERUST_RPC_URL`: a fresh, unconfigured build resolves through the
        // labelled public default.
        assert_eq!(resolve_rpc_endpoint(None), DEFAULT_RPC_ENDPOINT);
    }

    #[test]
    fn rpc_endpoint_prefers_a_non_empty_env_value_whitespace_trimmed() {
        // A set, non-empty `WERUST_RPC_URL` wins over the default, trimmed.
        assert_eq!(
            resolve_rpc_endpoint(Some("https://example.test/rpc")),
            "https://example.test/rpc"
        );
        assert_eq!(
            resolve_rpc_endpoint(Some("  https://example.test/rpc  ")),
            "https://example.test/rpc",
            "the env value is whitespace-trimmed"
        );
    }

    #[test]
    fn rpc_endpoint_falls_back_to_the_default_when_the_env_is_empty() {
        // An EMPTY `WERUST_RPC_URL` (e.g. the CI secret is not configured, so
        // the workflow exports an empty string) must NOT override the default.
        assert_eq!(resolve_rpc_endpoint(Some("")), DEFAULT_RPC_ENDPOINT);
        assert_eq!(
            resolve_rpc_endpoint(Some("   ")),
            DEFAULT_RPC_ENDPOINT,
            "a whitespace-only value counts as empty"
        );
    }

    #[test]
    fn rpc_provider_new_resolves_its_endpoint_through_the_env_lever() {
        // The wiring assertion: `RpcProvider::new` / `default` point at whatever
        // `rpc_endpoint()` resolves for THIS process env (no env mutation — the
        // precedence itself is pinned by the pure-core tests above, and their
        // composition gives "unset env -> DEFAULT_RPC_ENDPOINT"). Reading back
        // the bound transport's endpoint proves `new` went through the
        // resolver, not a hardcoded constant.
        assert_eq!(RpcProvider::new().transport.endpoint, rpc_endpoint());
        assert_eq!(RpcProvider::default().transport.endpoint, rpc_endpoint());
    }

    #[test]
    fn issues_an_eth_call_and_returns_the_decoded_return_bytes() {
        // Acceptance: a `dyn EthereumProvider` caller issues an eth_call and gets
        // back the ABI-encoded return bytes, decoded from the `0x`-hex result.
        let transport = CapturingTransport::answering(&ok_result_body("0xdeadbeef"));
        let provider: RpcProvider<CapturingTransport> = RpcProvider::with_transport(transport);
        let dyn_provider: &dyn EthereumProvider = &provider;

        let bytes = dyn_provider
            .eth_call(&EthCall::new(
                "0x00000000000000000000000000000000000000c0",
                "0xabcdef",
            ))
            .expect("a well-formed result decodes to bytes");

        assert_eq!(bytes, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn the_outgoing_request_carries_the_eth_call_json_body() {
        // The load-bearing assertion: the request actually POSTed the JSON-RPC
        // eth_call body (method + params), so a GET-only transport that dropped
        // the body could not pass. We inspect the CAPTURED outgoing request.
        let transport = CapturingTransport::answering(&ok_result_body("0x"));
        let provider = RpcProvider::with_transport(transport);

        provider
            .eth_call(
                &EthCall::new("0x0000000000000000000000000000000000000abc", "0x12345678")
                    .at_block(BlockTag::Latest),
            )
            .expect("call succeeds");

        let sent = provider.transport.captured_json();
        assert_eq!(sent["jsonrpc"], "2.0");
        assert_eq!(sent["method"], "eth_call", "the method must be eth_call");
        let params = sent["params"].as_array().expect("params is an array");
        assert_eq!(params.len(), 2, "params are [call object, block tag]");
        assert_eq!(
            params[0]["to"], "0x0000000000000000000000000000000000000abc",
            "the call object carries the target address"
        );
        assert_eq!(
            params[0]["data"], "0x12345678",
            "the call object carries the abi-encoded calldata"
        );
        assert_eq!(params[1], "latest", "the default block tag is latest");
    }

    #[test]
    fn a_pinned_block_number_is_sent_as_a_hex_quantity() {
        // A caller that pins a block sends it as the JSON-RPC `0x`-hex quantity,
        // not "latest".
        let transport = CapturingTransport::answering(&ok_result_body("0x"));
        let provider = RpcProvider::with_transport(transport);
        provider
            .eth_call(&EthCall::new("0x00", "0x00").at_block(BlockTag::Number(0x1234)))
            .expect("call succeeds");
        assert_eq!(provider.transport.captured_json()["params"][1], "0x1234");
    }

    #[test]
    fn a_jsonrpc_error_object_surfaces_as_a_typed_rpc_error_not_a_panic() {
        // The endpoint answers with a JSON-RPC error OBJECT (a valid response
        // reporting the call failed): it must surface as a typed ProviderError::Rpc
        // carrying the endpoint's own code/message, never a panic, never empty
        // bytes.
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": { "code": -32000, "message": "execution reverted" }
        })
        .to_string()
        .into_bytes();
        let provider = RpcProvider::with_transport(CapturingTransport::answering(&body));

        let err = provider
            .eth_call(&EthCall::new("0x00", "0x00"))
            .expect_err("a jsonrpc error object is a typed failure");
        assert_eq!(
            err,
            ProviderError::Rpc {
                code: -32000,
                message: "execution reverted".to_string(),
            }
        );
    }

    #[test]
    fn a_transport_failure_surfaces_as_a_typed_transport_error() {
        // A transport/connection failure surfaces as ProviderError::Transport,
        // never a panic or an empty-bytes success.
        let provider = RpcProvider::with_transport(CapturingTransport::failing(
            ProviderError::Transport("connection refused".to_string()),
        ));
        let err = provider
            .eth_call(&EthCall::new("0x00", "0x00"))
            .expect_err("a transport failure is a typed error");
        assert!(matches!(err, ProviderError::Transport(_)), "got: {err:?}");
    }

    #[test]
    fn an_unparseable_result_is_refused_not_returned_as_empty_bytes() {
        // A 2xx answer that is not a well-formed eth_call result must be REFUSED
        // as a Decode error, never returned as empty/guessed bytes.
        // Not JSON at all:
        let p1 = RpcProvider::with_transport(CapturingTransport::answering(b"<html>not json"));
        assert!(matches!(
            p1.eth_call(&EthCall::new("0x00", "0x00")),
            Err(ProviderError::Decode(_))
        ));
        // JSON, but neither a result nor an error:
        let p2 = RpcProvider::with_transport(CapturingTransport::answering(
            br#"{"jsonrpc":"2.0","id":1}"#,
        ));
        assert!(matches!(
            p2.eth_call(&EthCall::new("0x00", "0x00")),
            Err(ProviderError::Decode(_))
        ));
        // A result that is not 0x-prefixed hex:
        let p3 =
            RpcProvider::with_transport(CapturingTransport::answering(&ok_result_body("not-hex")));
        assert!(matches!(
            p3.eth_call(&EthCall::new("0x00", "0x00")),
            Err(ProviderError::Decode(_))
        ));
    }

    #[test]
    fn an_empty_result_decodes_to_no_bytes() {
        // `"0x"` is a genuine empty return (a contract that returned nothing): it
        // decodes to zero bytes, distinct from a decode failure.
        let provider =
            RpcProvider::with_transport(CapturingTransport::answering(&ok_result_body("0x")));
        let bytes = provider
            .eth_call(&EthCall::new("0x00", "0x00"))
            .expect("0x is a valid empty result");
        assert!(bytes.is_empty());
    }

    // -----------------------------------------------------------------------
    // End-to-end over the BOUND transport against a loopback fixture endpoint.
    // -----------------------------------------------------------------------

    /// A throwaway loopback JSON-RPC endpoint, isolated from the live network
    /// (binds `127.0.0.1:0`). It CAPTURES each request body it receives and
    /// answers with one canned JSON-RPC response, so an end-to-end test can drive
    /// the real bound [`UreqRpcTransport`] AND then assert the request that went
    /// over the wire carried the `eth_call` body. Mirrors the fetcher/ipfs seam
    /// `LocalHttpServer` harness; torn down on [`Drop`].
    struct LocalRpcServer {
        addr: SocketAddr,
        shutdown: Arc<AtomicBool>,
        requests: Arc<Mutex<Vec<Vec<u8>>>>,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl LocalRpcServer {
        fn start(response_body: &[u8]) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            listener
                .set_nonblocking(true)
                .expect("non-blocking listener");
            let addr = listener.local_addr().expect("local addr");
            let response_body = response_body.to_vec();
            let shutdown = Arc::new(AtomicBool::new(false));
            let stop = shutdown.clone();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let captured = requests.clone();
            let handle = thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let _ = stream.set_nonblocking(false);
                            // Drain the COMPLETE request (head + full
                            // `Content-Length` body) before responding, so the
                            // captured body carries the whole `eth_call` JSON even
                            // when the body arrives in a later TCP segment under
                            // parallel load. Shared, race-hardened reader:
                            // `crate::loopback_test_server`.
                            if let Some(body) =
                                crate::loopback_test_server::read_request_body(&mut stream)
                            {
                                captured.lock().unwrap().push(body);
                            }
                            let head = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
                                len = response_body.len(),
                            );
                            let _ = stream.write_all(head.as_bytes());
                            let _ = stream.write_all(&response_body);
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

    impl Drop for LocalRpcServer {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    #[test]
    fn end_to_end_eth_call_over_the_bound_transport_off_the_network() {
        // The full production path, headless and off the live network: a
        // `dyn EthereumProvider` (RpcProvider over the BOUND UreqRpcTransport,
        // pointed at a loopback fixture) issues a real HTTP POST eth_call and gets
        // back the decoded return bytes. Then we assert the fixture actually
        // RECEIVED the eth_call JSON body (method + params) over the wire — a
        // GET-only transport that dropped the body could not pass.
        let server = LocalRpcServer::start(&ok_result_body(
            "0x000000000000000000000000000000000000000000000000000000000000002a",
        ));
        let provider = RpcProvider::with_endpoint(&server.endpoint());
        let dyn_provider: &dyn EthereumProvider = &provider;

        let bytes = dyn_provider
            .eth_call(&EthCall::new(
                "0x00000000000c2e074ec69a0dfb2997ba6c7d2e1e",
                "0x0178b8bf1234",
            ))
            .expect("a real POST eth_call against the loopback fixture returns bytes");
        // 0x0...02a decodes to 32 bytes ending in 0x2a.
        assert_eq!(bytes.len(), 32);
        assert_eq!(*bytes.last().unwrap(), 0x2a);

        // The fixture recorded the request body it received: assert it carried the
        // eth_call JSON over the wire (method + params), not just that bytes came
        // back.
        let bodies = server.captured_request_bodies();
        assert_eq!(bodies.len(), 1, "exactly one request went over the wire");
        let sent: Value =
            serde_json::from_slice(&bodies[0]).expect("the received request body was json");
        assert_eq!(sent["method"], "eth_call");
        assert_eq!(
            sent["params"][0]["to"],
            "0x00000000000c2e074ec69a0dfb2997ba6c7d2e1e"
        );
        assert_eq!(sent["params"][0]["data"], "0x0178b8bf1234");
        assert_eq!(sent["params"][1], "latest");
    }

    #[test]
    fn a_non_2xx_status_from_the_endpoint_is_a_transport_error() {
        // A reachable endpoint answering a non-2xx status is a transport failure
        // at this seam (the call got no well-formed JSON-RPC answer), surfaced as
        // a typed ProviderError::Transport, never a panic.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener.set_nonblocking(true).expect("non-blocking");
        let addr = listener.local_addr().expect("local addr");
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let handle = thread::spawn(move || {
            while !stop_thread.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let mut buf = [0u8; 1024];
                        let _ = stream.read(&mut buf);
                        let body = b"boom";
                        let head = format!(
                            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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

        let provider = RpcProvider::with_endpoint(&format!("http://{addr}/"));
        let err = provider
            .eth_call(&EthCall::new("0x00", "0x00"))
            .expect_err("a 500 is a transport failure at this seam");
        assert!(matches!(err, ProviderError::Transport(_)), "got: {err:?}");

        stop.store(true, Ordering::Relaxed);
        let _ = handle.join();
    }
}
