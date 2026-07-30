//! Windows WebView2 backend SHAPE guard (task `windows-webview2-renderer-backend`,
//! `docs/adr/0011-webview2-for-windows.md`'s Windows split, sub-task 2).
//!
//! WHY A SOURCE-SHAPE GUARD: the backend's engine half is `#[cfg(windows)]`, and
//! this repo's `verify` gate runs on Ubuntu with no Windows SDK and no WebView2
//! Runtime, so `cargo build` NEVER compiles it. That is the same position
//! `crates/macos-renderer`, `crates/windows-origin-probe` and the two mobile
//! edges are in, and the repo's answer is the same: a plain `cargo test` that
//! PARSES the source it cannot compile and asserts the properties compilation
//! would not have proven anyway --
//!
//! * that the seam was not WIDENED for Windows,
//! * that the shared toolkit-free half is CONSUMED rather than copied (the
//!   criterion a compiler is blind to),
//! * that both TRUST HOOKS are really wired rather than silently no-op'd (the
//!   exact gap `docs/adr/0005` exists to forbid),
//! * that the scheme-name-set constraint is answered by a LAZY environment
//!   rather than by a trait change (ADR-0011 finding 5),
//! * that the blocking verify stays OFF the message-loop thread (`docs/adr/0008`),
//! * that a machine with NO WebView2 Runtime fails honestly rather than crashing,
//!   and
//! * that this task built NO chrome.
//!
//! Everything that is a pure RULE rather than a COM call lives in `src/pure.rs`
//! and is unit-tested normally; this guard covers the wiring.
//!
//! Acceptance criteria mapped to assertions below:
//! 1. A `Renderer` impl over WebView2, with NO widening of the trait
//!    (`the_backend_implements_the_whole_seam_over_webview2`,
//!    `the_renderer_seam_was_not_widened_for_windows`).
//! 2. Its own crate, no gtk4/webkit6, CONSUMING `webview-shared`
//!    (`the_windows_backend_crate_depends_on_no_toolkit_and_consumes_the_shared_half`).
//! 3. The scheme-name-set constraint is answered by a LAZY environment
//!    (`the_environment_is_created_lazily_so_schemes_can_be_registered_first`).
//! 4. Both trust hooks work (`both_trust_hooks_are_really_wired_never_silent_no_ops`),
//!    end-to-end on a real WebView2 in `examples/trust_hooks_smoke.rs`
//!    (`the_ci_smoke_drives_both_trust_hooks_with_a_negative_control`).
//! 5. Navigation, history, the lifecycle, the bridge and scheme interception go
//!    through the seam, and the SPA URL change uses `add_SourceChanged` /
//!    `IsNewDocument` rather than an inference
//!    (`the_load_lifecycle_history_bridge_and_scheme_hook_are_really_wired`).
//! 6. A machine without the runtime fails honestly, never crashes
//!    (`a_machine_without_the_webview2_runtime_fails_honestly`).
//! 7. The CI leg builds, tests and EXERCISES this crate
//!    (`the_windows_ci_leg_builds_tests_and_exercises_this_crate`).
//! 8. What CI proved versus what awaits real Windows hardware is written down
//!    (`the_verification_honesty_is_recorded`).

