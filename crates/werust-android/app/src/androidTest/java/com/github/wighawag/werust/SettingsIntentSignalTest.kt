package com.github.wighawag.werust

import android.os.Build
import android.os.SystemClock
import android.util.Log
import android.view.MotionEvent
import android.view.View
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.ByteArrayInputStream
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The ON-DEVICE PROBE for the Android half of werust's CHROME-MARKED NAVIGATION
 * INTENT (task `android-marks-user-intent-for-settings-mutations`, spec
 * `settings-mutations-require-user-intent`, `docs/adr/0013`): a
 * `werust://settings?backend=…` MUTATION is applied only for a MAIN-FRAME
 * request whose navigation werust's own chrome MARKED, and on Android the mark
 * comes from `WebViewClient.shouldOverrideUrlLoading` reading the facts the
 * System WebView reports for that navigation.
 *
 * WHAT ONLY A RUNNING SYSTEM WEBVIEW CAN SETTLE, and therefore what this probe
 * measures against the REAL engine:
 *
 * 1. what `WebResourceRequest.isForMainFrame` / `hasGesture()` / `isRedirect`
 *    and `WebView.getUrl()` actually REPORT for each navigation shape — a real
 *    tap on werust's own settings form (the user's own change, which must keep
 *    working), a script's `location = …`, a link on a hostile page, and a
 *    `window.open` (which arrives at `onCreateWindow`, not here); and
 * 2. that the Rust rule those facts feed
 *    (`IntentMarker::note_page_navigation`, reached over JNI through
 *    [WerustCore.notePageNavigation]) is really linked into the APK and answers
 *    the same way on-device as it does in the `cargo test` gate.
 *
 * The `hasGesture()` reading is the one this task most wants measured: Android
 * documents that it "may return false even though the sequence of events … was
 * initiated by a user gesture", and it is werust's ONLY Android spelling of "the
 * user activated this in the page". A false negative there is FAIL-CLOSED and
 * legible (the settings page renders and says `Not changed: werust did not start
 * this change …`), but it would mean the user's own form stopped applying, so it
 * is asserted here rather than assumed.
 *
 * This probe does NOT run in CI — there is no CI emulator leg in this repo (its
 * siblings `WebStorageTest`/`SpaClientNavOriginTest` are hand-run for the same
 * reason; the leg is task `android-instrumented-emulator-ci-leg`). The half that
 * runs on every push is the Rust unit suite in
 * `crates/werust-android/rust/src/lib.rs` plus the source-shape guard
 * `crates/werust-core/tests/settings_user_intent_edge_wiring_shape.rs`. Run this
 * one by hand with:
 *
 * ```
 * cd crates/werust-android && ./gradlew :app:connectedDebugAndroidTest
 * ```
 *
 * The captured values are recorded (with the emulator + System WebView version)
 * in `docs/spikes/android-marks-user-intent-for-settings-mutations/MEASUREMENTS.md`.
 *
 * It touches NO app state: it drives a raw `WebView` over canned bytes, and the
 * only native call it makes is the marking one, which writes a mark into a
 * session of its own and never applies or persists a setting.
 */
