package com.github.wighawag.werust

import android.util.Log
import android.webkit.CookieManager
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.ByteArrayInputStream
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The ON-DEVICE PROBE for the Android web-storage bug (task
 * `android-enable-dom-storage-and-guard-web-platform-parity`, finding
 * `work/notes/findings/android-localstorage-is-null-dom-storage-never-enabled-2026-07-31.md`):
 * on real hardware, testing `mandalas.eth`, the site worked on desktop and
 * `window.localStorage` was **`null`** on Android.
 *
 * `null` is the FINGERPRINT that identified the cause. The web platform allows
 * exactly two answers for `window.localStorage`: a `Storage` object, or a
 * `SecurityError` throw on an opaque origin. `null` is neither, so it is
 * non-conformant AND it rules out the obvious suspect — Android is the one
 * platform where `ipfs://` is origin-MAPPED (`origin_map.rs`), so an opaque
 * origin looked likely, but an opaque origin THROWS. The real cause is that
 * Android's `WebSettings.domStorageEnabled` defaults to `false` (the `WebView`
 * is built for an app embedding a view, not for a browser) and the edge never
 * set it.
 *
 * This probe MEASURES the platform, side by side, against the REAL System
 * WebView: the same page on the same internal origin
 * (`https://<cid>.ipfs.werust.invalid`, the origin every `ipfs://` load actually
 * runs on) with `WebSettings` exactly as Android ships them, and then with the
 * ONE setting `BrowserActivity` now sets. It covers all three storage APIs a
 * dapp uses — `localStorage`, `sessionStorage` and `indexedDB` — plus cookie
 * behaviour, because a `localStorage` fix that leaves IndexedDB broken has fixed
 * half the problem.
 *
 * **This test DOES NOT run in CI.** There is no CI emulator leg in this repo:
 * like its sibling [SpaClientNavOriginTest] it is a hand-run on-device probe.
 * The half that runs on every push is the source-shape guard
 * `crates/werust-core/tests/web_storage_edge_wiring_shape.rs`, which pins that
 * the Android edge still enables DOM storage so a refactor of that settings
 * block cannot silently return `window.localStorage` to `null`. Run this half by
 * hand on a device or emulator with:
 *
 * ```
 * cd crates/werust-android && ./gradlew :app:connectedDebugAndroidTest
 * ```
 *
 * The captured measurements (emulator, API 36, System WebView 142) are recorded
 * in `docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/MEASUREMENTS.md`.
 */
@RunWith(AndroidJUnit4::class)
class WebStorageTest {

    private var probe: Probe? = null

    @After
    fun tearDown() {
        probe?.destroy()
        probe = null
    }

    /**
     * THE BUG, measured: with `WebSettings` exactly as Android ships them,
     * `window.localStorage` is `null` — not a `Storage` object, and not a
     * `SecurityError` throw. Neither of the two answers the web platform allows.
     *
     * The origin is a NORMAL tuple origin here (the internal
     * `https://<cid>.ipfs.werust.invalid` one every `ipfs://` load runs on, which
     * [SpaClientNavOriginTest] already proved is not opaque), so this isolates
     * the cause to the SETTING and nothing else.
     *
     * MEASURED SURPRISE, pinned here rather than assumed: `domStorageEnabled`
     * governs ONLY `localStorage` on this WebView. `sessionStorage` is a real
     * `Storage` object and round-trips even with the setting off. The historical
     * folklore that the switch gates "DOM storage" as a whole (and IndexedDB with
     * it) does NOT hold on the API levels this app ships against — which is why
     * the task measured instead of trusting a blog post. See
     * `docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/MEASUREMENTS.md`.
     */
    @Test
    fun the_shipped_webview_defaults_return_null_for_local_storage_which_is_non_conformant() {
        val probe = Probe(domStorageEnabled = false).also { this.probe = it }
        val measured = probe.measure()
        Log.i(EVIDENCE_TAG, "BEFORE (WebSettings as Android ships them):\n$measured")

        assertEquals(
            "with DOM storage off the System WebView returns `null` from window.localStorage " +
                "— neither a Storage object nor a SecurityError throw, i.e. non-conformant\n$measured",
            "null",
            measured.localStorage,
        )
        assertTrue(
            "and a page trying to USE it fails outright, which is the field symptom\n$measured",
            measured.localStorageRoundTrip.startsWith("throw:"),
        )
        assertEquals(
            "MEASURED: sessionStorage is NOT governed by this setting — it is a real Storage " +
                "object even with DOM storage off, so the field bug was localStorage-only\n$measured",
            "[object Storage]",
            measured.sessionStorage,
        )
        assertEquals(
            "MEASURED: sessionStorage round-trips with the setting off too\n$measured",
            "ok:$STORED_VALUE",
            measured.sessionStorageRoundTrip,
        )
    }