use std::path::{Path, PathBuf};

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/windows-renderer`, so the root is two levels up.
fn source(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn exists(relative: &str) -> bool {
    repo_root().join(relative).exists()
}

fn backend() -> String {
    source("crates/windows-renderer/src/backend.rs")
}

/// `source` with every comment line dropped, so a "does this file mention X"
/// assertion is about the CODE and not about prose.
fn code_only(source: &str) -> String {
    source
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.starts_with("//") || trimmed.starts_with("#"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The slice of `source` from the start of `from` up to (not including) `to`.
/// Both markers must be present and in order, so a rename cannot silently make a
/// bounded assertion vacuous by matching an empty slice.
fn between<'a>(source: &'a str, from: &str, to: &str) -> &'a str {
    let start = source
        .find(from)
        .unwrap_or_else(|| panic!("the source must contain `{from}`"));
    let end = source[start..]
        .find(to)
        .unwrap_or_else(|| panic!("the source must contain `{to}` after `{from}`"));
    &source[start..start + end]
}

#[test]
fn the_backend_implements_the_whole_seam_over_webview2() {
    // Criterion 1: a real `Renderer` implementation over the platform WebView2
    // (not a stub, not `wry`, not the abandoned `webview2` crate).
    let backend = backend();
    assert!(
        backend.contains("impl Renderer for Webview2Renderer"),
        "the Windows backend must implement the `Renderer` seam"
    );
    assert!(
        backend.contains("use webview2_com::Microsoft::Web::WebView2::Win32::*"),
        "the backend must bind the platform WebView2 through `webview2-com`"
    );
    // Every seam method the browser drives, present in the impl block.
    let seam_impl = between(&backend, "impl Renderer for Webview2Renderer", "\n}\n");
    for method in [
        "fn navigate(",
        "fn reload(",
        "fn stop(",
        "fn go_back(",
        "fn go_forward(",
        "fn can_go_back(",
        "fn can_go_forward(",
        "fn load_state(",
        "fn current_url(",
        "fn poll_event(",
        "fn view_handle(",
        "fn send_pointer(",
        "fn send_key(",
        "fn send_scroll(",
        "fn set_focus(",
        "fn register_script_message_handler(",
        "fn inject_script(",
        "fn evaluate_javascript(",
        "fn register_scheme_handler(",
        "fn trust_hooks(",
        "fn trust_posture(",
        "fn mark_ens_origin(",
        "fn mark_mutable_name(",
    ] {
        assert!(
            seam_impl.contains(method),
            "the Windows backend must implement the seam's `{method}`"
        );
    }
}

#[test]
fn the_renderer_seam_was_not_widened_for_windows() {
    // Criterion 1, the other half: adding a platform must NOT add a method to the
    // shared trait. ADR-0011 finding 5's headline answer was "the trait does not
    // change"; this is what holds it to that. (Adding a method to `Renderer` is a
    // legitimate thing to do -- it just must be a deliberate, reviewed change to
    // this list, and it must not be driven by one platform's convenience.)
    let seam = source("crates/renderer/src/lib.rs");
    let trait_body = between(&seam, "pub trait Renderer {", "\n}\n");
    let declared: Vec<String> = trait_body
        .lines()
        .filter_map(|line| line.trim().strip_prefix("fn "))
        .filter_map(|rest| rest.split(['(', '<']).next())
        .map(str::to_string)
        .collect();
    let expected = [
        "navigate",
        "reload",
        "stop",
        "go_back",
        "go_forward",
        "can_go_back",
        "can_go_forward",
        "load_state",
        "current_url",
        "poll_event",
        "view_handle",
        "send_pointer",
        "send_key",
        "send_scroll",
        "set_focus",
        "register_script_message_handler",
        "inject_script",
        "evaluate_javascript",
        "register_scheme_handler",
        "trust_hooks",
        "trust_posture",
        "mark_ens_origin",
        "mark_mutable_name",
    ];
    assert_eq!(
        declared, expected,
        "the `Renderer` seam must be unchanged: a new backend plugs into the trait, it does not \
         widen it"
    );
}

#[test]
fn the_windows_backend_crate_depends_on_no_toolkit_and_consumes_the_shared_half() {
    // Criterion 2: the backend cannot live in a crate that depends on
    // gtk4/webkit6 unconditionally, and it must not drag them in itself.
    let manifest = source("crates/windows-renderer/Cargo.toml");
    for toolkit in ["gtk4", "webkit6", "objc2"] {
        assert!(
            !code_only(&manifest).contains(toolkit),
            "the Windows backend crate must not depend on `{toolkit}`"
        );
    }
    // The WebView2 bindings are TARGET-GATED, so the Ubuntu gate builds this
    // crate (and its pure half) without a Windows SDK.
    let windows_deps = manifest
        .split("[target.'cfg(windows)'.dependencies]")
        .nth(1)
        .expect("the WebView2 bindings must be a `cfg(windows)` dependency block");
    for binding in ["webview2-com", "windows"] {
        assert!(
            windows_deps.contains(binding),
            "`{binding}` must be a Windows-only dependency"
        );
    }
    // ADR-0011 finding 4: the maintained bindings, never the abandoned crate.
    assert!(
        windows_deps.contains("webview2-com = \"0.39.1\""),
        "the bindings must be `webview2-com` 0.39.1 (what `wry` itself depends on)"
    );
    assert!(
        !code_only(&manifest).contains("\nwebview2 ="),
        "the abandoned `webview2` crate (last released 2021, predates the custom-scheme API) \
         must never be a dependency"
    );
    assert!(
        !code_only(&manifest).contains("wry"),
        "`wry` was considered and REJECTED as the backend (ADR-0011 Considered options): it \
         forces the internal-localhost origin mapping the probe exists to avoid"
    );

    // The shared, toolkit-free crate is a real dependency -- CONSUMED, not copied.
    assert!(
        manifest.contains("webview-shared = { path = \"../webview-shared\" }"),
        "the Windows backend must depend on the shared toolkit-free crate"
    );
    let backend = backend();
    assert!(
        backend.contains("use webview_shared::offthread::{complete_ipfs_request"),
        "the Windows backend must use the SHARED off-thread boundary"
    );
    assert!(
        backend.contains("use webview_shared::{validate_url, LoadLifecycle, SharedLifecycle}"),
        "the Windows backend must use the SHARED lifecycle and navigate URL rule"
    );
    // ...and it re-defines NONE of it. Exactly one definition exists in the tree.
    for name in [
        "complete_ipfs_request",
        "retrieve_off_thread",
        "validate_url",
    ] {
        assert!(
            !backend.contains(&format!("fn {name}")),
            "the Windows backend must not re-define `{name}`: it is shared, not copied"
        );
    }
    // And it does not fork the lifecycle either.
    assert!(
        !backend.contains("struct LoadLifecycle"),
        "the Windows backend must not fork the shared load lifecycle"
    );
    // Nor the shared page-side shims: the ADAPTER is what is Windows-specific.
    assert!(
        backend.contains("provider_shim"),
        "the provider shim must be the SHARED core one, injected unchanged"
    );
}

#[test]
fn the_environment_is_created_lazily_so_schemes_can_be_registered_first() {
    // Criterion 3, and ADR-0011 finding 5's ONE structural constraint: the SET of
    // custom scheme NAMES is fixed at ENVIRONMENT creation and immutable for the
    // browser-process lifetime, while `register_scheme_handler` is called after
    // construction. The prescribed answer is a LAZY environment, NOT a trait
    // change (which `the_renderer_seam_was_not_widened_for_windows` separately
    // proves did not happen).
    let backend = backend();
    assert!(
        backend.contains("pub fn realize(&mut self)") && backend.contains("self.webview.is_some()"),
        "the environment and controller must be created LAZILY, so schemes can be registered \
         after construction"
    );
    assert!(
        between(&backend, "fn navigate(", "\n    }\n").contains("self.realize()?"),
        "the first `navigate` is what realises the environment"
    );
    // The container HWND is EAGER, which is the other half of the same answer:
    // `view_handle` must work before the engine exists.
    assert!(
        between(&backend, "fn view_handle(", "\n    }\n").contains("self.container"),
        "the container HWND must be EAGER so `view_handle` works before realisation"
    );
    assert!(
        between(&backend, "pub fn with_user_data_folder(", "\n    }\n")
            .contains("create_container_window()"),
        "the container window must be created at CONSTRUCTION, not at realisation"
    );
    // The scheme names really are what the environment is created with.
    let create_environment = between(&backend, "fn create_environment(", "\n    }\n");
    for api in [
        "CoreWebView2CustomSchemeRegistration::new",
        "set_has_authority_component",
        "set_treat_as_secure",
        "set_scheme_registrations",
    ] {
        assert!(
            create_environment.contains(api),
            "the environment must register the custom schemes through `{api}`"
        );
    }
    // The measured flags, not hand-rolled booleans: `pure.rs` pins them and the
    // Ubuntu gate asserts them.
    assert!(
        create_environment.contains("SCHEME_HAS_AUTHORITY_COMPONENT")
            && create_environment.contains("SCHEME_TREAT_AS_SECURE"),
        "the registration flags must be the pinned, MEASURED ones (ADR-0011 Amendment 2)"
    );
    // A scheme registered too late cannot be silently swallowed.
    assert!(
        between(&backend, "fn add_route(", "\n    }\n").contains("eprintln!"),
        "a scheme registered AFTER realisation must be reported, not silently ignored"
    );
}

#[test]
fn the_load_lifecycle_history_bridge_and_scheme_hook_are_really_wired() {
    // Criterion 5: each of these goes through the seam onto a REAL WebView2 API,
    // not a placeholder.
    let backend = backend();

    // The LOAD LIFECYCLE: WebView2's own navigation events drive the SHARED
    // lifecycle.
    for event in [
        "add_NavigationStarting",
        "add_ContentLoading",
        "add_NavigationCompleted",
    ] {
        assert!(
            backend.contains(event),
            "the load lifecycle must come from the real `{event}`"
        );
    }
    for transition in ["life.begin(", "life.commit(", "life.finish(", "life.fail("] {
        assert!(
            backend.contains(transition),
            "the platform events must drive the shared lifecycle's `{transition}`"
        );
    }

    // SAME-DOCUMENT (SPA) URL tracking: NATIVE here. `IsNewDocument == FALSE` IS
    // the same-document change -- this row must NOT be inferred, as the three
    // other edges have to.
    assert!(
        backend.contains("add_SourceChanged")
            && backend.contains("IsNewDocument")
            && backend.contains(".url_changed("),
        "an SPA pushState must surface `LoadEvent::UrlChanged` from `add_SourceChanged` with \
         `IsNewDocument == FALSE`, not from an inference"
    );

    // HISTORY is WebView2's, driven through the seam (no URL stack of our own).
    for native in ["CanGoBack(", "CanGoForward(", "GoBack()", "GoForward()"] {
        assert!(
            backend.contains(native),
            "session history must be WebView2's own `{native}`"
        );
    }

    // The SCRIPT-MESSAGE BRIDGE: a real `WebMessageReceived` channel, real
    // document-start injection, and a real browser -> page response push.
    for api in [
        "add_WebMessageReceived",
        "TryGetWebMessageAsString",
        "AddScriptToExecuteOnDocumentCreated",
        "ExecuteScript",
    ] {
        assert!(
            backend.contains(api),
            "the script-message bridge must be the real `{api}`"
        );
    }
    // The bridge NAME travels in the envelope, because WebView2 has one channel.
    assert!(
        backend.contains("parse_bridge_envelope") && backend.contains("bridge_adapter_script"),
        "the named-channel shape the SHARED shims post to must be supplied by an adapter"
    );

    // CUSTOM-SCHEME INTERCEPTION: a real registered scheme plus a real handler.
    for api in [
        "AddWebResourceRequestedFilter",
        "add_WebResourceRequested",
        "CreateWebResourceResponse",
        "SetResponse",
    ] {
        assert!(
            backend.contains(api),
            "custom-scheme interception must be the real `{api}`"
        );
    }
    // The honest per-request STATUS travels with the bytes (the `_redirects`
    // site-404 case), rather than every answer claiming 200.
    assert!(
        backend.contains("i32::from(response.status)") && backend.contains("reason_phrase("),
        "an intercepted response must carry the seam's honest status, not a fabricated 200"
    );

    // The new-window hook (`docs/adr/0010`) feeds the SHARED rule, and never
    // opens a second window.
    assert!(
        backend.contains("add_NewWindowRequested")
            && backend.contains("new_window_action(")
            && backend.contains("args.SetHandled(true)"),
        "a `_blank` link must navigate IN PLACE through the shared rule, with the request marked \
         handled so WebView2 opens no second window"
    );

    // ADR-0009: FOLLOW the OS color scheme, never force dark.
    assert!(
        backend.contains("COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO"),
        "the engine must follow the OS color scheme (AUTO), never force dark"
    );
}

#[test]
fn both_trust_hooks_are_really_wired_never_silent_no_ops() {
    // Criterion 4, and `docs/adr/0005`'s rule: a seam method that is an empty
    // no-op is how a capability silently ships on one platform only.
    let backend = backend();

    // HOOK 1 -- EIP-1193 provider injection, over the SHARED core path (never a
    // Windows-local provider).
    let provider = between(&backend, "pub fn install_provider(", "\n    }\n");
    for shared in [
        "werust_core::provider",
        "provider_shim",
        "route_provider_message",
        "ProviderBridge",
        "PROVIDER_BRIDGE",
    ] {
        assert!(
            provider.contains(shared),
            "the provider hook must route through the shared core's `{shared}`"
        );
    }

    // HOOK 2 -- `ipfs://`, over the SHARED verifying core path (never a
    // Windows-local resolver, never an unverified fetch).
    let ipfs = between(&backend, "pub fn install_ipfs(", "\n    }\n");
    for shared in [
        "TrustlessGatewayCarRetriever",
        "IPFS_SCHEME",
        "retrieve_off_thread",
        "RedirectSink",
    ] {
        assert!(
            ipfs.contains(shared),
            "the `ipfs://` hook must route through the shared verifying path's `{shared}`"
        );
    }

    // The backend OPTS IN to both hooks (the seam default is fail-closed).
    assert!(
        between(&backend, "fn trust_hooks(", "\n    }\n").contains("TrustHooks::all()"),
        "a backend that wires both hooks must declare both"
    );

    // The trust posture tracks the ACTUAL load path: it is read from the shared
    // lifecycle, and marked ONLY by the shared completion rule.
    assert!(
        between(&backend, "fn trust_posture(", "\n    }\n")
            .contains("self.life.borrow().posture()"),
        "the posture must be the shared lifecycle's, never inferred from the URL"
    );
    assert!(
        backend.contains("complete_ipfs_request(outcome, &mut sink, &self.life)"),
        "the verified mark must come from the ONE shared completion rule"
    );
    // The SYNCHRONOUS seam route must NOT mark anything verified -- only the
    // verifying route may (the same split the other two backends have).
    let sync_route = between(&backend, "Route::Sync(handler) => {", "Route::OffThread");
    assert!(
        !sync_route.contains("mark_content_verified")
            && !sync_route.contains("complete_ipfs_request"),
        "the plain seam scheme route must never mark a load content-verified"
    );
    // FAIL CLOSED: a refused resolution sets NO response, so not one unverified
    // byte reaches the engine.
    let fail = between(
        &backend,
        "fn fail(&mut self, error: RendererError)",
        "\n    }\n",
    );
    assert!(
        !fail.contains("SetResponse"),
        "a failed resolution must set NO response: unverified bytes must never render"
    );

    // No silent no-op: none of the four hook-carrying seam methods is an empty
    // body.
    let seam_impl = between(&backend, "impl Renderer for Webview2Renderer", "\n}\n");
    for method in [
        "fn register_scheme_handler(",
        "fn register_script_message_handler(",
        "fn inject_script(",
        "fn evaluate_javascript(",
    ] {
        let body = between(seam_impl, method, "\n    }\n");
        assert!(
            body.lines().count() > 3,
            "`{method}` must be really wired, not a silent no-op"
        );
    }
}

