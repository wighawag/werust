//! macOS WKWebView backend SHAPE guard (task `macos-wkwebview-renderer-backend`,
//! `docs/adr/0011-webview2-for-windows.md`'s macOS split, sub-task 2).
//!
//! WHY A SOURCE-SHAPE GUARD: the backend's engine half is
//! `#[cfg(target_os = "macos")]`, and this repo's `verify` gate runs on Ubuntu
//! with no Xcode and no SDK, so `cargo build` NEVER compiles it. That is the same
//! position `crates/windows-origin-probe` and the two mobile edges are in, and
//! the repo's answer is the same: a plain `cargo test` that PARSES the source it
//! cannot compile and asserts the properties compilation would not have proven
//! anyway --
//!
//! * that the seam was not WIDENED for macOS,
//! * that the toolkit-free `offthread.rs` was **MOVED** to a shared home rather
//!   than COPIED (the criterion a compiler is blind to),
//! * that both TRUST HOOKS are really wired rather than silently no-op'd (the
//!   exact gap `docs/adr/0005` exists to forbid),
//! * that the blocking verify stays OFF the main thread (`docs/adr/0008`), and
//! * that this task built NO chrome.
//!
//! Everything that is a pure RULE rather than an SDK call lives in
//! `src/pure.rs` and is unit-tested normally; this guard covers the wiring.
//!
//! Acceptance criteria mapped to assertions below:
//! 1. A `Renderer` impl over WKWebView, with NO widening of the trait
//!    (`the_backend_implements_the_whole_seam_over_wkwebview`,
//!    `the_renderer_seam_was_not_widened_for_macos`).
//! 2. It does not live in a gtk4/webkit6 crate, and `offthread.rs` was MOVED
//!    (`offthread_moved_to_a_shared_toolkit_free_home_and_was_not_copied`,
//!    `the_macos_backend_crate_depends_on_no_toolkit`).
//! 3. Navigation, history, the load lifecycle, the script bridge and
//!    custom-scheme interception all go through the seam
//!    (`the_load_lifecycle_history_bridge_and_scheme_hook_are_really_wired`).
//! 4. Both trust hooks work (`both_trust_hooks_are_really_wired_never_silent_no_ops`),
//!    end-to-end on a real WKWebView in `examples/trust_hooks_smoke.rs`
//!    (`the_ci_smoke_drives_both_trust_hooks_with_a_negative_control`).
//! 5. The origin behaviour is confirmed at runtime by the sibling probe
//!    (`the_origin_probe_and_its_negative_control_exist`).
//! 6. A CI job on the existing `macos-14` runner builds and exercises it
//!    (`a_macos_14_ci_job_builds_and_exercises_the_backend`).
//! 7/8. What CI proved vs what awaits a Mac is written down, and the Ubuntu gate
//!    stays green (`the_verification_honesty_is_recorded`, plus this file being a
//!    plain `cargo test`).

use std::path::{Path, PathBuf};

/// The macOS leg's `pull_request` filter, ENTRY FOR ENTRY: the list the
/// workflow's header paragraph describes in prose.
///
/// Every entry is here because it is genuinely macOS-shaped (`macos-renderer`
/// and `werust-macos` hold `cfg(target_os = "macos")` halves that compile
/// nowhere else; `macos-origin-probe` is the WebKit origin measurement), because
/// it is the toolkit-free half this backend reuses verbatim (`webview-shared`),
/// because it is the SHARED painter both native desktop windows consume
/// (`desktop-paint` -- not macOS-shaped, but a break in the one carrier both
/// windows paint from is genuinely cross-platform and this leg's window smoke is
/// what catches it), or because it holds the RECORDED VERDICT this leg re-measures
/// WebKit against (the two spike directories).
///
/// Pinned as an EXACT set, in the shape of the sibling
/// `crates/werust-core/tests/windows_renderer_leg_shape.rs`'s
/// `PULL_REQUEST_FILTER` (task
/// `macos-harness-guard-teeth-and-paint-path-residue`, item 5). This filter has
/// drifted WIDER twice in three tasks with nothing going red, which is exactly
/// the accretion an exact pin ends: adding or removing a path must now be an edit
/// to this list, with the workflow's header paragraph updated in the same change.
const PULL_REQUEST_FILTER: &[&str] = &[
    "crates/macos-renderer/**",
    "crates/werust-macos/**",
    "crates/desktop-paint/**",
    "crates/macos-origin-probe/**",
    "crates/webview-shared/**",
    "docs/spikes/macos-wkwebview-renderer-backend/**",
    "docs/spikes/macos-appkit-window-and-chrome/**",
    ".github/workflows/macos-renderer.yml",
];