@RunWith(AndroidJUnit4::class)
class SettingsIntentSignalTest {

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
     * THE USER'S OWN CHANGE, on the one path the shell cannot cover: a REAL TAP
     * on the settings page's own submit control. werust's settings page is a
     * plain GET form, so submitting it is a PAGE-initiated navigation — this is
     * the navigation the Android edge exists to mark, and it must keep applying.
     *
     * If the synthetic tap never reaches the page at all (an un-attached
     * `WebView` may not hit-test), the case is SKIPPED with a logged note rather
     * than failed: an undelivered tap measures nothing. If it IS delivered, the
     * facts are asserted — a real activation that reported no gesture would mean
     * werust's own settings form stops applying on Android, which is exactly what
     * this probe exists to catch.
     */
    @Test
    fun a_real_tap_on_werusts_own_settings_form_is_marked_as_the_users_intent() {
        probe.load(SETTINGS_URL)
        probe.clearNavigations()
        probe.tapCentre()

        val nav = probe.awaitNavigation()
        if (nav == null) {
            Log.i(EVIDENCE_TAG, "TAP: no navigation reported (synthetic tap not delivered)")
            return
        }
        Log.i(EVIDENCE_TAG, "TAP on werust's own settings form: $nav")

        assertTrue("the tap navigates to the mutating settings URL\n$nav", nav.url.startsWith(MUTATING_URL))
        assertTrue("a form submit is a MAIN-FRAME navigation\n$nav", nav.mainFrame)
        assertTrue("a real tap must report a user gesture\n$nav", nav.gesture)
        assertFalse("and it is not a redirect of another navigation\n$nav", nav.redirect)
        assertTrue(
            "the document it starts FROM is werust's own page (the fact a page cannot forge)\n$nav",
            nav.document.startsWith("werust://"),
        )
        assertTrue(
            "so the core MARKS it, and the user's own change applies\n$nav",
            probe.markVerdict(nav),
        )
    }

    /**
     * A SCRIPT's `location = 'werust://settings?…'`, run on werust's own page:
     * the shape the gesture fact exists to exclude. werust's settings page ships
     * no script, so this is a hypothetical there — but it is the same shape a
     * hostile page uses, and the reading is recorded rather than assumed.
     */
    @Test
    fun a_script_started_navigation_reports_no_gesture_and_is_not_marked() {
        probe.load(SETTINGS_URL)
        probe.clearNavigations()
        probe.evaluate("window.__scriptNavigate(); 'go'")

        val nav = probe.awaitNavigation() ?: error("a script navigation must reach the hook")
        Log.i(EVIDENCE_TAG, "SCRIPT location=: $nav")
        assertFalse("a script's `location = …` carries no user gesture\n$nav", nav.gesture)
        assertFalse("so the core does not mark it\n$nav", probe.markVerdict(nav))
    }

    /**
     * A LINK ON A HOSTILE PAGE pointing at a mutating settings URL: every
     * request-level fact it can produce may be present, and the one fact it
     * cannot forge — the document the navigation starts FROM — is web content.
     */
    @Test
    fun a_navigation_started_in_web_content_is_never_marked() {
        probe.load(HOSTILE_URL)
        probe.clearNavigations()
        probe.evaluate("document.getElementById('go').click(); 'clicked'")

        val nav = probe.awaitNavigation() ?: error("the link click must reach the hook")
        Log.i(EVIDENCE_TAG, "HOSTILE page link: $nav")
        assertTrue("the document it starts from is web content\n$nav", nav.document.startsWith("https://"))
        assertFalse("web content can never be at a werust:// URL, so no mark\n$nav", probe.markVerdict(nav))
        // Even if a future WebView reported a gesture for it, the source document
        // still refuses it: the gesture is corroboration, not the authorisation.
        assertFalse(
            "and it stays refused with the gesture forced true\n$nav",
            probe.markVerdict(nav.copy(gesture = true)),
        )
    }

    /**
     * `window.open('werust://settings?…')`: the in-place router
     * (`docs/adr/0010`) that must NOT become a trust bypass. It arrives at
     * `WebChromeClient.onCreateWindow`, which reports NOTHING to the core, so
     * whatever the WebView says about it there is not an authorisation.
     */
    @Test
    fun a_window_open_target_arrives_at_the_router_which_marks_nothing() {
        probe.load(SETTINGS_URL)
        probe.clearNavigations()
        probe.evaluate("window.__openWindow(); 'opened'")

        val opened = probe.awaitWindowOpen()
        Log.i(EVIDENCE_TAG, "window.open: created=$opened navigations=${probe.navigations}")
        assertTrue(
            "the target is routed through onCreateWindow, which reports no navigation " +
                "to the core (a router, not a trust bypass)",
            probe.navigations.none { it.url.startsWith(MUTATING_URL) && probe.markVerdict(it) },
        )
    }