#[test]
fn the_blocking_verify_runs_off_the_message_loop_thread_and_completes_on_it() {
    // `docs/adr/0008`: the CAR fetch + per-block verify must not block the UI
    // thread, and the shared (`!Send`) lifecycle must be mutated only on the
    // marshalling thread. On WebView2 the mechanism is the deferral ADR-0011's
    // mapping named.
    let backend = backend();
    assert!(
        backend.contains("args.GetDeferral()") && backend.contains("deferral.Complete()"),
        "the off-thread answer must be held open by a WebView2 deferral"
    );
    assert!(
        backend.contains("std::thread::spawn(move || {"),
        "the blocking retrieval must run on a worker thread"
    );
    let worker = between(&backend, "std::thread::spawn(move || {", "});");
    for forbidden in ["life", "borrow_mut", "args", "deferral"] {
        assert!(
            !worker.contains(forbidden),
            "nothing `!Send` (`{forbidden}`) may cross to the worker: only the `Send` outcome does"
        );
    }
    assert!(
        backend.contains("fn drain_completions(&self)")
            && backend.contains("pub fn pump_scheme_completions(&mut self)"),
        "completions must be applied on the message-loop thread by an explicit pump"
    );
    assert!(
        between(&backend, "fn poll_event(", "\n    }\n").contains("self.pump_scheme_completions()"),
        "draining the seam must also apply the off-thread completions, so a shell needs no extra \
         wiring"
    );
}

