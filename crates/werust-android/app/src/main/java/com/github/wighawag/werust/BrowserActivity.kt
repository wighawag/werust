package com.github.wighawag.werust

import android.annotation.SuppressLint
import android.app.Activity
import android.graphics.Bitmap
import android.os.Bundle
import android.view.Gravity
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.view.inputmethod.EditorInfo
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
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
        val toolbar = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }

        backButton = Button(this).apply { text = "◀"; setOnClickListener { core.goBack(); afterCoreAction() } }
        forwardButton = Button(this).apply { text = "▶"; setOnClickListener { core.goForward(); afterCoreAction() } }
        reloadButton = Button(this).apply { text = "⟳"; setOnClickListener { core.reload(); afterCoreAction() } }
        stopButton = Button(this).apply { text = "✕"; setOnClickListener { core.stop(); afterCoreAction() } }
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
    }
}