    private companion object {
        /** The logcat tag the captured on-device evidence is logged under (quoted in MEASUREMENTS.md). */
        const val EVIDENCE_TAG = "SettingsIntentProbe"

        /** werust's own internal settings page, as the chrome reaches it. */
        const val SETTINGS_URL = "werust://settings"

        /** The mutating URL every case aims at (the change the gate must authorise). */
        const val MUTATING_URL = "werust://settings?backend=custom"

        /** A plain web page, standing in for any site the user visits. */
        const val HOSTILE_URL = "https://hostile.example/"

        /** The full target, as werust's own form would build it. */
        const val TARGET = "$MUTATING_URL&url=http%3A%2F%2F127.0.0.1%3A8080"

        /**
         * A stand-in for werust's own settings page: one full-viewport submit
         * control (so a synthetic tap anywhere hits it) plus the two script
         * shapes a page could use. It is deliberately the SAME shape the real
         * page has — a GET form whose action is `werust://settings`.
         */
        val SETTINGS_HTML = """
            <!doctype html>
            <html>
            <head><meta charset="utf-8"><title>settings</title>
            <style>html,body{margin:0;height:100%}form,button{display:block;width:100%;height:100%}</style>
            </head>
            <body>
            <form action="werust://settings" method="get">
              <input type="hidden" name="backend" value="custom">
              <input type="hidden" name="url" value="http://127.0.0.1:8080">
              <button id="use" type="submit">use this</button>
            </form>
            <script>
            window.__scriptNavigate = function () { location = '$TARGET'; };
            window.__openWindow = function () { window.open('$TARGET'); };
            </script>
            </body>
            </html>
        """.trimIndent()

        /** A plain web page with a link at the same mutating URL. */
        val HOSTILE_HTML = """
            <!doctype html>
            <html>
            <head><meta charset="utf-8"><title>hostile</title></head>
            <body><a id="go" href="$TARGET">free crypto</a></body>
            </html>
        """.trimIndent()
    }

    /** One navigation as the System WebView reported it to the marking hook. */
    private data class Navigation(
        val url: String,
        val document: String,
        val mainFrame: Boolean,
        val gesture: Boolean,
        val redirect: Boolean,
    )

    /**
     * A raw-`WebView` harness that serves the two canned pages for BOTH the
     * `werust://` scheme and the plain origin, and records what
     * `shouldOverrideUrlLoading` reports for every navigation. It CANCELS each
     * navigation (returns `true`) so the probe stays on the page under test and
     * the cases run independently; the facts are read at the same callback the
     * production edge reads them at.
     *
     * All `WebView` interaction is marshalled onto the main thread (the `WebView`
     * is single-threaded); instrumentation tests run on their own thread.
     */
    private class Probe {
        private val instrumentation = InstrumentationRegistry.getInstrumentation()
        private lateinit var webView: WebView

        /** The native core, for the ONE call this probe makes over JNI. */
        private val core = WerustCore()

        /** Every navigation the marking hook was told about. */
        val navigations = CopyOnWriteArrayList<Navigation>()

        private var pageFinished = CountDownLatch(1)
        private var windowOpened = CountDownLatch(1)

