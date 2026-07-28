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

    /** Report the platform `WebView`'s commit signal into the core. */
    fun onPageCommitted(url: String) = nativeOnPageCommitted(handle, url)

    /** Report the platform `WebView`'s finished signal into the core. */
    fun onPageFinished(url: String) = nativeOnPageFinished(handle, url)

    /** Report the platform `WebView`'s error signal into the core. */
    fun onPageFailed(url: String, reason: String) = nativeOnPageFailed(handle, url, reason)

    /**
     * Report a SAME-DOCUMENT URL change (an SPA `pushState`/`replaceState`
     * client-side navigation) into the core, so the URL bar follows the new
     * location instead of freezing. Reported from
     * [android.webkit.WebViewClient.doUpdateVisitedHistory], which fires on
     * same-document history changes (no `onPageStarted`/`onPageFinished`).
     */
    fun onUrlChanged(url: String) = nativeOnUrlChanged(handle, url)

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
    ) {
        /**
         * The one-line status the Activity shows: a failure wins, else a loading
         * indicator that NAMES the real pipeline step (resolving name / fetching
         * record / fetching content / rendering) so a slow load reads as working,
         * not frozen, else idle. The step hint is the core's `loadStep` (driven by
         * the actual lifecycle), the SAME fact desktop reads (task
         * `clearer-loading-and-error-indicator`).
         */
        fun statusLine(): String = when {
            error != null -> "failed: $error"
            loading -> {
                val hint = loadStepHint()
                if (hint.isEmpty()) "loading…" else "loading… — $hint"
            }
            else -> "idle"
        }

        /**
         * The short human-readable hint for the current pipeline step, or empty for
         * no step (idle). Mirrors the core's `LoadStep::hint`, so the mobile status
         * text matches desktop.
         */
        private fun loadStepHint(): String = when (loadStep) {
            "resolving-name" -> "resolving name"
            "fetching-record" -> "fetching record"
            "fetching-content" -> "fetching content"
            "rendering" -> "rendering"
            else -> ""
        }

        /**
         * The short trust-indicator badge the Activity paints from the core's
         * posture (the ACTUAL load path, not the URL) — the SAME states the
         * desktop chrome shows. Never labels a name-resolved or mutable page
         * "verified" (only a direct `ipfs://<cid>` earns that).
         *
         * While a load is IN FLIGHT (`loading`) the indicator is a NEUTRAL loading
         * state that WINS over the posture, making NO trust claim — the
         * trust-honesty fix (task `chrome-loading-state-resets-trust-indicator`):
         * on navigation to a possibly differently-trusted page, the indicator must
         * not keep asserting the previous page's (or a not-yet-proven) trust while
         * the new page loads; the real posture appears only once the load settles.
         * The SAME loading-wins rule the desktop chrome applies, from the SAME
         * `loading` fact.
         */
        fun trustIndicator(): String = when {
            loading -> "⋯ loading…"
            trustPosture == "content-verified" -> "✓ verified"
            trustPosture == "name-via-trusted-rpc" -> "◈ name via trusted RPC"
            trustPosture == "mutable-name" -> "◇ content verified, mutable name"
            else -> "⚠ unverified origin"
        }

        /**
         * Whether the PROMINENT in-view error banner should be shown: exactly when
         * the last load failed (`error` is set). The whole point of fail-closed is
         * that the user UNDERSTANDS why nothing rendered; the subtle footer status
         * was "not easily seen" (a real `ronan.eth` IPNS failure was missed), so a
         * failed load ALSO raises a high-contrast banner the user cannot miss. The
         * SAME rule desktop applies, from the SAME chrome-JSON fact.
         */
        fun errorBannerVisible(): Boolean = error != null

        /**
         * The PROMINENT error-banner text for a failed load: the accurate,
         * protocol-named reason drawn straight from `error` (the resolver/decoder
         * taxonomy — e.g. "IPNS record did not verify: …"), never a generic
         * "failed". Empty when there is no failure (the banner is hidden then).
         */
        fun errorBanner(): String = when {
            error == null -> ""
            // A TRANSIENT/timeout failure (retryable) is surfaced DISTINCTLY from a
            // hard failure: a softer "timed out — reload to retry" (the Reload
            // button IS the retry — a failed ENS load re-resolves), while a hard
            // failure keeps the prominent "failed to load" wording + its
            // protocol-named reason. The distinction is the core's `retryable`
            // (task `clearer-loading-and-error-indicator`), the SAME fact desktop
            // reads, so the two never disagree.
            retryable -> "⏳ This page timed out — reload to retry: $error"
            else -> "⚠ This page failed to load: $error"
        }

        /**
         * Whether the surfaced failure is RETRYABLE (a transient timeout a reload
         * may fix), so the Activity can show a retry affordance. `false` for a hard
         * failure or when nothing failed. The core's `retryable` fact, matching
         * desktop.
         */
        fun errorIsRetryable(): Boolean = error != null && retryable

        /**
         * Whether the small "invalid URL" BADGE should be shown: exactly when the
         * last URL-bar entry was INVALID (a scheme-less garbage entry that did not
         * navigate). A pure read of the orthogonal `invalidEntry` fact — distinct
         * from a load error (`error`) — so the Activity paints the badge + the
         * red-underlined URL bar from the SAME chrome-JSON fact desktop uses (field
         * finding D, task `scheme-less-entry-https-fallback-and-keep-bar-on-error`).
         */
        fun invalidEntryVisible(): Boolean = invalidEntry != null

        /**
         * The small "invalid URL" badge text for an invalid entry, empty otherwise
         * (the badge is hidden then). Matches desktop's badge wording.
         */
        fun invalidEntryBadge(): String = if (invalidEntry != null) "⛔ invalid URL" else ""

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