/// The wider DEPENDENCY surface: built by the leg, deliberately NOT on the
/// `pull_request` filter, and therefore watched on `push` to `main` instead.
///
/// `crates/werust-core/**` joined this list with task
/// `macos-harness-guard-teeth-and-paint-path-residue`, which answered the open
/// question the previous task left standing: it had been on the PR filter since
/// this leg landed, so most core work in this repo spent `macos-14` minutes and
/// could be gated by a red cross-platform leg. The Windows sibling had already
/// refused that cost, and two legs answering the same question differently is a
/// difference nobody can act on.
const PUSH_ONLY_DEPENDENCY_SURFACE: &[&str] = &[
    "crates/werust-core/**",
    "crates/renderer/**",
    "crates/fetcher/**",
];

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/macos-renderer`, so the root is two levels up.
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

/// The `paths:` entries of one `on:` trigger, read as whole list ITEMS.
///
/// Item-wise rather than by substring, because the filters in this workflow are
/// surrounded by prose comments that NAME paths (including paths deliberately
/// kept OFF the list they sit next to), and a substring search would happily read
/// the explanation as the trigger.
fn trigger_paths(workflow: &str, from: &str, to: &str) -> Vec<String> {
    between(workflow, from, to)
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- \""))
        .filter_map(|entry| entry.strip_suffix('"'))
        .map(str::to_string)
        .collect()
}

fn backend() -> String {
    source("crates/macos-renderer/src/backend.rs")
}

/// `source` with every comment line dropped, so a "does this file mention X"
/// assertion is about the CODE and not about prose. The shared crate's docs
/// legitimately DESCRIBE the GTK history it was extracted from; what must not
/// appear is a GTK/WebKit/ObjC dependency in the code itself.
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
fn the_backend_implements_the_whole_seam_over_wkwebview() {
    // Criterion 1: a real `Renderer` implementation, over the platform WKWebView
    // (not a stub, not a second toolkit).
    let backend = backend();
    assert!(
        backend.contains("impl Renderer for MacosRenderer"),
        "the macOS backend must implement the `Renderer` seam"
    );
    assert!(
        backend.contains("use objc2_web_kit::") && backend.contains("WKWebView"),
        "the backend must bind the platform WKWebView"
    );
    // Every seam method the browser drives, present in the impl block.
    let seam_impl = between(&backend, "impl Renderer for MacosRenderer", "\n}\n");
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
            "the macOS backend must implement the seam's `{method}`"
        );
    }
}

#[test]
fn the_renderer_seam_was_not_widened_for_macos() {
    // Criterion 1, the other half: adding a platform must NOT add a method to the
    // shared trait. The seam's method set is PINNED here, so a later macOS-driven
    // widening reds this test instead of quietly changing every backend's
    // obligations. (Adding a method to `Renderer` is a legitimate thing to do --
    // it just must be a deliberate, reviewed change to this list.)
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
fn offthread_moved_to_a_shared_toolkit_free_home_and_was_not_copied() {
    // Criterion 2: MOVED, not copied. The old home must be GONE, the new home
    // must hold it, and both backends must consume the SAME module.
    assert!(
        !exists("crates/webview-renderer/src/offthread.rs"),
        "`offthread.rs` must have MOVED out of the gtk4/webkit6 crate, not been copied from it"
    );
    assert!(
        exists("crates/webview-shared/src/offthread.rs"),
        "`offthread.rs` must live in the shared toolkit-free crate"
    );

    let shared = source("crates/webview-shared/src/offthread.rs");
    let shared_code = code_only(&shared);
    // The shared home is genuinely TOOLKIT-FREE: it may import only the seam, the
    // fetcher, the core and its own crate. A gtk/webkit/objc import here would
    // mean the "shared" crate is not shared at all.
    for toolkit in ["gtk4", "webkit6", "objc2", "glib", "gio::"] {
        assert!(
            !shared_code.contains(toolkit),
            "the shared off-thread boundary must stay toolkit-free, found `{toolkit}`"
        );
    }
    let lifecycle = code_only(&source("crates/webview-shared/src/lifecycle.rs"));
    for toolkit in ["gtk4", "webkit6", "objc2", "glib"] {
        assert!(
            !lifecycle.contains(toolkit),
            "the shared load lifecycle must stay toolkit-free, found `{toolkit}`"
        );
    }

    // BOTH desktop backends consume the shared module...
    let gtk_backend = source("crates/webview-renderer/src/backend.rs");
    assert!(
        gtk_backend.contains("webview_shared::offthread"),
        "the WebKitGTK backend must use the SHARED off-thread boundary"
    );
    assert!(
        backend().contains("webview_shared::offthread"),
        "the macOS backend must use the SHARED off-thread boundary"
    );
    // ...and neither re-defines it. Exactly one definition exists in the tree.
    let definitions = ["complete_ipfs_request", "retrieve_off_thread"];
    for name in definitions {
        let needle = format!("pub fn {name}");
        for copy in [
            "crates/webview-renderer/src/backend.rs",
            "crates/webview-renderer/src/lib.rs",
            "crates/macos-renderer/src/backend.rs",
        ] {
            assert!(
                !source(copy).contains(&needle),
                "{copy} must not re-define `{name}`: it is shared, not copied"
            );
        }
        assert!(
            shared.contains(&needle),
            "the shared crate must own `{name}`"
        );
    }

    // The shared crate also owns the ONE `validate_url` rule both backends apply,
    // for the same reason.
    assert!(
        source("crates/webview-shared/src/lib.rs").contains("pub fn validate_url"),
        "the shared crate must own the navigate URL rule"
    );
    assert!(
        !source("crates/webview-renderer/src/lib.rs").contains("fn validate_url(url: &str)"),
        "the WebKitGTK crate must not keep its own copy of the URL rule"
    );
}

#[test]
fn the_macos_backend_crate_depends_on_no_toolkit() {
    // Criterion 2: the backend cannot live in a crate that depends on
    // gtk4/webkit6 unconditionally, and it must not drag them in itself.
    let manifest = source("crates/macos-renderer/Cargo.toml");
    for toolkit in ["gtk4", "webkit6"] {
        assert!(
            !code_only(&manifest).contains(toolkit),
            "the macOS backend crate must not depend on `{toolkit}`"
        );
    }
    // The AppKit/WebKit bindings are TARGET-GATED, so the Ubuntu gate builds this
    // crate (and its pure half) without an SDK.
    let macos_deps = between(
        &manifest,
        "[target.'cfg(target_os = \"macos\")'.dependencies]",
        "\n[",
    );
    for binding in [
        "objc2",
        "objc2-foundation",
        "objc2-app-kit",
        "objc2-web-kit",
    ] {
        assert!(
            macos_deps.contains(binding),
            "`{binding}` must be a macOS-only dependency"
        );
    }
    // And the shared, toolkit-free crate is a real dependency, not a copy.
    assert!(
        manifest.contains("webview-shared = { path = \"../webview-shared\" }"),
        "the macOS backend must depend on the shared toolkit-free crate"
    );
    // The shared crate itself must stay toolkit-free.
    let shared_manifest = code_only(&source("crates/webview-shared/Cargo.toml"));
    for toolkit in ["gtk4", "webkit6", "objc2"] {
        assert!(
            !shared_manifest.contains(toolkit),
            "the shared crate must not depend on `{toolkit}`"
        );
    }
}

#[test]
fn the_load_lifecycle_history_bridge_and_scheme_hook_are_really_wired() {
    // Criterion 3: each of these goes through the seam onto a REAL WebKit API,
    // not a placeholder.
    let backend = backend();

    // The LOAD LIFECYCLE: WebKit's own navigation delegate drives the SHARED
    // lifecycle.
    assert!(
        backend.contains("unsafe impl WKNavigationDelegate for NavigationBridge"),
        "the load lifecycle must come from a real WKNavigationDelegate"
    );
    for callback in [
        "webView:didStartProvisionalNavigation:",
        "webView:didCommitNavigation:",
        "webView:didFinishNavigation:",
        "webView:didFailNavigation:withError:",
        "webView:didFailProvisionalNavigation:withError:",
    ] {
        assert!(
            backend.contains(callback),
            "the navigation delegate must observe `{callback}`"
        );
    }
    assert!(
        backend.contains("setNavigationDelegate"),
        "the delegate must actually be installed on the webview"
    );

    // SAME-DOCUMENT (SPA) URL tracking: the KVO observation on the webview's own
    // URL, feeding the lifecycle's `url_changed` -- never a faked load.
    assert!(
        backend.contains("observeValueForKeyPath:ofObject:change:context:")
            && backend.contains("addObserver_forKeyPath_options_context")
            && backend.contains(".url_changed("),
        "an SPA pushState must surface `LoadEvent::UrlChanged` via KVO on the webview URL"
    );

    // HISTORY is WebKit's, driven through the seam (no URL stack of our own).
    for native in ["canGoBack()", "canGoForward()", "goBack()", "goForward()"] {
        assert!(
            backend.contains(native),
            "session history must be WebKit's own `{native}`"
        );
    }

    // The SCRIPT-MESSAGE BRIDGE: a real WKScriptMessageHandler on the real
    // user-content controller, plus document-start user scripts.
    assert!(
        backend.contains("unsafe impl WKScriptMessageHandler for ScriptBridge")
            && backend.contains("addScriptMessageHandler_name")
            && backend.contains("userContentController:didReceiveScriptMessage:"),
        "the script-message bridge must be a real WKScriptMessageHandler"
    );
    assert!(
        backend.contains("WKUserScriptInjectionTime::AtDocumentStart")
            && backend.contains("addUserScript"),
        "injected scripts must run at document start, as on every other edge"
    );
    assert!(
        backend.contains("evaluateJavaScript_completionHandler"),
        "the browser -> page response push must evaluate JS in the live page"
    );

    // CUSTOM-SCHEME INTERCEPTION: a real WKURLSchemeHandler on the configuration.
    assert!(
        backend.contains("unsafe impl WKURLSchemeHandler for SchemeBridge")
            && backend.contains("setURLSchemeHandler_forURLScheme")
            && backend.contains("webView:startURLSchemeTask:")
            && backend.contains("webView:stopURLSchemeTask:"),
        "custom-scheme interception must be a real WKURLSchemeHandler"
    );
    // The scheme SET is fixed when the webview is constructed, so the engine is
    // built LAZILY -- ADR-0011 finding 5's prescribed answer, not a trait change.
    assert!(
        backend.contains("pub fn realize(&mut self)") && backend.contains("self.view.is_none()"),
        "the webview must be created lazily so schemes can be registered after construction"
    );
    assert!(
        backend.contains("ViewHandle(Retained::as_ptr(&self.container)"),
        "the container view must be EAGER so `view_handle` works before the engine exists"
    );

    // The honest per-request STATUS travels with the bytes (the `_redirects`
    // site-404 case), rather than every answer claiming 200.
    assert!(
        backend.contains("NSHTTPURLResponse") && backend.contains("response.status"),
        "an intercepted response must carry the seam's honest status, not a fabricated 200"
    );
}

#[test]
fn both_trust_hooks_are_really_wired_never_silent_no_ops() {
    // Criterion 4, and `docs/adr/0005`'s rule: a seam method that is an empty
    // no-op is how a capability silently ships on one platform only.
    let backend = backend();

    // HOOK 1 -- EIP-1193 provider injection, over the SHARED core path (never a
    // macOS-local provider).
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
    // macOS-local resolver, never an unverified fetch).
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
        backend.contains("complete_ipfs_request(outcome, &mut sink, &self.ivars().life)"),
        "the verified mark must come from the ONE shared completion rule"
    );
    // The SYNCHRONOUS seam route must NOT mark anything verified -- only the
    // verifying route may (the same split the WebKitGTK backend has).
    let sync_route = between(&backend, "Route::Sync(handler) => {", "Route::OffThread");
    assert!(
        !sync_route.contains("mark_content_verified")
            && !sync_route.contains("complete_ipfs_request"),
        "the plain seam scheme route must never mark a load content-verified"
    );

    // No silent no-op: none of the four hook-carrying seam methods is an empty
    // body.
    let seam_impl = between(&backend, "impl Renderer for MacosRenderer", "\n}\n");
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
fn the_blocking_verify_runs_off_the_main_thread_and_completes_on_it() {
    // `docs/adr/0008`: the CAR fetch + per-block verify must not block the UI
    // thread, and the shared (`!Send`) lifecycle must be mutated only on the
    // marshalling thread.
    let backend = backend();
    assert!(
        backend.contains("std::thread::spawn(move || {"),
        "the blocking retrieval must run on a worker thread"
    );
    let worker = between(&backend, "std::thread::spawn(move || {", "});");
    for forbidden in ["life", "borrow_mut", "task"] {
        assert!(
            !worker.contains(forbidden),
            "nothing `!Send` (`{forbidden}`) may cross to the worker: only the `Send` outcome does"
        );
    }
    assert!(
        backend.contains("fn drain_completions(&self)")
            && backend.contains("pub fn pump_scheme_completions(&mut self)"),
        "completions must be applied on the main thread by an explicit pump"
    );
    assert!(
        between(&backend, "fn poll_event(", "\n    }\n").contains("self.pump_scheme_completions()"),
        "draining the seam must also apply the off-thread completions, so a shell needs no extra \
         wiring"
    );
}

#[test]
fn this_task_built_no_chrome() {
    // The scope boundary: the window, URL bar, trust indicator, menus and debug
    // view are the sibling task `macos-appkit-window-and-chrome`.
    let backend = backend();
    for chrome in [
        "ChromeState",
        "status_line",
        "trust_indicator",
        "error_banner",
        "NSTextField",
        "NSButton",
        "NSMenu",
    ] {
        assert!(
            !backend.contains(chrome),
            "this task builds the ENGINE only: `{chrome}` belongs to `macos-appkit-window-and-chrome`"
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
    // Criterion 4, end to end on a real WKWebView: the CI smoke must exercise
    // BOTH hooks and must be able to FAIL.
    let smoke = source("crates/macos-renderer/examples/trust_hooks_smoke.rs");
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
fn the_origin_probe_and_its_negative_control_exist() {
    // Criterion 5: the WKURLSchemeHandler origin behaviour is MEASURED on macOS,
    // in the shape `crates/windows-origin-probe` established, negative control
    // included.
    assert!(
        exists("crates/macos-origin-probe/src/facts.rs")
            && exists("crates/macos-origin-probe/src/page.rs"),
        "the macOS origin probe must exist, in the Windows probe's shape"
    );
    let facts = source("crates/macos-origin-probe/src/facts.rs");
    assert!(
        facts.contains("Control"),
        "the probe must carry a NEGATIVE CONTROL, or a pass means nothing"
    );
    for measured in ["origin", "fetch", "fetch_handler_fired", "push_state"] {
        assert!(
            facts.contains(measured),
            "the probe must measure `{measured}`"
        );
    }
}

#[test]
fn a_macos_14_ci_job_builds_and_exercises_the_backend() {
    // Criterion 6: the EXISTING `macos-14` runner (the one the iOS leg already
    // uses), building and RUNNING the backend -- not merely compiling it.
    let workflow = source(".github/workflows/macos-renderer.yml");
    assert!(
        workflow.contains("runs-on: macos-14"),
        "the job must run on the existing macos-14 runner"
    );
    assert!(
        workflow.contains("cargo build -p macos-renderer"),
        "the job must BUILD the macOS backend"
    );
    assert!(
        workflow.contains("cargo test -p macos-renderer"),
        "the job must run the backend's tests on macOS"
    );
    assert!(
        workflow.contains("--example trust_hooks_smoke"),
        "the job must EXERCISE both trust hooks on a real WKWebView"
    );
    assert!(
        workflow.contains("macos-origin-probe"),
        "the job must run the origin probe"
    );
    // It must NOT try to build the GTK-bound workspace on macOS.
    assert!(
        !workflow.contains("cargo build\n") && !workflow.contains("cargo build --workspace"),
        "the macOS job must build only the macOS crates: the workspace is GTK-bound"
    );
}

#[test]
fn the_verification_honesty_is_recorded() {
    // Criterion 7: what CI PROVED versus what still awaits a Mac, written down
    // at the task's stable spike path (ADR-0011 Amendment 1's requirement).
    let readme = source("docs/spikes/macos-wkwebview-renderer-backend/README.md");
    for section in ["What CI proved", "What still awaits"] {
        assert!(
            readme.contains(section),
            "the spike README must state `{section}`"
        );
    }
    // Criterion 5: the origin behaviour is CONFIRMED AT RUNTIME, so the spike
    // must carry the verbatim run the verdict was stamped from -- the same shape
    // `windows-ipfs-origin-probe-on-ci` landed in. Prose alone cannot tell a
    // measurement from a prediction, which is exactly how a prediction once got
    // committed in the verdict's slot; `macos-origin-probe`'s
    // `tests/recorded_verdict.rs` then holds the two to each other.
    assert!(
        exists("docs/spikes/macos-wkwebview-renderer-backend/probe-report-2026-07-30.json"),
        "the measured verdict must ship with the verbatim run report it was stamped from"
    );
    assert!(
        readme.contains("actions/runs/"),
        "the README must link the CI run its measured claims come from"
    );
    // And the iOS mechanism-analysis caveat this work exists to retire must have
    // been updated to say what is now measured.
    let ios_caveat = source("docs/spikes/mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md");
    assert!(
        ios_caveat.contains("macos-wkwebview-renderer-backend"),
        "the iOS parity caveat must point at the macOS measurement that addresses it"
    );
}

#[test]
fn the_readme_claim_about_when_the_leg_runs_matches_the_pull_request_trigger() {
    // A doc that promises a trigger the workflow does not have is worse than no
    // doc at all: the README tells a reader the leg re-runs on a pull request
    // that changes "the backend, the probe or the recorded verdict", so a PR that
    // re-records `expected.json` after a deliberate re-decision would be reviewed
    // believing WebKit had just been re-measured against it. The recorded verdict
    // lives in this spike directory, so the `pull_request` path filter must cover
    // it -- the claim and the trigger are held to each other HERE, because prose
    // and YAML drift apart silently.
    let readme = source("docs/spikes/macos-wkwebview-renderer-backend/README.md");
    assert!(
        readme.contains("the backend, the probe or the recorded verdict changes"),
        "the README must state when the leg runs; if that sentence changes, change \
         this test and the workflow's path filter with it"
    );
    let workflow = source(".github/workflows/macos-renderer.yml");
    let pull_request = trigger_paths(&workflow, "  pull_request:", "\npermissions:");
    for claimed in [
        // "the backend"
        "crates/macos-renderer/**",
        "crates/werust-macos/**",
        // "the probe"
        "crates/macos-origin-probe/**",
        // "the recorded verdict" -- `expected.json` and the run it was stamped
        // from, which live beside this README.
        "docs/spikes/macos-wkwebview-renderer-backend/**",
        // Not claimed by that README sentence, and pinned here deliberately by
        // task `windows-backend-error-mapping-and-leg-header-accuracy`: the
        // SHARED painter both native desktop windows consume arrived on BOTH
        // legs' PR filters with `windows-win32-window-and-chrome`, with nothing
        // holding it. It is KEPT (a break in the one carrier both windows paint
        // from is genuinely cross-platform, and each leg's window smoke is what
        // catches it) and pinned, so the next widening of either filter is an
        // edit to a test rather than an accretion. The Windows half of the pin
        // is `crates/werust-core/tests/windows_renderer_leg_shape.rs`.
        "crates/desktop-paint/**",
    ] {
        assert!(
            pull_request.iter().any(|p| p == claimed),
            "the `pull_request` path filter must cover `{claimed}`: the spike README \
             claims a PR touching it re-runs the leg"
        );
    }
    // And no FURTHER than that: making the claim true is not licence to widen the
    // trigger. That half is `the_pull_request_filter_is_the_pinned_exact_set_and_push_carries_the_rest`
    // below, which pins the whole list rather than a must-have/must-not-have pair.
}

#[test]
fn the_pull_request_filter_is_the_pinned_exact_set_and_push_carries_the_rest() {
    // The DELIBERATE trade-off, stated in the workflow's header: this leg's PR
    // filter carries only what is genuinely macOS-shaped (plus the shared painter
    // and the recorded verdict), and the wider DEPENDENCY surface it also BUILDS
    // is watched on `push` to `main` instead -- early detection, on a leg that
    // gates nothing, plus `workflow_dispatch` for the deliberate case. Gating
    // every core pull request on `macos-14` minutes is the cost the sibling
    // `windows-renderer.yml` refused from the day it landed; this leg now answers
    // the same question the same way.
    //
    // Pinned as an EXACT set, in the sibling's shape
    // (`crates/werust-core/tests/windows_renderer_leg_shape.rs`), because a
    // must-have/must-not-have pair let this filter drift wider twice in three
    // tasks while the header went on describing the filter the file no longer had.
    let workflow = source(".github/workflows/macos-renderer.yml");
    let pull_request = trigger_paths(&workflow, "  pull_request:", "\npermissions:");
    let mut expected: Vec<&str> = PULL_REQUEST_FILTER.to_vec();
    expected.sort_unstable();
    let mut got: Vec<&str> = pull_request.iter().map(String::as_str).collect();
    got.sort_unstable();
    assert_eq!(
        got, expected,
        "the `pull_request` filter must be EXACTLY the pinned set. Widening it gates more pull \
         requests on a `macos-14` runner and narrowing it drops a macOS-shaped change from the \
         only leg that compiles it. Either way it is a decision, so change this list AND the \
         workflow's header paragraph, which describes it in prose"
    );

    let push = trigger_paths(&workflow, "  push:", "  pull_request:");
    for dependency_only in PUSH_ONLY_DEPENDENCY_SURFACE {
        assert!(
            !pull_request.iter().any(|p| p == dependency_only),
            "the `pull_request` filter must NOT carry `{dependency_only}`: it is the wider \
             dependency surface, watched on `push` to `main` instead. Adding it gates ordinary \
             core work on a cross-platform runner"
        );
        // ...but what the PR filter gives up must still be caught post-merge, or
        // the narrowness is a hole rather than a trade.
        assert!(
            push.iter().any(|p| p == dependency_only),
            "`{dependency_only}` must stay on the `push` filter: it is what the narrow \
             `pull_request` filter relies on to be caught minutes after a merge instead"
        );
    }
}