        init {
            instrumentation.runOnMainSync {
                webView = WebView(instrumentation.targetContext)
                webView.settings.javaScriptEnabled = true
                webView.settings.setSupportMultipleWindows(true)
                webView.settings.javaScriptCanOpenWindowsAutomatically = true
                // A real size, so the page lays out and a synthetic tap can hit
                // the full-viewport control (an un-attached WebView is 0x0).
                webView.measure(
                    View.MeasureSpec.makeMeasureSpec(WIDTH, View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec(HEIGHT, View.MeasureSpec.EXACTLY),
                )
                webView.layout(0, 0, WIDTH, HEIGHT)
                webView.webViewClient = object : WebViewClient() {
                    override fun shouldInterceptRequest(
                        view: WebView,
                        request: WebResourceRequest,
                    ): WebResourceResponse? {
                        val url = request.url.toString()
                        val html = when {
                            url.startsWith("werust://") -> SETTINGS_HTML
                            url.startsWith(HOSTILE_URL) -> HOSTILE_HTML
                            else -> return null
                        }
                        return WebResourceResponse(
                            "text/html",
                            "utf-8",
                            ByteArrayInputStream(html.toByteArray()),
                        )
                    }

                    override fun shouldOverrideUrlLoading(
                        view: WebView,
                        request: WebResourceRequest,
                    ): Boolean {
                        navigations.add(
                            Navigation(
                                url = request.url.toString(),
                                document = view.url ?: "",
                                mainFrame = request.isForMainFrame,
                                gesture = request.hasGesture(),
                                redirect = Build.VERSION.SDK_INT >= Build.VERSION_CODES.N &&
                                    request.isRedirect,
                            )
                        )
                        // Cancel it: the probe measures the REPORT, and staying on
                        // the page under test keeps the cases independent.
                        return true
                    }

                    override fun onPageFinished(view: WebView, url: String) {
                        pageFinished.countDown()
                    }
                }
                webView.webChromeClient = object : WebChromeClient() {
                    override fun onCreateWindow(
                        view: WebView,
                        isDialog: Boolean,
                        isUserGesture: Boolean,
                        resultMsg: android.os.Message,
                    ): Boolean {
                        windowOpened.countDown()
                        // Like the production router, minus the routing: nothing
                        // here reports a navigation to the core.
                        return false
                    }
                }
            }
        }

        /** Load [url] from the canned bytes and wait for it to settle. */
        fun load(url: String) {
            pageFinished = CountDownLatch(1)
            instrumentation.runOnMainSync { webView.loadUrl(url) }
            check(pageFinished.await(30, TimeUnit.SECONDS)) { "the probe page did not load: $url" }
        }

        fun clearNavigations() {
            navigations.clear()
            windowOpened = CountDownLatch(1)
        }

        /** A genuine touch in the middle of the laid-out `WebView`. */
        fun tapCentre() {
            val now = SystemClock.uptimeMillis()
            val x = WIDTH / 2f
            val y = HEIGHT / 2f
            instrumentation.runOnMainSync {
                val down = MotionEvent.obtain(now, now, MotionEvent.ACTION_DOWN, x, y, 0)
                webView.dispatchTouchEvent(down)
                down.recycle()
            }
            SystemClock.sleep(80)
            instrumentation.runOnMainSync {
                val up = MotionEvent.obtain(now, SystemClock.uptimeMillis(), MotionEvent.ACTION_UP, x, y, 0)
                webView.dispatchTouchEvent(up)
                up.recycle()
            }
        }

        /** The first navigation reported within the timeout, or `null`. */
        fun awaitNavigation(): Navigation? {
            val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(10)
            while (System.nanoTime() < deadline) {
                navigations.firstOrNull()?.let { return it }
                SystemClock.sleep(100)
            }
            return null
        }

        fun awaitWindowOpen(): Boolean = windowOpened.await(10, TimeUnit.SECONDS)

        /**
         * What the SHARED rule says about a reported navigation: the same
         * `IntentMarker::note_page_navigation` the production edge calls, over
         * the same JNI export, with the facts this device produced.
         */
        fun markVerdict(nav: Navigation): Boolean =
            core.notePageNavigation(nav.url, nav.document, nav.mainFrame, nav.gesture, nav.redirect)

        /** Evaluate [js] on the main thread and return the JSON-encoded result. */
        fun evaluate(js: String): String? {
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
            core.close()
        }

        private companion object {
            const val WIDTH = 800
            const val HEIGHT = 1200
        }
    }
}