    /**
     * THE FIX, proven at the same seam: with the ONE setting `BrowserActivity`
     * now sets (`settings.domStorageEnabled = true`), `window.localStorage` is a
     * real `Storage` object and a set/get round-trip works — the conformant
     * behaviour the other four edges have had all along, because WebKitGTK,
     * WKWebView and WebView2 all enable DOM storage by default.
     */
    @Test
    fun enabling_dom_storage_gives_a_storage_object_that_round_trips() {
        val probe = Probe(domStorageEnabled = true).also { this.probe = it }
        val measured = probe.measure()
        Log.i(EVIDENCE_TAG, "AFTER (settings.domStorageEnabled = true):\n$measured")

        assertEquals(
            "window.localStorage must be a real Storage object\n$measured",
            "[object Storage]",
            measured.localStorage,
        )
        assertEquals(
            "window.sessionStorage must be a real Storage object too\n$measured",
            "[object Storage]",
            measured.sessionStorage,
        )
        assertEquals(
            "a localStorage set/get round-trip must work\n$measured",
            "ok:$STORED_VALUE",
            measured.localStorageRoundTrip,
        )
        assertEquals(
            "a sessionStorage set/get round-trip must work\n$measured",
            "ok:$STORED_VALUE",
            measured.sessionStorageRoundTrip,
        )
    }

    /**
     * The round-trip that actually matters to a dapp: what was written SURVIVES a
     * reload of the same origin. (`sessionStorage` deliberately survives too — a
     * reload keeps the same browsing session; it is a NEW WebView that would
     * not.)
     */
    @Test
    fun what_local_storage_kept_survives_a_reload_of_the_same_origin() {
        val probe = Probe(domStorageEnabled = true).also { this.probe = it }
        probe.measure()
        val afterReload = probe.measure()
        Log.i(EVIDENCE_TAG, "AFTER RELOAD (same origin, same WebView):\n$afterReload")

        assertEquals(
            "the value written before the reload must still be readable after it\n$afterReload",
            "ok:$STORED_VALUE",
            afterReload.localStorageBeforeThisLoad,
        )
    }

    /**
     * IndexedDB, MEASURED rather than assumed. Wallets and dapps use it heavily,
     * so "did the localStorage switch fix IndexedDB too, or does it need more?"
     * is a question this task refused to answer from a blog post. The assertion
     * is the one a dapp cares about: with the edge's settings, an IndexedDB
     * database opens and a record round-trips.
     *
     * The `domStorageEnabled = false` half is recorded as EVIDENCE (the exact
     * strings go in MEASUREMENTS.md) rather than asserted, because what the
     * platform does with IndexedDB under that setting is a platform fact this
     * probe reports, not a werust behaviour to pin.
     */
    @Test
    fun indexed_db_works_with_the_settings_the_edge_configures() {
        val probe = Probe(domStorageEnabled = true).also { this.probe = it }
        val measured = probe.measure()
        Log.i(EVIDENCE_TAG, "INDEXEDDB (settings.domStorageEnabled = true):\n$measured")

        assertTrue(
            "window.indexedDB must be present\n$measured",
            measured.indexedDb.startsWith("[object IDBFactory]"),
        )
        assertEquals(
            "an IndexedDB open + put + get round-trip must work\n$measured",
            "ok:$STORED_VALUE",
            measured.indexedDbRoundTrip,
        )
    }

