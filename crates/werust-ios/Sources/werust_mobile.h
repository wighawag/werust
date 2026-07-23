/*
 * C-ABI surface for the werust iOS mobile static library
 * (crates/werust-ios/rust, lib `werust_mobile`).
 *
 * This is the bridging header the iOS Swift shell imports (as the project's
 * `SWIFT_OBJC_BRIDGING_HEADER`) to call the Rust `extern "C"` exports and drive
 * the shared werust core (the `werust-core` crate) over the `Renderer` seam.
 * Keep these declarations in lock-step with the `#[no_mangle] extern "C"` fns in
 * `crates/werust-ios/rust/src/lib.rs`.
 *
 * The protocol (twin of the Android JNI shim, swapping JNI for a plain C-ABI):
 * one opaque `WerustCoreSession *` per view controller. On a user action Swift
 * drives the session, then reads back the pending-load URL (to feed WKWebView)
 * and the chrome JSON (to paint the URL bar / nav enablement / status). Swift
 * reports the WKWebView's real load-lifecycle signals back in. The session owns
 * ALL browsing logic; Swift stays confined to the OS edge.
 *
 * String ownership: input `const char *` are borrowed (Swift owns them for the
 * call). Returned `char *` are heap-allocated by Rust and MUST be released with
 * `werust_ios_string_free`; a NULL return means "nothing" (no pending load / a
 * null session handle).
 */
#ifndef WERUST_MOBILE_H
#define WERUST_MOBILE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque browsing session (a Rust `CoreSession`). Threaded through every call. */
typedef struct WerustCoreSession WerustCoreSession;

/* Opaque resolved `ipfs://` request (a Rust `SchemeResolution`): the verified
 * bytes + MIME type, or a fail-closed reason. Produced by
 * werust_ios_resolve_ipfs; queried via the _resolution_* accessors; freed with
 * werust_ios_resolution_free. */
typedef struct WerustSchemeResolution WerustSchemeResolution;

/* Create a fresh browsing session; free with werust_ios_session_free. */
WerustCoreSession *werust_ios_session_new(void);

/* Free a session created by werust_ios_session_new (null tolerated). */
void werust_ios_session_free(WerustCoreSession *session);

/* Free a char* returned by werust_ios_take_pending_load / _chrome_json. */
void werust_ios_string_free(char *s);

/* Navigate to `url` (URL-bar Enter). Returns true on success, false if rejected
 * (an unusable URL leaves the chrome untouched for the user to fix). */
bool werust_ios_navigate(WerustCoreSession *session, const char *url);

/* Back / Forward / Reload / Stop, THROUGH the core's session history + seam. */
void werust_ios_go_back(WerustCoreSession *session);
void werust_ios_go_forward(WerustCoreSession *session);
bool werust_ios_reload(WerustCoreSession *session);
void werust_ios_stop(WerustCoreSession *session);

/* The URL the core committed to but the WKWebView has not yet loaded, as a heap
 * C string (free with werust_ios_string_free), or NULL if nothing is pending.
 * Swift drains this after driving the core and calls WKWebView.load with it. */
char *werust_ios_take_pending_load(WerustCoreSession *session);

/* Resolve an intercepted `ipfs://<cid>[/path]` request through the SHARED core
 * resolve path (the same hash-verified path desktop uses). The WKWebView loads
 * `ipfs://` only via a WKURLSchemeHandler, so Swift's handler calls this with
 * the intercepted URL and answers the WKURLSchemeTask from the result. Returns
 * an opaque resolution handle (query via the accessors below, free with
 * werust_ios_resolution_free), or NULL if the URL is not an intercepted scheme
 * (Swift then lets the WKWebView handle it normally). */
WerustSchemeResolution *werust_ios_resolve_ipfs(WerustCoreSession *session,
                                                const char *uri);

