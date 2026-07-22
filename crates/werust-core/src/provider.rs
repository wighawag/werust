//! The native **EIP-1193 provider** surface and its message transport over the
//! [`Renderer`] seam's script-message bridge.
//!
//! This module is the toolkit-free heart of werust's first trust hook
//! (`CONTEXT.md`, `docs/adr/0001`): a page's JS sees a native Ethereum provider
//! exposing the standard `request({ method, params })` interface (plus the
//! EIP-1193 event-emitter surface), and each `request(...)` round-trips across
//! the bridge to a NATIVE handler and back. It is deliberately split so the whole
//! round-trip is testable WITHOUT a webview or a GTK main loop:
//!
//! * [`provider_shim`] is the page-side JS installed at document start
//!   ([`Renderer::inject_script`](renderer::Renderer::inject_script)). It exposes
//!   `window.ethereum`, turns each `request(...)` into a pending Promise, posts a
//!   JSON envelope up the [script-message
//!   bridge](renderer::Renderer::register_script_message_handler), and settles
//!   the Promise when the native side pushes a response back.
//! * [`ProviderBridge`] is the NATIVE handler: it parses a page envelope, answers
//!   it with a read-only method stub (no keys — see below), and emits the JS the
//!   backend evaluates in the page
//!   ([`Renderer::evaluate_javascript`](renderer::Renderer::evaluate_javascript))
//!   to settle the page's pending Promise. This is the browser -> page RESPONSE
//!   push the round-trip needs.
//!
//! **No key custody here.** This wires the provider SURFACE and message transport
//! only; it answers exclusively benign, read-only methods (a chain-id / accounts
//! stub) and holds NO private keys. The wallet-broker security model (own-process
//! signing broker, the page never holds keys) is deferred to the exploration spec
//! `rust-successor-native-renderer-architecture-benchmark`.

use renderer::{Renderer, ScriptMessage};
use serde_json::{json, Value};

/// The default script-message bridge name the provider posts to and the shim
/// reads back from.
///
/// The page posts request envelopes to
/// `window.webkit.messageHandlers.<PROVIDER_BRIDGE>.postMessage(...)` and the
/// native side settles pending Promises by calling
/// `window.<PROVIDER_BRIDGE>.__resolve(...)`. Kept as one constant so the
/// injected shim, the registered handler, and the response push all agree on the
/// single channel name.
pub const PROVIDER_BRIDGE: &str = "werustProvider";

/// The EIP-1193 chain id the read-only stub reports, as the standard `0x`-prefixed
/// quantity. `0x1` is Ethereum mainnet — a benign, keyless value that proves the
/// full request -> native -> response round-trip.
pub const STUB_CHAIN_ID: &str = "0x1";

/// A parsed page-side `request(...)` envelope: the correlation `id` plus the
/// EIP-1193 `method` and its `params`.
///
/// The page assigns a monotonic `id` per `request(...)` so the native response
/// can settle the right pending Promise; `method`/`params` are the EIP-1193 call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRequest {
    /// The page-assigned correlation id for this request.
    pub id: u64,
    /// The EIP-1193 method name (e.g. `eth_chainId`).
    pub method: String,
    /// The method params, as a JSON array (EIP-1193 params are positional).
    pub params: Value,
}

/// An EIP-1193 `ProviderRpcError`-shaped failure the native side returns for a
/// request it will not answer.
///
/// Carries the numeric `code` and human `message` the page-side shim rejects the
/// pending Promise with, so a dapp sees a standard provider error rather than an
/// opaque one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderError {
    /// The EIP-1193 / JSON-RPC error code (e.g. `4200` method-not-supported).
    pub code: i64,
    /// A human-readable error message.
    pub message: String,
}

impl ProviderError {
    /// The EIP-1193 `4200` "Unsupported Method" error for a method the read-only
    /// stub does not answer.
    #[must_use]
    pub fn unsupported(method: &str) -> Self {
        ProviderError {
            code: 4200,
            message: format!("unsupported method: {method}"),
        }
    }