    /**
     * IndexedDB with the SHIPPED defaults: the "does IndexedDB need more than
     * this switch?" question, answered ON-DEVICE on the API levels this app
     * supports instead of trusted from a blog post — because a `localStorage` fix
     * that leaves IndexedDB broken is half a fix.
     *
     * MEASURED ANSWER: it needs nothing. IndexedDB is a working `IDBFactory` that
     * opens a database and round-trips a record EVEN WITH `domStorageEnabled`
     * off, so the historical dependency does not hold here. Pinned rather than
     * merely logged, so a future WebView that DID couple the two would surface as
     * a red test on the next hand-run instead of as a field report.
     */
    @Test
    fun indexed_db_needs_nothing_from_this_switch_which_is_measured_not_assumed() {
        val probe = Probe(domStorageEnabled = false).also { this.probe = it }
        val measured = probe.measure()
        Log.i(EVIDENCE_TAG, "INDEXEDDB (WebSettings as Android ships them):\n$measured")

        assertTrue(
            "window.indexedDB is present regardless of domStorageEnabled\n$measured",
            measured.indexedDb.startsWith("[object IDBFactory]"),
        )
        assertEquals(
            "and an open + put + get round-trip works regardless of it too\n$measured",
            "ok:$STORED_VALUE",
            measured.indexedDbRoundTrip,
        )
    }

    /**
     * COOKIES, measured for the record: whether the `CookieManager` accepts
     * cookies at all, whether THIRD-PARTY cookies are accepted for this WebView,
     * and whether a `document.cookie` round-trip works.
     *
     * Third-party cookies being OFF by default is arguably CORRECT for a
     * privacy-focused browser, so this task deliberately changes nothing here and
     * records the position instead — see
     * `docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/MEASUREMENTS.md`.
     */
    @Test
    fun cookie_behaviour_is_measured_for_the_record() {
        val probe = Probe(domStorageEnabled = true).also { this.probe = it }
        val measured = probe.measure()
        val cookies = probe.cookiePolicy()
        Log.i(EVIDENCE_TAG, "COOKIES: $cookies\n$measured")

        assertTrue(
            "first-party cookies must round-trip through document.cookie\n$measured",
            measured.cookieRoundTrip.startsWith("ok:"),
        )
    }