#[test]
fn a_machine_without_the_webview2_runtime_fails_honestly() {
    // Criterion 6, a pre-specified user-visible behaviour (ADR-0011 finding 6).
    // The MESSAGE itself is unit-tested in `pure.rs` on the Ubuntu gate -- which
    // matters, because a `windows-latest` runner HAS the runtime and can never
    // exercise this path. What is asserted here is that the wiring really routes
    // through it, and that it is an error rather than a panic.
    let backend = backend();
    assert!(
        backend.contains("GetAvailableCoreWebView2BrowserVersionString"),
        "the runtime presence check must be Microsoft's own API"
    );
    let check = between(&backend, "fn runtime_version() -> Result<String", "\n}\n");
    assert!(
        check.contains("missing_runtime_error"),
        "a missing runtime must produce the honest, NAMED error"
    );
    for panicky in ["unwrap()", "expect(", "panic!"] {
        assert!(
            !check.contains(panicky),
            "the runtime check must never `{panicky}`: a machine without the runtime must get an \
             honest failure, not a crash"
        );
    }
    // Construction checks it, so a shell learns the truth before it opens a
    // window it cannot fill; environment creation maps its refusal through the
    // same message, so a runtime that disappears in between is still honest.
    assert!(
        between(&backend, "pub fn with_user_data_folder(", "\n    }\n")
            .contains("runtime_version()?"),
        "construction must check for the runtime"
    );
    assert!(
        between(&backend, "fn create_environment(", "\n    }\n").contains("missing_runtime_error"),
        "an environment refusal must be reported with the same honest, named message"
    );
    // And the whole engine half must be free of panicking unwraps: this is the
    // ONE platform whose runtime can genuinely be absent.
    assert!(
        !code_only(&backend).contains(".unwrap()"),
        "the Windows backend must not `.unwrap()`: every platform failure travels as a \
         `RendererError`"
    );
}

