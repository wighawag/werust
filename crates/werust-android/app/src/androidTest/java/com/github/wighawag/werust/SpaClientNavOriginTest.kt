package com.github.wighawag.werust

import android.util.Log
import android.webkit.ConsoleMessage
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.ByteArrayInputStream
import java.util.concurrent.CountDownLatch
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The ON-DEVICE PROBE for the mobile no-navigation bug (task
 * `mobile-ronan-eth-buttons-no-navigation`, the parked mobile half of field-test
 * finding D): on `ronan.eth` (a SvelteKit `adapter-static` site) the blog and
 * portfolio buttons did NOTHING on Android while the same buttons work on
 * desktop.
 *
 * This is the minimal, network-isolated repro of the bug CLASS, driven against
 * the REAL System WebView on an emulator/device: a raw `WebView` serves a tiny
 * page that performs the three things a SvelteKit client-side navigation needs
 * (a relative `fetch` of the route's `__data.json`, then `history.pushState`),
 * first from an `ipfs://` document answered through `shouldInterceptRequest`
 * (the PRE-FIX mechanism) and then from the internal
 * `https://<cid>.ipfs.werust.invalid` origin (the FIX, `origin_map.rs`). The
 * harness records exactly the signals the task told the diagnosis to capture:
 * every intercepted request (would a `__data.json` Network entry appear?),
 * every console message (does a console error fire?), and every
 * `doUpdateVisitedHistory` (does the WebView see the nav at all?).
 *
 * The diagnosis + the captured on-device values live in
 * `docs/spikes/mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md`. The Rust
 * edge's own seam guards (the URL mapping and the core-side signal routing) are
 * the `cargo test` unit tests in `crates/werust-android/rust/src/`; THIS probe
 * is the regression guard at the one seam those cannot reach: the real
 * Chromium/Blink origin behaviour of the System WebView. It is not part of the
 * repo's pure-Rust `verify` gate (it needs a device/emulator); run it with:
 *
 * ```
 * cd crates/werust-android && ./gradlew :app:connectedDebugAndroidTest
 * ```
 */
@RunWith(AndroidJUnit4::class)
class SpaClientNavOriginTest {

    private lateinit var probe: Probe

    @Before
    fun setUp() {
        probe = Probe()
    }

    @After
    fun tearDown() {
        probe.destroy()
    }

    /**
     * THE BUG, reproduced at the mechanism seam: an `ipfs://` document answered
     * through `shouldInterceptRequest` gets an OPAQUE origin in the System
     * WebView, and on that origin a SvelteKit-style client navigation dies
     * BEFORE any signal reaches werust: the `__data.json` fetch is rejected
     * inside Blink without ever hitting the network stack (so
     * `shouldInterceptRequest` never fires for it and the debug view's Network
     * tab shows NOTHING), and `history.pushState` throws `SecurityError` (so
     * `doUpdateVisitedHistory` never fires and the URL bar/history never move).
     * The only signal at all is a CONSOLE error from Blink.
     */
    @Test
    fun an_ipfs_document_served_via_interception_has_an_opaque_origin_where_client_nav_dies() {
        val pageUrl = "ipfs://$CID_V1/"
        val outcome = probe.loadAndRunClientNav(pageUrl)

        val evidence = """
            |page: $pageUrl
            |origin: ${outcome.origin}
            |fetch: ${outcome.fetch}
            |pushState: ${outcome.pushState}
            |intercepted requests: ${probe.intercepted}
            |console: ${probe.console}
            |history updates: ${probe.historyUpdates}
        """.trimMargin()
        // The captured on-device evidence, quoted verbatim in DIAGNOSIS.md.
        Log.i(EVIDENCE_TAG, "BEFORE (pre-fix mechanism):\n$evidence")

        assertTrue(
            "the document's origin is OPAQUE (no usable scheme://host tuple)\n$evidence",
            outcome.origin == "null" || outcome.origin == "ipfs://",
        )
        assertTrue(
            "the client router's __data.json fetch is REJECTED inside Blink\n$evidence",
            outcome.fetch.startsWith("reject:"),
        )
        assertTrue(
            "the rejected fetch NEVER reaches the network stack (no Network entry)\n$evidence",
            probe.intercepted.none { it.contains("__data.json") },
        )
        assertTrue(
            "pushState to another ipfs:// path THROWS SecurityError\n$evidence",
            outcome.pushState.startsWith("throw:SecurityError"),
        )
        assertTrue(
            "no doUpdateVisitedHistory fires (the nav is invisible to werust)\n$evidence",
            probe.historyUpdates.isEmpty(),
        )
        assertTrue(
            "a CONSOLE error is the only signal (Blink names the unsupported scheme)\n$evidence",
            probe.console.any { it.contains("__data.json") || it.contains("not supported") },
        )
    }

    /**
     * THE FIX, proven at the same seam: the SAME page served on the internal
     * `https://<cid>.ipfs.werust.invalid` origin (what the WebView now loads;
     * the Rust edge maps every URL back to the real `ipfs://` form) has a
     * normal tuple origin, so the client navigation proceeds AND completes:
     * the `__data.json` fetch reaches `shouldInterceptRequest` and succeeds,
     * and `pushState` succeeds and fires `doUpdateVisitedHistory`, the signal
     * the core's SPA-nav URL tracking (`onUrlChanged`) needs.
     */
    @Test
    fun the_internal_https_origin_lets_a_spa_client_nav_proceed_and_complete() {
        val pageUrl = "$INTERNAL_ORIGIN/"
        val outcome = probe.loadAndRunClientNav(pageUrl)

        val evidence = """
            |page: $pageUrl
            |origin: ${outcome.origin}
            |fetch: ${outcome.fetch}
            |pushState: ${outcome.pushState}
            |intercepted requests: ${probe.intercepted}
            |console: ${probe.console}
            |history updates: ${probe.historyUpdates}
        """.trimMargin()
        Log.i(EVIDENCE_TAG, "AFTER (the fix):\n$evidence")

        assertEquals("the internal origin is a real tuple origin\n$evidence", INTERNAL_ORIGIN, outcome.origin)
        assertEquals("the client router's __data.json fetch SUCCEEDS\n$evidence", "ok:200", outcome.fetch)
        assertTrue(
            "the fetch DID reach the network stack (a Network entry, query preserved)\n$evidence",
            probe.intercepted.any {
                it == "$INTERNAL_ORIGIN/blog/__data.json?x-sveltekit-invalidated=01"
            },
        )
        assertEquals("pushState to /blog/ SUCCEEDS\n$evidence", "ok:/blog/", outcome.pushState)
        assertTrue(
            "doUpdateVisitedHistory fires with the new URL (the SPA-nav signal)\n$evidence",
            probe.historyUpdates.any { it == "$INTERNAL_ORIGIN/blog/" },
        )
    }

    /**
     * The Kotlin -> JNI wiring of the ONE session-free map the Kotlin edge
     * needs ([WerustCore.toWebViewUrl], used by the `_blank`/`window.open`
     * transport): a hand-typed CIDv0 `ipfs://` URL maps onto the SAME content's
     * canonical base32 internal origin (Chromium lowercases hostnames, so a
     * mixed-case CIDv0 could never round-trip through a host label). The full
     * mapping rules are unit-tested in Rust (`origin_map.rs`); this pins that
     * the native export is actually linked and answers on-device.
     */
    @Test
    fun the_core_maps_an_ipfs_url_to_the_internal_origin_over_jni() {
        val core = WerustCore()
        try {
            assertEquals(
                "$INTERNAL_ORIGIN/blog/",
                core.toWebViewUrl("ipfs://$CID_V0/blog/"),
            )
            assertEquals(
                "a non-ipfs URL passes through unchanged",
                "https://example.com/",
                core.toWebViewUrl("https://example.com/"),
            )
        } finally {
            core.close()
        }
    }

    private companion object {
        /** The ronan.eth fixture root's canonical base32 CIDv1 (the form the ENS contenthash decoder produces). */
        const val CID_V1 = "bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq"

        /** The same content as a hand-typed CIDv0 (mixed case). */
        const val CID_V0 = "QmUsRTSHzVrxGNGc3scGFapuFa3NCELA7T6x356YmDjf79"

        /** The internal origin the fix serves the site under (see `origin_map.rs`). */
        const val INTERNAL_ORIGIN = "https://$CID_V1.ipfs.werust.invalid"

        /** The logcat tag the captured on-device evidence is logged under (quoted in DIAGNOSIS.md). */
        const val EVIDENCE_TAG = "SpaClientNavProbe"

        /**
         * The probe page: the smallest page that still does what a SvelteKit
         * `adapter-static` client navigation does on a blog-button click:
         * prevent the default anchor navigation, `fetch` the route's
         * `__data.json` (relative, exactly the query shape SvelteKit uses),
         * then `history.pushState` to the route. Every step's outcome is
         * recorded on `window.__result` for the harness to read back.
         */
        val PAGE_HTML = """
            <!doctype html>
            <html>
            <head><meta charset="utf-8"><title>probe</title></head>
            <body>
            <a id="blog" href="/blog/">blog</a>
            <script>
            document.getElementById('blog').addEventListener('click', function (e) {
              e.preventDefault();
              window.__clientNav();
            });
            window.__clientNav = async function () {
              var r = {};
              r.origin = String(window.location.origin);
              try {
                var resp = await fetch('/blog/__data.json?x-sveltekit-invalidated=01');
                r.fetch = 'ok:' + resp.status;
              } catch (e) {
                r.fetch = 'reject:' + e.name;
              }
              try {
                history.pushState({}, '', '/blog/');
                r.pushState = 'ok:' + location.pathname;
              } catch (e) {
                r.pushState = 'throw:' + e.name;
              }
              window.__result = JSON.stringify(r);
            };
            </script>
            </body>
            </html>
        """.trimIndent()

        /** The canned route data the interception answers for the `__data.json` fetch. */
        const val DATA_JSON = "[{\"type\":\"data\",\"nodes\":[]}]"
    }

    /** The three outcomes the probe page reports, parsed leniently (the evidence string keeps the raw JSON). */
    private data class Outcome(val raw: String, val origin: String, val fetch: String, val pushState: String)

    /**
     * A raw-`WebView` probe harness: serves [PAGE_HTML] + [DATA_JSON] for BOTH
     * the `ipfs://` scheme and the internal origin (so the before/after
     * mechanisms run side by side against the SAME canned bytes), and records
     * the three signal streams the diagnosis needed: intercepted requests
     * (the Network tab's source), console messages (the Console tab's source),
     * and `doUpdateVisitedHistory` (the SPA-nav signal).
     *
     * All `WebView` interaction is marshalled onto the main thread (the
     * `WebView` is single-threaded); instrumentation test methods run on a
     * dedicated test thread, so every call below synchronizes explicitly.
     */
    private class Probe {
        private val instrumentation = InstrumentationRegistry.getInstrumentation()
        private lateinit var webView: WebView

        /** Every URL `shouldInterceptRequest` was asked about (the would-be Network tab). */
        val intercepted = CopyOnWriteArrayList<String>()

        /** Every console message, `level: text` (the would-be Console tab). */
        val console = CopyOnWriteArrayList<String>()

        /** Every URL `doUpdateVisitedHistory` reported (the SPA-nav signal). */
        val historyUpdates = CopyOnWriteArrayList<String>()

        private var pageFinished = CountDownLatch(1)

        init {
            instrumentation.runOnMainSync {
                webView = WebView(instrumentation.targetContext)
                webView.settings.javaScriptEnabled = true
                webView.webViewClient = object : WebViewClient() {
                    override fun shouldInterceptRequest(
                        view: WebView,
                        request: WebResourceRequest,
                    ): WebResourceResponse? {
                        val url = request.url.toString()
                        val served = when {
                            url.startsWith("ipfs://") || url.startsWith(INTERNAL_ORIGIN) -> when {
                                url.contains("__data.json") -> DATA_JSON to "application/json"
                                else -> PAGE_HTML to "text/html"
                            }
                            else -> return null
                        }
                        intercepted.add(url)
                        return WebResourceResponse(
                            served.second,
                            "utf-8",
                            ByteArrayInputStream(served.first.toByteArray()),
                        )
                    }

                    override fun onPageFinished(view: WebView, url: String) {
                        pageFinished.countDown()
                    }

                    override fun doUpdateVisitedHistory(view: WebView, url: String, isReload: Boolean) {
                        historyUpdates.add(url)
                    }
                }
                webView.webChromeClient = object : WebChromeClient() {
                    override fun onConsoleMessage(message: ConsoleMessage): Boolean {
                        console.add("${message.messageLevel()}: ${message.message()}")
                        return false
                    }
                }
            }
        }

        /** Load [pageUrl], click the probe page's blog link, and wait for the outcome. */
        fun loadAndRunClientNav(pageUrl: String): Outcome {
            pageFinished = CountDownLatch(1)
            instrumentation.runOnMainSync { webView.loadUrl(pageUrl) }
            check(pageFinished.await(30, TimeUnit.SECONDS)) { "the probe page did not finish loading: $pageUrl" }

            // Only the signals the CLIENT NAV produces matter: a normal load
            // can itself fire `doUpdateVisitedHistory` / console lines, so the
            // streams are reset after the page settles and before the click.
            historyUpdates.clear()
            console.clear()

            // The link CLICK (not a direct call): the field symptom is a button
            // that does nothing, so drive the page the way the user does.
            evaluate("document.getElementById('blog').click(); 'clicked'")
            val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(15)
            var ready = ""
            while (System.nanoTime() < deadline) {
                ready = evaluate("window.__result ? 'ready' : ''")?.trim('"') ?: ""
                if (ready == "ready") break
                Thread.sleep(100)
            }
            check(ready == "ready") { "the probe page never reported an outcome" }
            // A same-document history update can land just after the JS result.
            Thread.sleep(500)
            val raw = evaluate("window.__result") ?: ""
            return Outcome(raw, field("origin"), field("fetch"), field("pushState"))
        }

        /** Read one field out of the probe page's outcome, evaluating it in the page. */
        private fun field(name: String): String =
            evaluate("JSON.parse(window.__result).$name")?.trim('"') ?: ""

        /** Evaluate [js] on the main thread and return the JSON-encoded result. */
        private fun evaluate(js: String): String? {
            val latch = CountDownLatch(1)
            val out = AtomicReference<String?>()
            instrumentation.runOnMainSync {
                webView.evaluateJavascript(js) { value ->
                    out.set(value)
                    latch.countDown()
                }
            }
            check(latch.await(30, TimeUnit.SECONDS)) { "evaluateJavascript never answered: $js" }
            return out.get()
        }

        fun destroy() {
            instrumentation.runOnMainSync { webView.destroy() }
        }
    }
}
