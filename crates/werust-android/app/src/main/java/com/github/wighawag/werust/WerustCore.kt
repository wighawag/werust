package com.github.wighawag.werust

import org.json.JSONObject

/**
 * The Kotlin edge's thin binding to the werust **Rust core** (`libwerust_mobile.so`,
 * built from the `werust-core` / `werust-android-core` crates).
 *
 * This class holds NO browsing logic: it is a mechanical bridge over the JNI
 * surface the Rust core exports. Every browsing decision (whether Back is
 * available, what the URL bar shows, the load state) is the core's truth, read
 * back through [chrome]. The Activity drives the core through this class and
 * paints whatever the core reports; the platform `WebView` is fed the URL the
 * core surfaces via [takePendingLoad] and reports its real load signals back in
 * via [onPageCommitted] / [onPageFinished] / [onPageFailed].
 *
 * One instance per Activity; call [close] to free the native session.
 */
class WerustCore : AutoCloseable {
    /** Opaque pointer to the native `CoreSession`, threaded through every call. */
    private var handle: Long = nativeNew()

    /** Navigate to [url] (the URL bar's Enter action). Returns false if rejected. */
    fun navigate(url: String): Boolean = nativeNavigate(handle, url)

    /** Go one step back in the core's session history. */
    fun goBack() = nativeGoBack(handle)

    /** Go one step forward in the core's session history. */
    fun goForward() = nativeGoForward(handle)

    /** Reload the current page. Returns false if there is nothing to reload. */
    fun reload(): Boolean = nativeReload(handle)

    /** Stop the in-flight load. */
    fun stop() = nativeStop(handle)

    /**
     * The URL (if any) the core has committed to but the platform `WebView` has
     * not yet loaded. Empty string means "nothing pending".
     */
    fun takePendingLoad(): String? = nativeTakePendingLoad(handle).ifEmpty { null }

    /**
     * Resolve an intercepted `ipfs://<cid>[/path]` request through the SHARED
     * `werust-core` resolve path (the same hash-verified path desktop uses), for
     * the [android.webkit.WebViewClient.shouldInterceptRequest] hook.
     *
     * The platform `WebView` cannot load an `ipfs://` URL itself
     * (`net::ERR_UNKNOWN_URL_SCHEME`), so the `BrowserActivity`'s `WebViewClient`
     * intercepts the request and calls this. Returns the verified bytes + MIME
     * type on success, a fail-closed [Resolution.Error] with a legible reason
     * (a hash mismatch / unverifiable CID / source error, never rendered) on
     * failure, or `null` if the URL is not an intercepted scheme (let the
     * `WebView` handle it normally). The native resolution handle is queried and
     * freed here so no native memory leaks across the JNI boundary.
     */
    fun resolveIpfs(uri: String): Resolution? {
        val res = nativeResolveIpfs(handle, uri)
        if (res == 0L) return null
        try {
            return if (nativeResolutionIsOk(res)) {
                Resolution.Ok(
                    nativeResolutionMime(res),
                    nativeResolutionBody(res),
                    nativeResolutionStatus(res),
                )
            } else {
                Resolution.Error(nativeResolutionError(res))
            }
        } finally {
            nativeResolutionFree(res)
        }
    }

    /**
     * The document-start script (the EIP-1193 provider shim) to install onto the
     * platform `WebView` so a page's `window.ethereum` is the injected native
     * provider. Routed through the SAME `werust-core` provider path desktop uses.
     * Empty string means nothing to inject.
     */
    fun documentStartScript(): String = nativeDocumentStartScript(handle)

    /**
     * Dispatch an EIP-1193 envelope a page posted on the provider channel through
     * the shared `werust-core` provider path and return the response JS to run in
     * the live page (via [android.webkit.WebView.evaluateJavascript]) to settle
     * the page's pending Promise. Empty string means nothing to run. This is the
     * page -> native -> page provider round-trip on Android; it is called from the
     * `@JavascriptInterface` bridge (a WebView JS-interface thread), serialized by
     * the native `SyncSession` mutex against the UI thread exactly like
     * [resolveIpfs].
     */
    fun handleProviderMessage(name: String, body: String): String =
        nativeHandleProviderMessage(handle, name, body)

    /**
     * Report the platform `WebView`'s commit signal into the core. Runs on the
     * UI thread and returns OFF the native session lock (it records through the
     * backend's thread-safe clone handle), so it never waits on an in-flight
     * `ipfs://` retrieval — the ANR guard (task
     * `mobile-page-signal-callbacks-off-session-lock`). The chrome fold is
     * deferred to the next [chrome] / [takePendingLoad] read, which repaints it.
     */
    fun onPageCommitted(url: String) = nativeOnPageCommitted(handle, url)