    /// A `-32700` parse-error for an envelope the native side could not read.
    #[must_use]
    pub fn parse_error(detail: &str) -> Self {
        ProviderError {
            code: -32700,
            message: format!("could not parse provider request: {detail}"),
        }
    }
}

/// The native handler behind the provider bridge: it answers page `request(...)`
/// envelopes with a read-only method stub and holds NO keys.
///
/// [`handle`](ProviderBridge::handle) takes the raw JSON body a page posted up the
/// script-message bridge and returns the JS to evaluate back in the page to settle
/// its pending Promise. Answering is a pure function of the request (a keyless,
/// read-only stub), so the whole round-trip is unit-testable without a webview.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProviderBridge;

impl ProviderBridge {
    /// A fresh provider bridge.
    #[must_use]
    pub fn new() -> Self {
        ProviderBridge
    }

    /// Parse a page-posted envelope body into a [`ProviderRequest`].
    ///
    /// The page posts `{ "id": <number>, "method": <string>, "params": <array> }`;
    /// a missing `params` defaults to an empty array (EIP-1193 allows omitting
    /// params). A body that is not this shape is an error (surfaced to the page as
    /// a parse-error rejection).
    pub fn parse(body: &str) -> Result<ProviderRequest, ProviderError> {
        let value: Value =
            serde_json::from_str(body).map_err(|e| ProviderError::parse_error(&e.to_string()))?;
        let id = value
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| ProviderError::parse_error("missing numeric id"))?;
        let method = value
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| ProviderError::parse_error("missing string method"))?
            .to_string();
        let params = value.get("params").cloned().unwrap_or_else(|| json!([]));
        Ok(ProviderRequest { id, method, params })
    }

    /// Answer a parsed [`ProviderRequest`] with the read-only stub.
    ///
    /// The stub answers only benign, keyless methods:
    ///
    /// * `eth_chainId` -> [`STUB_CHAIN_ID`] (the configured chain id).
    /// * `eth_accounts` / `eth_requestAccounts` -> `[]` (no accounts, because no
    ///   keys are held here — the wallet-broker model is deferred).
    ///
    /// Any other method is refused with an EIP-1193
    /// [`unsupported`](ProviderError::unsupported) error. NOTHING here signs or
    /// touches a private key.
    pub fn answer(request: &ProviderRequest) -> Result<Value, ProviderError> {
        match request.method.as_str() {
            "eth_chainId" => Ok(json!(STUB_CHAIN_ID)),
            // No keys are held here, so there are no accounts to report. This is a
            // read-only stub: it demonstrates the round-trip, it does not custody
            // keys or grant access (the wallet-broker model is deferred).
            "eth_accounts" | "eth_requestAccounts" => Ok(json!([])),
            other => Err(ProviderError::unsupported(other)),
        }
    }

    /// Handle a raw page-posted body end-to-end, returning the JS to evaluate back
    /// in the page to settle the matching pending Promise.
    ///
    /// Parses the envelope, answers it with the stub, and renders the response
    /// delivery call ([`resolve_script`] / [`reject_script`]). On a parse failure
    /// where no id could be recovered there is no pending Promise to settle, so
    /// `None` is returned (nothing to evaluate). This is the single call the
    /// backend wires: page message in, response-delivery JS out.
    pub fn handle(&self, body: &str) -> Option<String> {
        match Self::parse(body) {
            Ok(request) => Some(match Self::answer(&request) {
                Ok(result) => resolve_script(request.id, &result),
                Err(err) => reject_script(request.id, &err),
            }),
            // The envelope was unreadable, so we could not recover an id: there is
            // no pending page Promise to settle, and nothing to evaluate.
            Err(_) => None,
        }
    }
}

/// The JS that settles a page's pending `request(...)` Promise with a `result`.
///
/// The backend evaluates this in the page
/// ([`Renderer::evaluate_javascript`](renderer::Renderer::evaluate_javascript)):
/// it calls the shim's private resolver with the correlation `id` and the JSON
/// result, so the exact pending Promise resolves. `result` is JSON-encoded, so it
/// is always a safe JS literal.
#[must_use]
pub fn resolve_script(id: u64, result: &Value) -> String {
    format!(
        "window.{bridge}.__resolve({id}, {result});",
        bridge = PROVIDER_BRIDGE,
        result = result
    )
}