#[test]
fn this_task_built_no_chrome() {
    // The scope boundary: the window, URL bar, trust indicator, menus and debug
    // view are the sibling task `windows-win32-window-and-chrome`.
    let backend = backend();
    for chrome in [
        "ChromeState",
        "status_line",
        "trust_indicator",
        "error_banner",
        "CreateMenu",
        "EDIT",
        "BUTTON",
    ] {
        assert!(
            !backend.contains(chrome),
            "this task builds the ENGINE only: `{chrome}` belongs to \
             `windows-win32-window-and-chrome`"
        );
    }
    // A bare host window is allowed, and is named as such.
    assert!(
        backend.contains("pub fn host_in_bare_window(&mut self)"),
        "a bare/offscreen host window is expected, so the engine can be RUN"
    );
}

#[test]
fn the_ci_smoke_drives_both_trust_hooks_with_a_negative_control() {
    // Criterion 4, end to end on a real WebView2: the CI smoke must exercise BOTH
    // hooks and must be able to FAIL.
    let smoke = source("crates/windows-renderer/examples/trust_hooks_smoke.rs");
    assert!(
        smoke.contains("install_verifying_scheme") && smoke.contains("retrieve_off_thread"),
        "the smoke must drive the REAL verifying path, not a canned response"
    );
    assert!(
        smoke.contains("install_provider") && smoke.contains("window.ethereum"),
        "the smoke must assert the page sees the native EIP-1193 provider"
    );
    assert!(
        smoke.contains("eth_chainId"),
        "the smoke must assert a provider request ROUND-TRIPS, not merely that the shim exists"
    );
    assert!(
        smoke.contains("TrustPosture::ContentVerified"),
        "the smoke must assert the verified load reports the verified posture"
    );
    // The Amendment 2 verdict, observed from inside the BACKEND: a registered
    // scheme gives the document its real tuple origin.
    assert!(
        smoke.contains("\\\"origin\\\":\\\"ipfs://"),
        "the smoke must assert the document got its REAL `ipfs://<cid>` tuple origin"
    );
    // The negative control: bytes that do not hash to their CID must fail the
    // load and never be reported verified.
    assert!(
        smoke.contains("negative control") || smoke.contains("NEGATIVE CONTROL"),
        "the smoke must carry a negative control"
    );
    assert!(
        smoke.contains("TrustPosture::UnverifiedOrigin") && smoke.contains("LoadState::Failed"),
        "the control must assert the fail-closed outcome"
    );
    // No network: a smoke that needs a gateway is not a CI smoke.
    for networked in ["https://", "http://"] {
        assert!(
            !smoke.contains(&format!("navigate(\"{networked}")),
            "the smoke must stay offline"
        );
    }
}