    /**
     * Report the platform `WebView`'s finished signal into the core. Off the
     * native session lock, like [onPageCommitted].
     */
    fun onPageFinished(url: String) = nativeOnPageFinished(handle, url)

    /**
     * Report the platform `WebView`'s error signal into the core. Off the
     * native session lock, like [onPageCommitted].
     */
    fun onPageFailed(url: String, reason: String) = nativeOnPageFailed(handle, url, reason)

    /**
     * Report a SAME-DOCUMENT URL change (an SPA `pushState`/`replaceState`
     * client-side navigation) into the core, so the URL bar follows the new
     * location instead of freezing. Reported from
     * [android.webkit.WebViewClient.doUpdateVisitedHistory], which fires on
     * same-document history changes (no `onPageStarted`/`onPageFinished`).
     * Off the native session lock, like [onPageCommitted] — this is the signal
     * the v0.2.7 SPA-nav freeze pinned, so it must return in microseconds even
     * while the router's `__data.json` retrieval holds the worker-side lock.
     */
    fun onUrlChanged(url: String) = nativeOnUrlChanged(handle, url)

    /**
     * Map a core URL to the URL the platform `WebView` should load:
     * `ipfs://<cid>[/path]` -> the internal `https://<cid>.ipfs.werust.invalid`
     * origin, anything else unchanged. SESSION-FREE (a pure native function),
     * for the ONE call site that loads a URL the core did not surface as a
     * pending load: the `_blank`/`window.open` transport in `onCreateWindow`,
     * which hands its target to `WebView.loadUrl` directly. An unmapped
     * `ipfs://` main-frame load would land the page on the System WebView's
     * OPAQUE `ipfs://` origin, where Blink refuses `fetch(ipfs://…)` and
     * `pushState` throws — the mobile no-navigation root cause (task
     * `mobile-ronan-eth-buttons-no-navigation`).
     */
    fun toWebViewUrl(url: String): String = nativeToWebViewUrl(url)

    /** The current chrome the Activity paints (URL bar, nav enablement, status). */
    fun chrome(): Chrome = Chrome.fromJson(nativeChromeJson(handle))

    /**
     * The bounded console + network CAPTURE STORE as its own JSON document, the
     * wire form the in-app debug view renders (a DEDICATED accessor beside
     * [chrome], so the chrome JSON — re-encoded on every refresh — stays lean).
     * Read OFF the native session lock, so polling it never waits on an in-flight
     * `ipfs://` retrieval.
     */
    fun debugJson(): String = nativeDebugJson(handle)

    /** Empty the capture store: the debug view's Clear action. */
    fun debugClear() = nativeDebugClear(handle)

    /**
     * Capture one CONSOLE message from the platform's REAL native callback
     * ([android.webkit.WebChromeClient.onConsoleMessage]) into the shared core
     * store, for the in-app debug menu's Console tab (task
     * `debug-console-network-capture-per-platform`).
     *
     * Android needs NO injected console shim (which is what desktop and iOS must
     * use, since neither WebKitGTK 6 nor WKWebView exposes a console callback):
     * this hook reports message/level/source/line directly, sees engine-emitted
     * messages a page-side wrapper never could, and cannot be un-wrapped by the
     * page. [level] is the platform's `ConsoleMessage.MessageLevel` name; the core
     * maps it onto werust's one console vocabulary. [line] is 1-based, `0` for
     * unknown.
     *
     * Runs on the UI THREAD, and the native side pushes OFF the session lock, so
     * capture can never block the UI behind an in-flight `ipfs://` retrieval (the
     * ANR guard). Capture is READ-ONLY observation: the page's own console is
     * untouched.
     */
    fun captureConsole(level: String, message: String, source: String, line: Int) =
        nativeCaptureConsole(handle, level, message, source, line)

