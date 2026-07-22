package com.github.wighawag.werust

import android.annotation.SuppressLint
import android.app.Activity
import android.graphics.Bitmap
import android.os.Bundle
import android.util.TypedValue
import android.view.Gravity
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.view.inputmethod.EditorInfo
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import java.io.ByteArrayInputStream
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView

/**
 * The Android OS edge: a real `Activity` with a URL bar and
 * Back/Forward/Reload/Stop controls over a live, interactive [WebView].
 *
 * This is the forced OS edge and NOTHING more: it owns the platform `WebView` and
 * the widgets, but every browsing DECISION is the Rust [WerustCore]'s. On a user
 * action it drives the core, then (1) applies whatever URL the core surfaces to
 * the `WebView` ([syncPendingLoad]) and (2) repaints its chrome from the core's
 * [WerustCore.Chrome] ([refreshChrome]). The `WebView`'s real load-lifecycle
 * callbacks are reported straight back into the core, which folds them into the
 * chrome exactly as the desktop GTK pump folds WebKitGTK's signals. The URL bar
 * text, the Back/Forward enablement, and the load status are all read from the
 * core — the edge keeps no history or load state of its own.
 */
class BrowserActivity : Activity() {

    private val core = WerustCore()

    private lateinit var urlBar: EditText
    private lateinit var backButton: Button
    private lateinit var forwardButton: Button
    private lateinit var reloadButton: Button
    private lateinit var stopButton: Button
    private lateinit var status: TextView
    private lateinit var webView: WebView

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val root = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        // The toolbar row is at least a touch-target tall (48dp), so even though the
        // nav buttons render as a small square glyph they stay comfortably tappable.
        val toolbar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = dp(TOUCH_TARGET_DP)
        }

        backButton = compactNavButton("◀") { core.goBack(); afterCoreAction() }
        forwardButton = compactNavButton("▶") { core.goForward(); afterCoreAction() }
        reloadButton = compactNavButton("⟳") { core.reload(); afterCoreAction() }
        stopButton = compactNavButton("✕") { core.stop(); afterCoreAction() }
        urlBar = EditText(this).apply {
            hint = "Enter a URL and press Enter"
            layoutParams = LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f)
            imeOptions = EditorInfo.IME_ACTION_GO
            setSingleLine()
            setOnEditorActionListener { _, actionId, _ ->
                if (actionId == EditorInfo.IME_ACTION_GO) {
                    core.navigate(text.toString()); afterCoreAction(); true
                } else {
                    false
                }
            }
        }

        toolbar.addView(backButton)
        toolbar.addView(forwardButton)
        toolbar.addView(reloadButton)
        toolbar.addView(stopButton)
        toolbar.addView(urlBar)

        webView = WebView(this).apply {
            layoutParams = LinearLayout.LayoutParams(MATCH_PARENT, 0, 1f)
            settings.javaScriptEnabled = true
            webViewClient = CoreWebViewClient()
        }

        status = TextView(this).apply { text = "idle"; gravity = Gravity.START }

        root.addView(toolbar, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
        root.addView(webView)
        root.addView(status, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
        setContentView(root)

        // Launch a browsing surface: drive the core to the start URL, then let the
        // core surface it onto the WebView.
        core.navigate(START_URL)
        afterCoreAction()
    }

    /**
     * A COMPACT nav button: a small fixed square that strips the default Android
     * button's large min-width and horizontal insets ([minWidth]/[minimumWidth] 0,
     * no horizontal padding), so four of them take a small fixed slice of the row
     * and the weighted URL bar keeps the majority of the width. The button glyph is
     * small, but the enclosing toolbar row is [TOUCH_TARGET_DP] tall so the
     * effective touch target stays >= 48dp.
     */
    private fun compactNavButton(glyph: String, onClick: () -> Unit): Button =
        Button(this).apply {
            text = glyph
            minWidth = 0
            minimumWidth = 0
            minHeight = 0
            minimumHeight = 0
            setPadding(0, 0, 0, 0)
            gravity = Gravity.CENTER
            includeFontPadding = false
            layoutParams = LinearLayout.LayoutParams(dp(NAV_BUTTON_DP), dp(NAV_BUTTON_DP))
            setOnClickListener { onClick() }
        }

    /** Density-independent pixels -> device pixels, for the compact fixed sizes. */
    private fun dp(value: Int): Int =
        TypedValue.applyDimension(
            TypedValue.COMPLEX_UNIT_DIP,
            value.toFloat(),
            resources.displayMetrics,
        ).toInt()

    /** After driving the core, apply any pending load to the WebView and repaint. */
    private fun afterCoreAction() {
        syncPendingLoad()
        refreshChrome()
    }

    /** Apply the URL the core surfaced (if any) to the platform WebView. */
    private fun syncPendingLoad() {
        core.takePendingLoad()?.let { webView.loadUrl(it) }
    }

    /** Repaint the chrome from the core's truth (never the edge's own state). */
    private fun refreshChrome() {
        val chrome = core.chrome()
        if (urlBar.text.toString() != chrome.url) urlBar.setText(chrome.url)
        backButton.isEnabled = chrome.canGoBack
        forwardButton.isEnabled = chrome.canGoForward
        stopButton.isEnabled = chrome.loading
        reloadButton.isEnabled = !chrome.loading
        status.text = chrome.statusLine()
    }

    override fun onDestroy() {
        core.close()
        super.onDestroy()
    }

    /**
     * Reports the platform `WebView`'s real load-lifecycle signals straight back
     * into the Rust core, then repaints the chrome from the core.
     */
    private inner class CoreWebViewClient : WebViewClient() {
        /**
         * Intercept `ipfs://` requests the platform `WebView` cannot load itself
         * (`net::ERR_UNKNOWN_URL_SCHEME`) and answer them from the SHARED
         * `werust-core` resolve path (the same hash-verified path desktop uses via
         * WebKitGTK `install_ipfs`). This is the Android realisation of the mobile
         * `ipfs://` interception: the request is routed through
         * [WerustCore.resolveIpfs] into the core, and the verified bytes are
         * served as a `WebResourceResponse` so an ENS-resolved `ipfs://<cid>` site
         * renders instead of failing. A fail-closed resolution error is answered
         * with an HTTP error status (no bytes), matching the desktop posture where
         * a hash mismatch fails the load rather than rendering unverified bytes.
         *
         * The `.eth` name the user typed stays in the address bar (the core's
         * chrome truth); the `ipfs://<cid>` scheme is served here transparently
         * with no `https://`/gateway URL shown to the user. A non-`ipfs://`
         * request returns `null` so the `WebView` handles it normally.
         *
         * INTERCEPTION MECHANISM (Android): the NATIVE custom scheme via
         * `shouldInterceptRequest`. See the recorded decision at
         * work/notes/observations/mobile-ipfs-interception-mechanism-2026-07-23.md
         * for the internal-`https://appassets` fallback if a device build shows a
         * top-level `ipfs://` navigation does not reach this hook.
         */
        override fun shouldInterceptRequest(
            view: WebView,
            request: WebResourceRequest,
        ): WebResourceResponse? {
            val url = request.url.toString()
            return when (val resolution = core.resolveIpfs(url)) {
                null -> null // not an intercepted scheme: let the WebView handle it
                is WerustCore.Resolution.Ok ->
                    WebResourceResponse(
                        resolution.mimeType,
                        "utf-8",
                        ByteArrayInputStream(resolution.body),
                    )
                is WerustCore.Resolution.Error ->
                    // Fail closed: an HTTP error status with no body, so unverified
                    // bytes never render and the failure is honest. The reason is
                    // carried in the reason phrase; the `WebView` surfaces the
                    // failed load via `onReceivedHttpError` on the UI thread (this
                    // hook runs off the UI thread, so it must not mutate the core).
                    WebResourceResponse(
                        "text/plain",
                        "utf-8",
                        502,
                        // The HTTP reason phrase must be a single line: collapse any
                        // newlines so a multi-line reason cannot throw.
                        resolution.reason
                            .replace('\n', ' ')
                            .replace('\r', ' ')
                            .ifBlank { "ipfs resolution failed" },
                        emptyMap<String, String>(),
                        ByteArrayInputStream(ByteArray(0)),
                    )
            }
        }

        override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
            core.onPageCommitted(url)
            refreshChrome()
        }

        override fun onPageFinished(view: WebView, url: String) {
            core.onPageFinished(url)
            refreshChrome()
        }

        override fun onReceivedError(
            view: WebView,
            request: WebResourceRequest,
            error: WebResourceError,
        ) {
            if (request.isForMainFrame) {
                core.onPageFailed(request.url.toString(), error.description.toString())
                refreshChrome()
            }
        }
    }

    companion object {
        /** The URL the app opens on launch, so it shows a browsing surface. */
        private const val START_URL = "https://example.com/"

        /** The compact nav-button square edge, in dp (small so the URL bar wins width). */
        private const val NAV_BUTTON_DP = 40

        /** The toolbar row's minimum height, in dp, keeping touch targets >= 48dp. */
        private const val TOUCH_TARGET_DP = 48
    }
}