#[test]
fn the_windows_ci_leg_builds_tests_and_exercises_this_crate() {
    // Criterion 7: the leg that already exists on `main` (task
    // `windows-renderer-ci-leg`, landed FIRST precisely so this task could be
    // MEASURED rather than predicted) must now build, test and RUN this crate.
    let workflow = source(".github/workflows/windows-renderer.yml");
    assert!(
        workflow.contains("runs-on: windows-latest"),
        "the job must run on a `windows-latest` runner"
    );
    assert!(
        workflow.contains("cargo build -p windows-renderer")
            || workflow.contains("cargo build") && workflow.contains("-p windows-renderer"),
        "the job must BUILD the Windows backend"
    );
    assert!(
        workflow.contains("cargo test") && workflow.contains("-p windows-renderer"),
        "the job must run the backend's tests on Windows"
    );
    assert!(
        workflow.contains("--example trust_hooks_smoke"),
        "the job must EXERCISE both trust hooks on a real WebView2"
    );
    // The coupling `crates/werust-core/tests/windows_renderer_leg_shape.rs` pins:
    // every crate the leg builds must also be in its `push` path filter.
    assert!(
        workflow.contains("crates/windows-renderer/**"),
        "the leg's path filter must cover this crate"
    );
}

#[test]
fn the_verification_honesty_is_recorded() {
    // Criterion 8: what CI PROVED versus what still awaits real Windows hardware,
    // written down at the task's stable spike path (ADR-0011 Amendment 1's
    // requirement, and the exact thing both macOS tasks were bounced for).
    let readme = source("docs/spikes/windows-webview2-renderer-backend/README.md");
    for section in ["What CI proved", "What still awaits"] {
        assert!(
            readme.contains(section),
            "the spike README must state `{section}`"
        );
    }
    // A prediction cannot pass for a measurement: the README must link the actual
    // run, and the verbatim log must ship with it.
    assert!(
        readme.contains("actions/runs/"),
        "the README must link the CI run its measured claims come from"
    );
    assert!(
        exists("docs/spikes/windows-webview2-renderer-backend/trust-hooks-smoke-2026-07-30.txt"),
        "the measured claims must ship with the verbatim run output they were stamped from"
    );
    // The origin question is SETTLED by measurement and must not be re-litigated
    // here: the README must point at the probe's verdict rather than re-deriving.
    assert!(
        readme.contains("windows-ipfs-origin-probe-on-ci"),
        "the README must defer the origin verdict to the probe that measured it"
    );
}