    /**
     * Capture one NETWORK request from
     * [android.webkit.WebViewClient.shouldInterceptRequest] into the shared core
     * store, for the in-app debug menu's Network tab.
     *
     * That hook sees EVERY request the WebView makes (Android has the widest
     * network reach of the three platforms), so it records BOTH the intercepted
     * (`ipfs://`, answered from the core) and the passed-through (`return null`)
     * requests. [verified] must say whether THIS request's bytes really came back
     * hash-verified through the content-addressed path — never whether the URL
     * merely looks content-addressed — so the Network tab can never imply a
     * request was trusted that was not (ADR-0006). [mainFrame] marks the
     * main-document row, which takes the LOAD's own posture so the tab cannot
     * contradict the trust indicator. A `0` [status]/[size] means unknown.
     *
     * Runs on the WebView WORKER thread and pushes off the session lock, so it
     * neither blocks nor is blocked by an in-flight retrieval. Capture is
     * READ-ONLY: it does not decide or delay what the hook returns.
     */
    fun captureNetwork(
        method: String,
        url: String,
        status: Int,
        mime: String,
        size: Long,
        verified: Boolean,
        mainFrame: Boolean,
    ) = nativeCaptureNetwork(handle, method, url, status, mime, size, verified, mainFrame)

    /**
     * The GENERAL browser menu (the ⋮ menu) the Activity builds its native
     * `PopupMenu` from: the werust VERSION line + a Debug entry that opens the
     * in-app debug view, decoded from the core's JSON wire form.
     *
     * The item list is the SHARED core's ([items][Menu.items]), so the Android menu
     * shows exactly what the desktop popover and the iOS menu show, and a FUTURE
     * menu item added in `werust-core` appears here with no Kotlin change.
     *
     * Note the native call threads NO session [handle] (unlike every other method
     * here): the menu is a property of the BUILD, not of a browsing session, so it
     * is always available and no native session is borrowed to read a constant.
     * (It is still declared as an instance external, so its JNI symbol is the
     * plain `Java_..._WerustCore_nativeMenuJson` the Rust side exports — a
     * companion-object `@JvmStatic external` would emit the native symbol on
     * `WerustCore$Companion` instead.)
     */
    fun menu(): Menu = Menu.fromJson(nativeMenuJson())

    /**
     * werust's version string, from the ONE shared source
     * (`werust_core::version`, the Rust workspace version) over the FFI — never a
     * Kotlin literal and never the Gradle `versionName`, so the Android menu can
     * never disagree with the desktop and iOS menus. Session-free, like [menu].
     */
    fun version(): String = nativeVersion()

    override fun close() {
        if (handle != 0L) {
            nativeFree(handle)
            handle = 0L
        }
    }

    /**
     * The outcome of resolving an intercepted `ipfs://` request through the core:
     * verified bytes + MIME type + status, or a fail-closed reason. Mirrors the
     * Rust `SchemeResolution`; the `WebViewClient` turns [Resolution.Ok] into a
     * `WebResourceResponse` and [Resolution.Error] into a failed load, so the
     * desktop fail-closed posture holds on Android.
     *
     * [Resolution.Ok.status] is 200 for an ordinary resource; it is carried so a
     * site's own error page (named by its `_redirects`, IPIP-0002, for a path not
     * in its DAG) is answered with the HONEST not-found status while still
     * rendering, exactly as an IPFS gateway serves it.
     */
    sealed class Resolution {
        data class Ok(val mimeType: String, val body: ByteArray, val status: Int) : Resolution()
        data class Error(val reason: String) : Resolution()
    }

    /**
     * The GENERAL browser menu, decoded from the core's JSON wire form: the
     * werust [version] plus the ordered [items] every platform renders.
     *
     * This is a CONTAINER meant to GROW into the usual browser items (bookmarks,
     * settings, history, …): the Activity iterates [items] and renders each by its
     * [Item.kind], so a new item added in `werust-core` needs no Kotlin change
     * unless it is an action with new behaviour.
     */
    data class Menu(val version: String, val items: List<Item>) {
        /**
         * One menu entry: the stable cross-platform [id] the Activity dispatches
         * on (never the [label], which is display text), the [label] to show, and
         * the [kind] telling the Activity whether to render it as a
         * non-interactive line or a tappable entry.
         */
        data class Item(val id: String, val label: String, val kind: String) {
            /** Whether this entry is activatable (vs a non-interactive line). */
            fun isAction(): Boolean = kind == KIND_ACTION
        }

        companion object {
            /** The stable id of the non-interactive `werust <version>` line. */
            const val ITEM_VERSION = "version"

            /** The stable id of the entry that opens the in-app debug view. */
            const val ITEM_DEBUG = "debug"

            /** The `kind` of an activatable entry (vs `"info"`, a plain line). */
            const val KIND_ACTION = "action"

            fun fromJson(json: String): Menu {
                val o = JSONObject(json)
                val array = o.optJSONArray("items")
                    ?: return Menu(o.optString("version", ""), emptyList())
                val items = ArrayList<Item>(array.length())
                for (i in 0 until array.length()) {
                    val item = array.getJSONObject(i)
                    items.add(
                        Item(
                            id = item.getString("id"),
                            label = item.getString("label"),
                            kind = item.optString("kind", "info"),
                        )
                    )
                }
                return Menu(version = o.optString("version", ""), items = items)
            }
        }
    }