/// The JS that rejects a page's pending `request(...)` Promise with an EIP-1193
/// error.
///
/// The mirror of [`resolve_script`]: it calls the shim's private rejecter with the
/// correlation `id` and an EIP-1193 `ProviderRpcError`-shaped `{ code, message }`,
/// JSON-encoded so it is a safe literal.
#[must_use]
pub fn reject_script(id: u64, error: &ProviderError) -> String {
    let payload = json!({ "code": error.code, "message": error.message });
    format!(
        "window.{bridge}.__reject({id}, {payload});",
        bridge = PROVIDER_BRIDGE,
        payload = payload
    )
}

/// The page-side EIP-1193 provider shim, injected at document start.
///
/// Installs `window.ethereum` (and `window.<PROVIDER_BRIDGE>` for the native
/// response push) exposing the standard EIP-1193 surface:
///
/// * `request({ method, params })` -> a Promise. Each call is assigned a
///   correlation id, its resolve/reject are parked in a pending map, and a JSON
///   envelope is posted up the script-message bridge. The Promise settles when the
///   native side pushes back `__resolve(id, result)` / `__reject(id, error)`.
/// * the EIP-1193 event-emitter surface: `on`, `removeListener`, and an internal
///   `emit`, over the standard events (`connect`, `disconnect`, `chainChanged`,
///   `accountsChanged`, `message`).
/// * `isWerust: true` so a dapp can detect the injected provider.
///
/// The shim holds no keys and makes no trust decisions; it is pure transport.
#[must_use]
pub fn provider_shim() -> String {
    // The bridge name is substituted in so the shim, the registered handler, and
    // the native response push all use the single `PROVIDER_BRIDGE` channel.
    format!(
        r#"(function () {{
  "use strict";
  var BRIDGE = "{bridge}";
  var pending = new Map();
  var nextId = 1;
  var listeners = new Map();

  function post(envelope) {{
    // The script-message bridge: page -> native. WebKitGTK exposes it at
    // window.webkit.messageHandlers.<name>; guard so the shim degrades cleanly
    // if it is somehow absent rather than throwing into the page.
    var mh = window.webkit
      && window.webkit.messageHandlers
      && window.webkit.messageHandlers[BRIDGE];
    if (mh && typeof mh.postMessage === "function") {{
      mh.postMessage(JSON.stringify(envelope));
    }} else {{
      throw new Error("werust provider bridge unavailable");
    }}
  }}

  var provider = {{
    isWerust: true,
    request: function (args) {{
      var method = args && args.method;
      var params = (args && args.params) || [];
      var id = nextId++;
      return new Promise(function (resolve, reject) {{
        pending.set(id, {{ resolve: resolve, reject: reject }});
        try {{
          post({{ id: id, method: method, params: params }});
        }} catch (e) {{
          pending.delete(id);
          reject(e);
        }}
      }});
    }},
    on: function (event, handler) {{
      if (!listeners.has(event)) listeners.set(event, []);
      listeners.get(event).push(handler);
      return provider;
    }},
    removeListener: function (event, handler) {{
      var hs = listeners.get(event);
      if (!hs) return provider;
      var i = hs.indexOf(handler);
      if (i !== -1) hs.splice(i, 1);
      return provider;
    }},
    emit: function (event, payload) {{
      var hs = listeners.get(event);
      if (!hs) return false;
      hs.slice().forEach(function (h) {{ h(payload); }});
      return hs.length > 0;
    }}
  }};

  // The native response push channel: the backend evaluates
  // window.<BRIDGE>.__resolve(id, result) / __reject(id, error) to settle the
  // pending Promise the correlation id belongs to.
  var channel = {{
    __resolve: function (id, result) {{
      var p = pending.get(id);
      if (!p) return;
      pending.delete(id);
      p.resolve(result);
    }},
    __reject: function (id, error) {{
      var p = pending.get(id);
      if (!p) return;
      pending.delete(id);
      var err = new Error((error && error.message) || "provider error");
      if (error && typeof error.code !== "undefined") err.code = error.code;
      p.reject(err);
    }}
  }};

  Object.defineProperty(window, BRIDGE, {{ value: channel }});
  Object.defineProperty(window, "ethereum", {{ value: provider, configurable: true }});
}})();
"#,
        bridge = PROVIDER_BRIDGE,
    )
}