/* Serve (and apply) an intercepted `werust://settings[?backend=...]` request
 * through the SHARED core settings path (the same retrieval-backend settings page
 * desktop + Android serve). The WKWebView loads `werust://` only via a
 * WKURLSchemeHandler, so Swift's handler for the `werust` scheme calls this with
 * the intercepted URL and answers the WKURLSchemeTask from the result: the page
 * HTML + MIME on success, a fail-closed error (a non-`settings` host) whose reason
 * is werust_ios_resolution_error. A `?backend=<kind>[&url=...]` selection is
 * persisted by the shared core. Returns an opaque resolution handle (queried via
 * the same _resolution_* accessors as the ipfs path, freed with
 * werust_ios_resolution_free), or NULL if the URL is not the `werust` scheme. */
WerustSchemeResolution *werust_ios_apply_settings(WerustCoreSession *session,
                                                  const char *uri);

/* True iff the resolution is a verified success (bytes to render); false is a
 * fail-closed error (a hash mismatch / unverifiable CID / source error) whose
 * reason is werust_ios_resolution_error — fail the WKURLSchemeTask, never render. */
bool werust_ios_resolution_is_ok(const WerustSchemeResolution *resolution);

/* The MIME type of a successful resolution, as a heap C string (empty on an
 * error result). Free with werust_ios_string_free. */
char *werust_ios_resolution_mime(const WerustSchemeResolution *resolution);

/* The verified body bytes of a successful resolution (NULL / 0 length on an
 * error result). The bytes are owned by the resolution and valid until
 * werust_ios_resolution_free; copy them into Data before freeing. Pair with
 * werust_ios_resolution_body_len. */
const uint8_t *werust_ios_resolution_body(const WerustSchemeResolution *resolution);
size_t werust_ios_resolution_body_len(const WerustSchemeResolution *resolution);

/* The fail-closed reason of an error resolution, as a heap C string (empty on
 * success). Free with werust_ios_string_free. */
char *werust_ios_resolution_error(const WerustSchemeResolution *resolution);

/* Free a resolution handle from werust_ios_resolve_ipfs (NULL tolerated). */
void werust_ios_resolution_free(WerustSchemeResolution *resolution);

/* The document-start scripts (the EIP-1193 provider shim) as a single heap C
 * string Swift installs onto the WKWebView as a WKUserScript at document start,
 * so a page's `window.ethereum` is the injected native provider (routed through
 * the SAME werust-core provider path desktop uses). Free with
 * werust_ios_string_free; empty string means nothing to inject. */
char *werust_ios_document_start_script(WerustCoreSession *session);

/* Dispatch an EIP-1193 envelope a page posted on the provider channel `name`
 * through the shared werust-core provider path and return the response JS Swift
 * runs in the live page (via WKWebView.evaluateJavaScript) to settle the page's
 * pending Promise, as a single heap C string (empty means nothing to run). Free
 * with werust_ios_string_free. This is the page -> native -> page provider
 * round-trip on iOS, called from the WKScriptMessageHandler. */
char *werust_ios_handle_provider_message(WerustCoreSession *session,
                                         const char *name, const char *body);

/* Report the WKWebView's real load-lifecycle signals back into the core (from the
 * WKNavigationDelegate). `url` / `reason` are borrowed C strings. */
void werust_ios_on_page_committed(WerustCoreSession *session, const char *url);
void werust_ios_on_page_finished(WerustCoreSession *session, const char *url);
void werust_ios_on_page_failed(WerustCoreSession *session, const char *url,
                               const char *reason);

/* The current chrome as a heap C string (JSON: url / loadState / loading /
 * canGoBack / canGoForward / trustPosture / error), for Swift to paint the URL
 * bar, nav-control enablement, status line, and the trust indicator. Free with
 * werust_ios_string_free. */
char *werust_ios_chrome_json(WerustCoreSession *session);

#ifdef __cplusplus
}
#endif

#endif /* WERUST_MOBILE_H */