    /**
     * The chrome the Activity paints, decoded from the core's JSON wire form.
     * Every field is the core's truth; the Activity holds none of this logic.
     *
     * TWO HALVES, both decided in `werust-core`:
     *
     * * the FACTS ([url], [loading], [loadStep], [trustPosture], [error],
     *   [failureKind], [retryable], [invalidEntry], …): what the chrome IS;
     * * the DERIVATION ([statusLine], [trustIndicator], [trustIndicatorDetail],
     *   [errorBannerText], [invalidEntryBadgeText], [loadProgressFraction], …):
     *   what the chrome SHOWS, each field the return value of the core rule of
     *   the same name (`status_line`, `trust_indicator`, `trust_indicator_detail`,
     *   `error_banner_text`, `invalid_entry_badge_text`, `load_progress_fraction`,
     *   …).
     *
     * This class used to RE-DERIVE the second half in Kotlin (`statusLine()`,
     * `trustIndicator()`, `errorBanner()`, `invalidEntryBadge()`,
     * `loadProgress*()`), a hand-written twin of the Rust originals: one rule set
     * in three languages, which had already drifted. the trust EXPLANATION
     * ([trustIndicatorDetail]) existed only on desktop, and the load-progress unit
     * was a fraction in Rust and Swift but a PERCENT here. It now reads a field
     * instead of running a `when` (task
     * `mobile-chrome-presentation-from-one-derivation`, `docs/adr/0011`), so a new
     * trust posture or pipeline phase reaches Android with no Kotlin change at
     * all.
     *
     * Adding a display rule here is therefore a REGRESSION, not a feature: the
     * rule belongs in `werust-core` beside its siblings, where every edge gets it
     * (a source-shape guard,
     * `crates/werust-core/tests/mobile_chrome_presentation_shape.rs`, reds the
     * gate if a twin comes back).
     */
    data class Chrome(
        val url: String,
        val loadState: String,
        val loading: Boolean,
        val loadStep: String,
        val canGoBack: Boolean,
        val canGoForward: Boolean,
        val trustPosture: String,
        val error: String?,
        val failureKind: String?,
        val retryable: Boolean,
        val invalidEntry: String?,
        /**
         * The one-line status shown in the footer: a failure wins, else a loading
         * indicator that NAMES the real pipeline step (resolving name / fetching
         * record / fetching content / rendering) so a slow load reads as working,
         * not frozen, else idle. The core's `status_line`.
         */
        val statusLine: String,
        /**
         * The short trust-indicator badge: the core's `trust_indicator`, painted
         * from the posture of the ACTUAL load path (never the URL), and a NEUTRAL
         * loading state while a load is in flight so the previous page's trust
         * never lingers into a new one.
         */
        val trustIndicator: String,
        /**
         * The longer EXPLANATION of what the current [trustIndicator] means: the
         * core's `trust_indicator_detail`, the same sentence the desktop badge
         * shows on hover. Mobile has no hover, so the Activity surfaces it as the
         * badge's accessibility description plus a tap affordance (task
         * `mobile-chrome-presentation-from-one-derivation`). For months this text
         * reached desktop only, which is exactly the drift the collapse closes.
         */
        val trustIndicatorDetail: String,
        /**
         * Whether the PROMINENT in-view error banner shows: exactly when the last
         * load failed. The core's `error_banner_visible`.
         */
        val errorBannerVisible: Boolean,
        /**
         * The error banner's text: the accurate, protocol-named reason, framed as
         * a retryable timeout or a hard failure. The core's `error_banner_text`
         * (empty when the banner is hidden).
         */
        val errorBannerText: String,
        /**
         * Whether the small "invalid URL" badge shows: exactly when the last
         * URL-bar entry was INVALID (a garbage entry that did not navigate). The
         * core's `invalid_entry_badge_visible`.
         */
        val invalidEntryBadgeVisible: Boolean,
        /** The invalid-entry badge's text; the core's `invalid_entry_badge_text`. */
        val invalidEntryBadgeText: String,
        /**
         * Whether there is a load to indicate at all: a backend load in flight OR
         * a pinned pre-content resolution step (the long `ronan.eth` window where
         * the backend has not started yet). The core's `load_progress_visible`.
         */
        val loadProgressVisible: Boolean,
        /**
         * The load-progress FRACTION, 0.0-1.0: the core's
         * `load_progress_fraction`, in the core's unit. The `ProgressBar`'s 0-100
         * scale is applied where the widget is painted, so this stays the one
         * shared number (it was a hand-converted percent here before, the only
         * unit fork among the three copies).
         */
        val loadProgressFraction: Double,
        /**
         * The phase NAME behind the current progress, for the progress line's
         * content description: the core's `load_progress_hint`, the `LoadStep`
         * hint vocabulary verbatim.
         */
        val loadProgressHint: String,
    ) {
        companion object {
            fun fromJson(json: String): Chrome {
                val o = JSONObject(json)
                return Chrome(
                    url = o.getString("url"),
                    loadState = o.getString("loadState"),
                    loading = o.getBoolean("loading"),
                    loadStep = o.optString("loadStep", "idle"),
                    canGoBack = o.getBoolean("canGoBack"),
                    canGoForward = o.getBoolean("canGoForward"),
                    trustPosture = o.optString("trustPosture", "unverified-origin"),
                    error = if (o.isNull("error")) null else o.getString("error"),
                    failureKind = if (o.isNull("failureKind")) null else o.optString("failureKind"),
                    retryable = o.optBoolean("retryable", false),
                    invalidEntry = if (o.isNull("invalidEntry")) null else o.getString("invalidEntry"),
                    // The DERIVED half. Each default is the EMPTY/absent value, never
                    // a Kotlin copy of the core's wording: a document that somehow
                    // lacked a derived field must show nothing rather than a second,
                    // drifting spelling of it.
                    statusLine = o.optString("statusLine", ""),
                    trustIndicator = o.optString("trustIndicator", ""),
                    trustIndicatorDetail = o.optString("trustIndicatorDetail", ""),
                    errorBannerVisible = o.optBoolean("errorBannerVisible", false),
                    errorBannerText = o.optString("errorBannerText", ""),
                    invalidEntryBadgeVisible = o.optBoolean("invalidEntryBadgeVisible", false),
                    invalidEntryBadgeText = o.optString("invalidEntryBadgeText", ""),
                    loadProgressVisible = o.optBoolean("loadProgressVisible", false),
                    loadProgressFraction = o.optDouble("loadProgressFraction", 0.0),
                    loadProgressHint = o.optString("loadProgressHint", ""),
                )
            }
        }
    }