    /**
     * The `WebSettings` AUDIT, measured rather than repeated from documentation.
     *
     * The root cause of the storage bug is general: Android's `WebView` defaults
     * are tuned for an app EMBEDDING a view, and werust is a browser, so several
     * of them are wrong for it. This reads the shipped default of every audited
     * setting off a fresh `WebView` so the audit note carries MEASURED values,
     * and so a WebView update that changes one shows up here.
     *
     * It asserts NOTHING and changes NOTHING: each of these is a user-visible UX
     * decision for a human, and the deliverable is the LIST. The recommendations
     * live in
     * `docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/WEBSETTINGS-AUDIT.md`.
     */
    @Test
    fun the_audited_websettings_defaults_are_measured_for_the_human_triaging_them() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val measured = AtomicReference("")
        instrumentation.runOnMainSync {
            val fresh = WebView(instrumentation.targetContext)
            val s = fresh.settings
            measured.set(
                """
                |webViewVersion: ${WebView.getCurrentWebViewPackage()?.versionName}
                |sdkInt: ${android.os.Build.VERSION.SDK_INT}
                |domStorageEnabled: ${s.domStorageEnabled}
                |databaseEnabled: ${s.databaseEnabled}
                |supportZoom: ${s.supportZoom()}
                |builtInZoomControls: ${s.builtInZoomControls}
                |displayZoomControls: ${s.displayZoomControls}
                |useWideViewPort: ${s.useWideViewPort}
                |loadWithOverviewMode: ${s.loadWithOverviewMode}
                |mediaPlaybackRequiresUserGesture: ${s.mediaPlaybackRequiresUserGesture}
                |textZoom: ${s.textZoom}
                |systemFontScale: ${instrumentation.targetContext.resources.configuration.fontScale}
                """.trimMargin()
            )
            fresh.destroy()
        }
        Log.i(EVIDENCE_TAG, "WEBSETTINGS DEFAULTS (a fresh WebView, nothing set):\n${measured.get()}")
    }

    private companion object {
        /** The ronan.eth fixture root's canonical base32 CIDv1 (as [SpaClientNavOriginTest] uses). */
        const val CID_V1 = "bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq"

        /** The internal origin every `ipfs://` load really runs on (see `origin_map.rs`). */
        const val INTERNAL_ORIGIN = "https://$CID_V1.ipfs.werust.invalid"

        /** The logcat tag the captured on-device evidence is logged under (quoted in MEASUREMENTS.md). */
        const val EVIDENCE_TAG = "WebStorageProbe"

        /** The value written and read back by every round-trip below. */
        const val STORED_VALUE = "werust-round-trip"

        /** The key every round-trip below writes under. */
        const val STORED_KEY = "werust-probe"

        /**
         * The probe page: it reports what each storage API IS (the platform's own
         * stringification, so `null` and `[object Storage]` are distinguishable
         * rather than collapsed into a boolean), then attempts a real round-trip
         * through each. It also reports what `localStorage` held BEFORE this load
         * wrote anything, which is what makes the reload test meaningful.
         */
        val PAGE_HTML = """
            <!doctype html>
            <html>
            <head><meta charset="utf-8"><title>web storage probe</title></head>
            <body>
            <script>
            window.__measure = async function () {
              var r = {};
              // WHAT each API IS. The web platform allows a Storage object or a
              // SecurityError throw; `null` is the non-conformance this measures.
              try { r.localStorage = String(window.localStorage); }
              catch (e) { r.localStorage = 'throw:' + e.name; }
              try { r.sessionStorage = String(window.sessionStorage); }
              catch (e) { r.sessionStorage = 'throw:' + e.name; }
              try { r.indexedDB = String(window.indexedDB); }
              catch (e) { r.indexedDB = 'throw:' + e.name; }

              // What a PREVIOUS load left behind (empty on a first load; the
              // reload test reads this).
              try {
                var kept = window.localStorage.getItem('$STORED_KEY');
                r.localStorageBeforeThisLoad = kept === null ? 'absent' : 'ok:' + kept;
              } catch (e) { r.localStorageBeforeThisLoad = 'throw:' + e.name; }

              // The ROUND-TRIPS a dapp actually performs.
              try {
                window.localStorage.setItem('$STORED_KEY', '$STORED_VALUE');
                r.localStorageRoundTrip = 'ok:' + window.localStorage.getItem('$STORED_KEY');
              } catch (e) { r.localStorageRoundTrip = 'throw:' + e.name; }
              try {
                window.sessionStorage.setItem('$STORED_KEY', '$STORED_VALUE');
                r.sessionStorageRoundTrip = 'ok:' + window.sessionStorage.getItem('$STORED_KEY');
              } catch (e) { r.sessionStorageRoundTrip = 'throw:' + e.name; }
              try {
                r.indexedDBRoundTrip = await new Promise(function (resolve) {
                  var open;
                  try { open = window.indexedDB.open('werust-probe-db', 1); }
                  catch (e) { resolve('throw:' + e.name); return; }
                  open.onerror = function () {
                    resolve('error:' + (open.error && open.error.name));
                  };
                  open.onblocked = function () { resolve('blocked'); };
                  open.onupgradeneeded = function () {
                    open.result.createObjectStore('kv');
                  };
                  open.onsuccess = function () {
                    var db = open.result;
                    try {
                      var tx = db.transaction('kv', 'readwrite');
                      tx.objectStore('kv').put('$STORED_VALUE', '$STORED_KEY');
                      tx.oncomplete = function () {
                        var read = db.transaction('kv', 'readonly').objectStore('kv')
                          .get('$STORED_KEY');
                        read.onsuccess = function () { resolve('ok:' + read.result); };
                        read.onerror = function () { resolve('error:read'); };
                      };
                      tx.onerror = function () {
                        resolve('error:' + (tx.error && tx.error.name));
                      };
                    } catch (e) { resolve('throw:' + e.name); }
                  };
                });
              } catch (e) { r.indexedDBRoundTrip = 'throw:' + e.name; }

              // COOKIES, first-party, through the document API.
              try {
                document.cookie = '$STORED_KEY=$STORED_VALUE; path=/';
                var found = document.cookie.indexOf('$STORED_KEY=$STORED_VALUE') >= 0;
                r.cookieRoundTrip = found ? 'ok:' + document.cookie : 'absent:' + document.cookie;
              } catch (e) { r.cookieRoundTrip = 'throw:' + e.name; }

              window.__result = JSON.stringify(r);
            };
            window.__measure();
            </script>
            </body>
            </html>
        """.trimIndent()
    }

    /** Everything one load of the probe page measured, as the page reported it. */
    private data class Measured(
        val raw: String,
        val localStorage: String,
        val sessionStorage: String,
        val indexedDb: String,
        val localStorageBeforeThisLoad: String,
        val localStorageRoundTrip: String,
        val sessionStorageRoundTrip: String,
        val indexedDbRoundTrip: String,
        val cookieRoundTrip: String,
    ) {
        override fun toString(): String = """
            |origin: $INTERNAL_ORIGIN
            |window.localStorage: $localStorage
            |window.sessionStorage: $sessionStorage
            |window.indexedDB: $indexedDb
            |localStorage before this load: $localStorageBeforeThisLoad
            |localStorage round-trip: $localStorageRoundTrip
            |sessionStorage round-trip: $sessionStorageRoundTrip
            |indexedDB round-trip: $indexedDbRoundTrip
            |document.cookie round-trip: $cookieRoundTrip
        """.trimMargin()
    }

    /**
     * A raw-`WebView` probe harness in [SpaClientNavOriginTest]'s style: it
     * serves [PAGE_HTML] on the INTERNAL origin through
     * `shouldInterceptRequest` (the same mechanism the real edge uses, so the
     * page runs on the same tuple origin a real `ipfs://` load does) and reads
     * back what the page measured.
     *
     * [domStorageEnabled] is the ONE variable: `false` is the `WebSettings` as
     * Android ships them (the bug), `true` is what `BrowserActivity` now sets
     * (the fix).
     *
     * All `WebView` interaction is marshalled onto the main thread (the `WebView`
     * is single-threaded); instrumentation test methods run on a dedicated test
     * thread, so every call below synchronizes explicitly.
     */
    private class Probe(private val domStorageEnabled: Boolean) {
        private val instrumentation = InstrumentationRegistry.getInstrumentation()
        private lateinit var webView: WebView
        private var pageFinished = CountDownLatch(1)

        init {
            instrumentation.runOnMainSync {
                webView = WebView(instrumentation.targetContext)
                webView.settings.javaScriptEnabled = true
                // The ONE variable under test. Left untouched it is Android's
                // shipped default (`false`), which is the bug.
                if (domStorageEnabled) webView.settings.domStorageEnabled = true
                webView.webViewClient = object : WebViewClient() {
                    override fun shouldInterceptRequest(
                        view: WebView,
                        request: WebResourceRequest,
                    ): WebResourceResponse? {
                        val url = request.url.toString()
                        if (!url.startsWith(INTERNAL_ORIGIN)) return null
                        return WebResourceResponse(
                            "text/html",
                            "utf-8",
                            ByteArrayInputStream(PAGE_HTML.toByteArray()),
                        )
                    }

                    override fun onPageFinished(view: WebView, url: String) {
                        pageFinished.countDown()
                    }
                }
            }
        }

        /** Load the probe page (again) and read back everything it measured. */
        fun measure(): Measured {
            pageFinished = CountDownLatch(1)
            instrumentation.runOnMainSync { webView.loadUrl("$INTERNAL_ORIGIN/") }
            check(pageFinished.await(30, TimeUnit.SECONDS)) { "the probe page did not finish loading" }

            val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(30)
            var ready = ""
            while (System.nanoTime() < deadline) {
                ready = evaluate("window.__result ? 'ready' : ''")?.trim('"') ?: ""
                if (ready == "ready") break
                Thread.sleep(100)
            }
            check(ready == "ready") { "the probe page never reported a measurement" }
            return Measured(
                raw = evaluate("window.__result") ?: "",
                localStorage = field("localStorage"),
                sessionStorage = field("sessionStorage"),
                indexedDb = field("indexedDB"),
                localStorageBeforeThisLoad = field("localStorageBeforeThisLoad"),
                localStorageRoundTrip = field("localStorageRoundTrip"),
                sessionStorageRoundTrip = field("sessionStorageRoundTrip"),
                indexedDbRoundTrip = field("indexedDBRoundTrip"),
                cookieRoundTrip = field("cookieRoundTrip"),
            )
        }

        /**
         * The platform COOKIE policy as this WebView sees it: whether cookies are
         * accepted at all, and whether THIRD-PARTY cookies are (they are off by
         * WebView default, which this task deliberately leaves alone — see the
         * recorded position in MEASUREMENTS.md).
         */
        fun cookiePolicy(): String {
            val manager = CookieManager.getInstance()
            val thirdParty = AtomicReference("")
            instrumentation.runOnMainSync {
                thirdParty.set(manager.acceptThirdPartyCookies(webView).toString())
            }
            return "acceptCookie=${manager.acceptCookie()} acceptThirdPartyCookies=${thirdParty.get()}"
        }

        /** Read one field out of the probe page's measurement, evaluating it in the page. */
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
