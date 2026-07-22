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

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque browsing session (a Rust `CoreSession`). Threaded through every call. */
typedef struct WerustCoreSession WerustCoreSession;

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

/* Report the WKWebView's real load-lifecycle signals back into the core (from the
 * WKNavigationDelegate). `url` / `reason` are borrowed C strings. */
void werust_ios_on_page_committed(WerustCoreSession *session, const char *url);
void werust_ios_on_page_finished(WerustCoreSession *session, const char *url);
void werust_ios_on_page_failed(WerustCoreSession *session, const char *url,
                               const char *reason);

/* The current chrome as a heap C string (JSON: url / loadState / loading /
 * canGoBack / canGoForward / error), for Swift to paint the URL bar, nav-control
 * enablement, and status line. Free with werust_ios_string_free. */
char *werust_ios_chrome_json(WerustCoreSession *session);

#ifdef __cplusplus
}
#endif

#endif /* WERUST_MOBILE_H */