    private external fun nativeNew(): Long
    private external fun nativeFree(handle: Long)
    private external fun nativeNavigate(handle: Long, url: String): Boolean
    private external fun nativeGoBack(handle: Long)
    private external fun nativeGoForward(handle: Long)
    private external fun nativeReload(handle: Long): Boolean
    private external fun nativeStop(handle: Long)
    private external fun nativeTakePendingLoad(handle: Long): String
    private external fun nativeDocumentStartScript(handle: Long): String
    private external fun nativeHandleProviderMessage(handle: Long, name: String, body: String): String
    private external fun nativeResolveIpfs(handle: Long, uri: String): Long
    private external fun nativeResolutionIsOk(resolution: Long): Boolean
    private external fun nativeResolutionMime(resolution: Long): String
    private external fun nativeResolutionBody(resolution: Long): ByteArray
    private external fun nativeResolutionStatus(resolution: Long): Int
    private external fun nativeResolutionError(resolution: Long): String
    private external fun nativeResolutionFree(resolution: Long)
    private external fun nativeOnPageCommitted(handle: Long, url: String)
    private external fun nativeOnPageFinished(handle: Long, url: String)
    private external fun nativeOnPageFailed(handle: Long, url: String, reason: String)
    private external fun nativeOnUrlChanged(handle: Long, url: String)
    private external fun nativeToWebViewUrl(url: String): String
    private external fun nativeChromeJson(handle: Long): String
    private external fun nativeDebugJson(handle: Long): String
    private external fun nativeDebugClear(handle: Long)
    private external fun nativeCaptureConsole(
        handle: Long,
        level: String,
        message: String,
        source: String,
        line: Int,
    )
    private external fun nativeCaptureNetwork(
        handle: Long,
        method: String,
        url: String,
        status: Int,
        mime: String,
        size: Long,
        verified: Boolean,
        mainFrame: Boolean,
    )

    // The browser-menu accessors thread NO session handle: the version and the
    // menu are properties of the BUILD, so the menu is available whatever the
    // session state is.
    private external fun nativeVersion(): String
    private external fun nativeMenuJson(): String

    companion object {
        init {
            System.loadLibrary("werust_mobile")
        }
    }
}
