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
                Resolution.Ok(nativeResolutionMime(res), nativeResolutionBody(res))
            } else {
                Resolution.Error(nativeResolutionError(res))
            }
        } finally {
            nativeResolutionFree(res)
        }
    }

    /** Report the platform `WebView`'s commit signal into the core. */
    fun onPageCommitted(url: String) = nativeOnPageCommitted(handle, url)

    /** Report the platform `WebView`'s finished signal into the core. */
    fun onPageFinished(url: String) = nativeOnPageFinished(handle, url)

    /** Report the platform `WebView`'s error signal into the core. */
    fun onPageFailed(url: String, reason: String) = nativeOnPageFailed(handle, url, reason)

    /** The current chrome the Activity paints (URL bar, nav enablement, status). */
    fun chrome(): Chrome = Chrome.fromJson(nativeChromeJson(handle))

    override fun close() {
        if (handle != 0L) {
            nativeFree(handle)
            handle = 0L
        }
    }

    /**
     * The outcome of resolving an intercepted `ipfs://` request through the core:
     * verified bytes + MIME type, or a fail-closed reason. Mirrors the Rust
     * `SchemeResolution`; the `WebViewClient` turns [Resolution.Ok] into a
     * `WebResourceResponse` and [Resolution.Error] into a failed load, so the
     * desktop fail-closed posture holds on Android.
     */
    sealed class Resolution {
        data class Ok(val mimeType: String, val body: ByteArray) : Resolution()
        data class Error(val reason: String) : Resolution()
    }

    /**
     * The chrome the Activity paints, decoded from the core's JSON wire form.
     * Every field is the core's truth; the Activity holds none of this logic.
     */
    data class Chrome(
        val url: String,
        val loadState: String,
        val loading: Boolean,
        val canGoBack: Boolean,
        val canGoForward: Boolean,
        val error: String?,
    ) {
        /** The one-line status the Activity shows: a failure wins, else loading/idle. */
        fun statusLine(): String = when {
            error != null -> "failed: $error"
            loading -> "loading…"
            else -> "idle"
        }

        companion object {
            fun fromJson(json: String): Chrome {
                val o = JSONObject(json)
                return Chrome(
                    url = o.getString("url"),
                    loadState = o.getString("loadState"),
                    loading = o.getBoolean("loading"),
                    canGoBack = o.getBoolean("canGoBack"),
                    canGoForward = o.getBoolean("canGoForward"),
                    error = if (o.isNull("error")) null else o.getString("error"),
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
    private external fun nativeResolveIpfs(handle: Long, uri: String): Long
    private external fun nativeResolutionIsOk(resolution: Long): Boolean
    private external fun nativeResolutionMime(resolution: Long): String
    private external fun nativeResolutionBody(resolution: Long): ByteArray
    private external fun nativeResolutionError(resolution: Long): String
    private external fun nativeResolutionFree(resolution: Long)
    private external fun nativeOnPageCommitted(handle: Long, url: String)
    private external fun nativeOnPageFinished(handle: Long, url: String)
    private external fun nativeOnPageFailed(handle: Long, url: String, reason: String)
    private external fun nativeChromeJson(handle: Long): String

    companion object {
        init {
            System.loadLibrary("werust_mobile")
        }
    }
}