/// Route one script-message-bridge message through the provider, emitting the
/// response-delivery JS (if any) to a `respond` sink.
///
/// This is the pure heart of the round-trip's native side, split out so it is
/// testable WITHOUT a webview: a live backend registers a script-message handler
/// that calls this with each posted [`ScriptMessage`] and a `respond` sink that
/// evaluates the emitted JS back in the page
/// ([`Renderer::evaluate_javascript`](renderer::Renderer::evaluate_javascript) —
/// see [`install_provider`]). Messages for any handler other than
/// [`PROVIDER_BRIDGE`] are ignored; an unreadable envelope with no recoverable id
/// yields nothing to push. No keys are involved.
pub fn route_provider_message(
    bridge: &ProviderBridge,
    message: &ScriptMessage,
    respond: &mut dyn FnMut(String),
) {
    if message.handler != PROVIDER_BRIDGE {
        return;
    }
    if let Some(script) = bridge.handle(&message.body) {
        respond(script);
    }
}

/// Inject the page-side provider shim into a [`Renderer`] backend.
///
/// Installs [`provider_shim`] at document start so every page sees a detectable
/// `window.ethereum` exposing the standard EIP-1193 `request(...)` interface and
/// event surface. This is HALF the wiring: it makes the provider DETECTABLE and
/// callable from the page. The other half — routing each posted request envelope
/// through a [`ProviderBridge`] and pushing the response back via
/// [`Renderer::evaluate_javascript`] — is backend-specific (the response push runs
/// on the backend's own loop, capturing a cloneable handle to it), so it is wired
/// where the backend lives (the webview backend's `install_provider`). The pure
/// routing that installer delegates to is [`route_provider_message`], exercised
/// headlessly by this module's tests.
pub fn inject_provider_shim(renderer: &mut dyn Renderer) {
    renderer.inject_script(&provider_shim());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shim_exposes_a_detectable_eip1193_provider_surface() {
        // Acceptance: pages see an injected provider exposing the standard
        // request(...) interface plus the event-emitter surface, and it is
        // detectable. We assert the shim's SHAPE (it is pure text here; the live
        // eval is the backend integration test).
        let shim = provider_shim();
        // Detectable: it installs window.ethereum and flags itself.
        assert!(shim.contains(r#""ethereum""#));
        assert!(shim.contains("isWerust: true"));
        // The standard request(...) interface.
        assert!(shim.contains("request: function"));
        // The EIP-1193 event-emitter surface.
        assert!(shim.contains("on: function"));
        assert!(shim.contains("removeListener: function"));
        // It posts over the named bridge and settles on the native push.
        assert!(shim.contains(PROVIDER_BRIDGE));
        assert!(shim.contains("postMessage"));
        assert!(shim.contains("__resolve"));
        assert!(shim.contains("__reject"));
    }

    #[test]
    fn parses_a_page_request_envelope() {
        let req = ProviderBridge::parse(r#"{"id":7,"method":"eth_chainId","params":[]}"#)
            .expect("a well-formed envelope parses");
        assert_eq!(
            req,
            ProviderRequest {
                id: 7,
                method: "eth_chainId".into(),
                params: json!([]),
            }
        );
    }

    #[test]
    fn parse_defaults_missing_params_to_empty_array() {
        let req = ProviderBridge::parse(r#"{"id":1,"method":"eth_accounts"}"#)
            .expect("params may be omitted");
        assert_eq!(req.params, json!([]));
    }

    #[test]
    fn parse_rejects_a_malformed_envelope() {
        assert!(ProviderBridge::parse("not json").is_err());
        assert!(ProviderBridge::parse(r#"{"method":"eth_chainId"}"#).is_err()); // no id
        assert!(ProviderBridge::parse(r#"{"id":1}"#).is_err()); // no method
    }

    #[test]
    fn chain_id_stub_answers_without_keys() {
        // Acceptance: a benign, read-only method demonstrates the round-trip end
        // to end without holding any private keys.
        let req = ProviderRequest {
            id: 42,
            method: "eth_chainId".into(),
            params: json!([]),
        };
        let result = ProviderBridge::answer(&req).expect("chain id is answered");
        assert_eq!(result, json!(STUB_CHAIN_ID));
    }

    #[test]
    fn accounts_stub_reports_no_keys() {
        // No keys are held here, so eth_accounts is an empty list — the round-trip
        // works, and it grants nothing.
        for method in ["eth_accounts", "eth_requestAccounts"] {
            let req = ProviderRequest {
                id: 1,
                method: method.into(),
                params: json!([]),
            };
            assert_eq!(ProviderBridge::answer(&req).unwrap(), json!([]));
        }
    }

    #[test]
    fn unsupported_method_is_refused_with_an_eip1193_error() {
        // Anything beyond the read-only stub (in particular signing) is refused
        // with a standard EIP-1193 error, never silently key-touched.
        let req = ProviderRequest {
            id: 3,
            method: "eth_sendTransaction".into(),
            params: json!([]),
        };
        let err = ProviderBridge::answer(&req).expect_err("signing is not answered here");
        assert_eq!(err.code, 4200);
        assert!(err.message.contains("eth_sendTransaction"));
    }

    #[test]
    fn handle_round_trips_a_request_to_a_resolve_push() {
        // Acceptance: a page request round-trips to the native handler and back
        // with a RESULT — the handler emits the JS that settles the page's pending
        // Promise (the browser -> page response push).
        let bridge = ProviderBridge::new();
        let script = bridge
            .handle(r#"{"id":9,"method":"eth_chainId","params":[]}"#)
            .expect("a well-formed request yields a response push");
        // It calls the shim's private resolver with the correlation id + result.
        assert_eq!(
            script,
            format!(r#"window.{PROVIDER_BRIDGE}.__resolve(9, "{STUB_CHAIN_ID}");"#)
        );
    }

    #[test]
    fn handle_round_trips_an_error_to_a_reject_push() {
        let bridge = ProviderBridge::new();
        let script = bridge
            .handle(r#"{"id":4,"method":"personal_sign","params":["0x00"]}"#)
            .expect("even a refused method settles the pending Promise");
        assert!(
            script.starts_with(&format!("window.{PROVIDER_BRIDGE}.__reject(4, ")),
            "a refused method rejects the correlated Promise: {script}"
        );
        assert!(script.contains("4200"));
    }

    #[test]
    fn handle_drops_an_unrecoverable_envelope() {
        // With no recoverable id there is no pending Promise to settle, so there
        // is nothing to evaluate back in the page.
        let bridge = ProviderBridge::new();
        assert_eq!(bridge.handle("}not json{"), None);
    }

    #[test]
    fn route_only_answers_the_provider_bridge_channel() {
        // The router settles requests on the provider channel and ignores traffic
        // for any other script-message handler.
        let bridge = ProviderBridge::new();
        let mut pushed: Vec<String> = Vec::new();

        route_provider_message(
            &bridge,
            &ScriptMessage {
                handler: PROVIDER_BRIDGE.into(),
                body: r#"{"id":2,"method":"eth_chainId","params":[]}"#.into(),
            },
            &mut |script| pushed.push(script),
        );
        assert_eq!(pushed.len(), 1);
        assert!(pushed[0].contains("__resolve(2"));

        route_provider_message(
            &bridge,
            &ScriptMessage {
                handler: "someOtherChannel".into(),
                body: r#"{"id":3,"method":"eth_chainId","params":[]}"#.into(),
            },
            &mut |script| pushed.push(script),
        );
        assert_eq!(pushed.len(), 1, "traffic for another channel is ignored");
    }
}
