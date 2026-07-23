# Decisions: user-choosable IPFS retrieval backend (`retrieval-backend-user-setting`)

Durable record of the load-bearing design choices this task made, per the task template's "RECORD the settings-surface + persistence decisions durably" instruction and the runner's decision-bar rule. Linked from the task done-record. Spec: `ens-to-ipfs-resolution-phase1-rpc-skeleton`; blocking task: `verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend`; relates to `platform-capability-parity-guard` and the release-gate follow-on `retrieval-default-egress-before-final-release`.

## Reality re-check (before building)

- The `ContentRetriever` seam landed exactly as the task described: `crates/fetcher/src/retriever.rs` has the trait plus `TrustlessGatewayCarRetriever` with `DEFAULT_TRUSTLESS_GATEWAY` (`https://dweb.link`) and a `with_gateway()` override, no config crate. The load path (`werust_core::ipfs::resolve_ipfs_request`) takes a `&dyn ContentRetriever`.
- The retriever is constructed identically in three `install_ipfs` sites: `crates/webview-renderer/src/backend.rs` (desktop), `crates/werust-ios/rust/src/lib.rs`, `crates/werust-android/rust/src/lib.rs` — each hardcoded `TrustlessGatewayCarRetriever::new(HttpFetcher::new())`.
- There is NO settings surface today (grep for `settings`/`werust://` finds none). The parity guard (`docs/platform-capability-matrix.toml` + `crates/werust-core/tests/platform_capability_parity.rs`) is live and green.
- No drift found; the task's premises hold, so the build proceeded.

## Decision 1 — settings surface is an internal `werust://settings` page (GET-driven)

**Chosen:** A new internal `werust` custom scheme, resolved through the SAME `register_scheme_handler` seam the `ipfs://` trust hook already uses. `werust://settings` renders a self-contained HTML page (built toolkit-free in `werust-core`) listing the retrieval-backend options with the privacy/trust framing. Selecting an option is a GET to `werust://settings?backend=<kind>[&url=<endpoint>]` that the same handler parses, persists, and re-renders (with a confirmation + any validation error).

**Why:** (a) The task settled the surface as an internal `werust://settings`-style page, uniform across all three platforms; (b) it reuses the existing scheme-handler seam and the mobile OS-edge interception (`shouldInterceptRequest` / `WKURLSchemeHandler`) with no new IPC/POST plumbing to design — a form POST would need a new request-body path through the seam that does not exist, whereas a query-string GET rides the existing `SchemeRequest { uri }`; (c) it keeps the page logic in the toolkit-free core so it is headlessly testable, matching the `ipfs.rs` split.

**Alternatives considered:** a native settings dialog per platform (rejected: three UIs to design + maintain, and it would need three parity-matrix shapes rather than one — the task explicitly deferred this); a form POST (rejected: no request-body seam today, larger surface for no user-visible gain at this stage).

**Touches:** adds the `werust` scheme name (new concept — does not collide with `ipfs`; sits at the same seam layer as an internal page). The desktop chrome could later add a menu button that navigates to `werust://settings`; this task wires the page + persistence + load-path switch, and leaves a visible entry-point button as a small follow-on nicety (the page is reachable by typing `werust://settings`).

## Decision 2 — persistence is a minimal isolated JSON settings file, location via a `WERUST_SETTINGS_DIR` lever

**Chosen:** A single small JSON file `retrieval.json` under a settings directory. The directory is resolved as: `WERUST_SETTINGS_DIR` env var if set (the test-isolation + explicit-override lever), else the OS user-config dir (`$XDG_CONFIG_HOME/werust` or platform equivalent via the `dirs` crate), else an in-memory-only fallback if neither is available (the interim the task allows). NOT a config subsystem — one file, one struct, load/save.

**Why:** The task settled "a minimal isolated settings file (NOT a config subsystem)" and required tests to "isolate its location (temp/scratch via the relevant lever) and assert the real one is untouched." The `WERUST_SETTINGS_DIR` env var is the PRODUCTION override lever (explicit override, no global, mirroring the seam crate's `with_*()` ethos). For TEST isolation the module also exposes directory-taking cores (`load_from` / `save_to` / `apply_settings_request_in` / `active_gateway_endpoint_in`) that the env-based public API delegates to, so tests pass their own unique scratch directory directly and never mutate process-global env. (An earlier draft had tests `set_var(WERUST_SETTINGS_DIR)`; that made the werust-core lib test binary intermittently fail because `std::env::set_var` in a multithreaded test binary races with other threads reading env, i.e. the edition-2024 unsafe-env UB. The directory-taking cores remove that entirely and are the durable isolation seam.) JSON because the repo already depends on `serde_json` in the FFI JSON path (`ffi_json`), so no new dependency for the wire format; TOML would add `toml` to a non-test build (it is currently a dev-dep of the parity guard only).

**Alternatives considered:** TOML (rejected: adds a runtime dep the core does not otherwise carry); a platform-native store per OS (rejected: three mechanisms, the config-subsystem the task forbade).

**Touches:** the `WERUST_SETTINGS_DIR` env var is a new user-visible lever shared by any future setting that persists (the IPNS-TOFU pin store task already anticipates reusing this mechanism). Recorded here so a later setting reuses the same file dir rather than forking a second one.

## Decision 3 — initial options + the "coming soon" framing

**Chosen:** Two SELECTABLE options at ship time: `DefaultGateway` (the labelled public trustless gateway, `DEFAULT_TRUSTLESS_GATEWAY`) and `Custom { url }` (a user-supplied gateway/local-node URL, validated as an `http(s)://` origin). `DelegatedRouting` and `EmbeddedP2p` are shown on the page as "coming soon" and are REFUSED if selected (a typed `not-yet-available` error), never silently broken.

**Why:** Settled decision 3 of the task. The default-gateway default is the Phase-1/dev default per settled decision 4; the final shipped default is the separate release-gate task `retrieval-default-egress-before-final-release` and is NOT decided here.

**Touches:** the "coming soon" set will shrink as the delegated-routing / embedded-p2p backends land; each is a pure backend swap behind the existing seam, so only the selectable-set gate here changes.

## Decision 4 — the choice switches the real load path

**Chosen:** All three `install_ipfs` sites now load the persisted `RetrievalBackendChoice`, turn it into a gateway endpoint, and build `TrustlessGatewayCarRetriever::with_gateway(HttpFetcher::new(), &endpoint)`. `DefaultGateway` yields `DEFAULT_TRUSTLESS_GATEWAY`; `Custom { url }` yields the validated URL. So the chosen backend is the one the `ipfs://` load path actually uses.

**Why:** Acceptance criterion 2 requires the selection to switch the `ContentRetriever` the load path uses. Building the retriever from the persisted choice at `install_ipfs` time is the minimal wiring the seam was designed for.

**Known limitation (recorded, not hidden):** the retriever is built once at session `new()`; changing the setting takes effect on the NEXT session/launch (the persisted choice is read at startup), not live mid-session. A live hot-swap would need a settable retriever slot behind the scheme handler; that is a deliberate follow-on, out of this task's minimal scope. The persistence + next-launch switch satisfies criterion 3 ("persists across launches").
