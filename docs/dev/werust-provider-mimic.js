/**
 * werust-provider-mimic.js
 *
 * Installs a `window.ethereum` that behaves EXACTLY like werust's injected
 * EIP-1193 provider, so you can test how mandalas reacts to werust in a NORMAL
 * browser (Chrome/Firefox/Safari) without running werust.
 *
 * Faithful replica of crates/werust-core/src/provider.rs
 * (provider_shim + ProviderBridge::answer) + crates/werust-core/src/ethereum.rs.
 *
 * ── Usage ──────────────────────────────────────────────────────────────────
 * Include this as a <script> in mandalas' HTML head, BEFORE the app's module
 * scripts, so it installs before mandalas detects the provider:
 *
 *   <script src="./werust-provider-mimic.js"></script>
 *
 * Activate it with ?werust=1 in the URL (or set localStorage["werust-mimic"] = "1"
 * for every load). Without the flag it does nothing — zero impact on normal use.
 *
 *   https://mandalas.eth.limo/?werust=1        ← mimic active
 *   https://mandalas.eth.limo/                  ← normal, untouched
 *
 * ── What it replicates ─────────────────────────────────────────────────────
 *   • isWerust: true, no isMetaMask / isExodus / isOpera / isGameStop flags
 *   • eth_chainId         → "0x1"  (mainnet — the chain werust's ENS backend reads)
 *   • eth_accounts        → []     (passive read: no keys, conformant empty)
 *   • eth_requestAccounts → REJECT 4100 Unauthorized (NOT [], never resolve empty)
 *   • everything else     → REJECT 4200 Unsupported Method
 *   • EIP-1193 event surface (on/removeListener/emit) but NEVER emits
 *   • NO EIP-6963 announcement (no announceProvider / requestProvider)
 *
 * Source of truth (keep in sync):
 *   crates/werust-core/src/provider.rs  — provider_shim() + ProviderBridge::answer()
 *   crates/werust-core/src/ethereum.rs  — CHAIN_ID = "0x1"
 */
(function () {
  "use strict";

  // ── Toggle: only activate when explicitly requested ──────────────────────
  var urlOn = new URLSearchParams(location.search).get("werust") === "1";
  var lsOn;
  try { lsOn = localStorage.getItem("werust-mimic") === "1"; } catch (e) { lsOn = false; }
  if (!urlOn && !lsOn) return;

  // If a real wallet extension already installed window.ethereum, save it so
  // you can restore with ?werust=0 if needed.
  var preexisting = window.ethereum || null;

  var CHAIN_ID = "0x1"; // ethereum::CHAIN_ID — mainnet (ENS resolves against mainnet registry)

  var UNAUTHORIZED_MESSAGE =
    "werust does not have a wallet yet: it gives this page a read-only Ethereum " +
    "connection and holds no keys, so there is no account it can authorise.";

  // ── Logging ─────────────────────────────────────────────────────────────
  // Every request() call, every event listener, every emit — logged with a
  // monotonically increasing seq so you can trace the exact sequence mandalas
  // follows when it detects and connects to the provider.
  var seq = 0;
  function tag(label) {
    return "%c[werust-mimic #" + (++seq) + "] " + label;
  }
  var LOG_STYLE = "color:#6af;font-weight:bold";
  var LOG_STYLE_ERR = "color:#f66;font-weight:bold";
  var LOG_STYLE_OK = "color:#6f6;font-weight:bold";
  var LOG_STYLE_EVENT = "color:#f9a;font-weight:bold";

  // ── EIP-1193 event-emitter surface (present but inert, like werust) ──────
  var listeners = new Map();

  function emit(event, payload) {
    var hs = listeners.get(event);
    var has = hs && hs.length > 0;
    console.log(tag("emit(" + event + ")"), LOG_STYLE_EVENT,
      has ? hs.length + " listener(s)" : "0 listeners (no-op)",
      payload !== undefined ? "payload:" : "", payload !== undefined ? payload : "");
    if (!hs) return false;
    hs.slice().forEach(function (h) { h(payload); });
    return has;
  }

  // ── The answer function — a faithful port of ProviderBridge::answer() ────
  function answer(method) {
    switch (method) {
      case "eth_chainId":
        return { ok: true, result: CHAIN_ID };
      case "eth_accounts":
        return { ok: true, result: [] };
      case "eth_requestAccounts":
        return { ok: false, error: { code: 4100, message: UNAUTHORIZED_MESSAGE } };
      default:
        return { ok: false, error: { code: 4200, message: "unsupported method: " + method } };
    }
  }

  var provider = {
    isWerust: true,
    // NOTE: werust does NOT set isMetaMask, isExodus, isOpera, isGameStop, etc.
    // A dapp that checks these to NAME the wallet will find them absent.
    request: function (args) {
      var method = args && args.method;
      var params = (args && args.params) || [];
      var reqSeq = ++seq;
      var label = "[werust-mimic #" + reqSeq + "] request(" + method + ")";
      console.groupCollapsed("%c" + label, LOG_STYLE, "params:", params);
      console.trace("call stack");
      return new Promise(function (resolve, reject) {
        var res = answer(method);
        if (res.ok) {
          console.log("%c" + label + " → resolve", LOG_STYLE_OK, JSON.stringify(res.result));
          console.groupEnd();
          resolve(res.result);
        } else {
          console.log("%c" + label + " → reject", LOG_STYLE_ERR, "code:" + res.error.code, "message:" + res.error.message);
          console.groupEnd();
          var err = new Error(res.error.message);
          err.code = res.error.code;
          reject(err);
        }
      });
    },
    on: function (event, handler) {
      console.log(tag("on(" + event + ")"), LOG_STYLE_EVENT, "listener registered, total for " + event + ":" + ((listeners.get(event) || []).length + 1));
      if (!listeners.has(event)) listeners.set(event, []);
      listeners.get(event).push(handler);
      return provider;
    },
    removeListener: function (event, handler) {
      var hs = listeners.get(event);
      console.log(tag("removeListener(" + event + ")"), LOG_STYLE_EVENT, hs ? hs.length + " listener(s) before" : "no listeners");
      if (!hs) return provider;
      var i = hs.indexOf(handler);
      if (i !== -1) hs.splice(i, 1);
      return provider;
    },
    emit: emit,
  };

  // Install exactly like werust: defineProperty with configurable: true.
  Object.defineProperty(window, "ethereum", {
    value: provider,
    configurable: true,
  });

  // NO EIP-6963 announcement — werust does not implement EIP-6963. A dapp that
  // dispatches eip6963:requestProvider gets no announceProvider back and must
  // fall back to window.ethereum (which mandalas does after a 100ms timeout).

  console.log("[werust-mimic] window.ethereum installed (isWerust: true, chainId: " + CHAIN_ID + ", no EIP-6963).",
    preexisting ? "Replaced preexisting provider: " + preexisting : "No preexisting provider was present.");
})();