package com.github.wighawag.werust

import android.annotation.SuppressLint
import android.app.AlertDialog
import android.content.pm.ApplicationInfo
import android.content.res.ColorStateList
import android.graphics.Bitmap
import android.graphics.Paint
import android.os.Build
import android.os.Bundle
import android.os.Message
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.view.accessibility.AccessibilityNodeInfo
import android.view.inputmethod.EditorInfo
import android.webkit.ConsoleMessage
import android.webkit.JavascriptInterface
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebChromeClient
import android.webkit.WebView
import android.webkit.WebViewClient
import java.io.ByteArrayInputStream
import android.widget.Button
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.PopupMenu
import android.widget.ProgressBar
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import kotlin.math.roundToInt

/**
 * The Android OS edge: a real `Activity` with a URL bar, ONE reload/stop control
 * and a loading spinner over a live, interactive [WebView].
 *
 * This is the forced OS edge and NOTHING more: it owns the platform `WebView` and
 * the widgets, but every browsing DECISION is the Rust [WerustCore]'s. On a user
 * action it drives the core, then (1) applies whatever URL the core surfaces to
 * the `WebView` ([syncPendingLoad]) and (2) repaints its chrome from the core's
 * [WerustCore.Chrome] ([refreshChrome]). The `WebView`'s real load-lifecycle
 * callbacks are reported straight back into the core, which folds them into the
 * chrome exactly as the desktop GTK pump folds WebKitGTK's signals. The URL bar
 * text, the reload/stop control's mode, the spinner and the load status are all
 * read from the core — the edge keeps no history or load state of its own.
 *
 * There are NO on-screen back/forward buttons: the platform's own Back (hardware
 * or gesture) already navigates page history through [systemBackCallback], so a
 * toolbar `◀` would spend scarce phone width duplicating it, and forward — which
 * has no Android gesture at all — is given up for that width deliberately (spec
 * `chrome-conventional-controls`, stories 11/12; task
 * `android-chrome-collapse-reload-stop-and-drop-history-buttons`). The history
 * CAPABILITY is untouched: `can_go_back` / `can_go_forward` and the core's
 * history actions are exactly as they were, and system Back rides on them.
 *
 * The SYSTEM Back button is therefore the ONE back affordance this edge has (see
 * [systemBackCallback]); the Activity is an
 * [androidx.activity.ComponentActivity] (rather than a bare `android.app.Activity`)
 * ONLY so the non-deprecated [androidx.activity.OnBackPressedDispatcher] is
 * available — see
 * `docs/spikes/android-hardware-back-button-navigates-history/README.md`.
 */
class BrowserActivity : ComponentActivity() {

    private val core = WerustCore()

    /**
     * A single background thread the blocking session-DRIVING actions
     * ([WerustCore.navigate] / [WerustCore.goBack] / [WerustCore.goForward] /
     * [WerustCore.reload]) run on, so they NEVER block the Android UI thread.
     *
     * WHY (task `android-anr-main-thread-diagnose-and-unblock`, field finding B,
     * `docs/spikes/android-anr-main-thread-diagnose-and-unblock/DIAGNOSIS.md`): a
     * bare `.eth` navigation resolves the ENS/IPNS name INLINE inside the shared
     * core's `navigate`, with BLOCKING network I/O (two sequential `eth_call`s,
     * plus an IPNS record fetch), each up to the 30s RPC timeout. Run on the UI
     * thread that blocked the main thread for seconds and tripped Android's ANR
     * watchdog REGULARLY ("isn't responding", recurring, while the bar stayed
     * typeable). Driving those actions off the UI thread here — then posting the
     * cheap WebView/widget updates BACK to the UI thread ([afterCoreAction]) —
     * keeps the main thread idle between frames. This is a THREADING/cadence fix
     * only: the SAME core methods run in the SAME order and return the SAME
     * chrome; the `SyncSession` mutex still serialises every native call, so no
     * trust/lifecycle/verification behaviour changes. It is the resolution-side
     * twin of `ipfs-retrieval-off-main-thread-no-ui-freeze` (which moved the
     * `ipfs://` content retrieval off the handler thread). Single-threaded so a
     * user's rapid actions serialise in order rather than racing each other.
     */
    private val coreExecutor: ExecutorService = Executors.newSingleThreadExecutor()

    private lateinit var urlBar: EditText

    /**
     * The ONE reload/stop control: the single button every browser has, which
     * reloads a settled page and STOPS a load in flight. It replaces the separate
     * Reload and Stop buttons, which were one fact ([WerustCore.Chrome.loading])
     * wearing two widgets, each enabled on the negation of the other's condition.
     *
     * WHICH MODE it is in is the core's call
     * (`werust_core::reload_stop_control`), carried on the chrome JSON: this edge
     * ASSIGNS the glyph ([WerustCore.Chrome.reloadStopControlLabel]) and the
     * accessible name ([WerustCore.Chrome.reloadStopControlDescription]) in
     * [refreshChrome], and its click activates
     * [WerustCore.activateReloadStopControl], where the SAME derivation decides
     * what the click does. So nothing here branches on the load state — that
     * `when` is the hand-written twin `docs/adr/0011` removed from this edge, and
     * a guard reds the gate if one returns.
     *
     * CANCEL is unmoved by the collapse: this control is in its STOP mode on
     * exactly the fact the separate Stop button took its sensitivity from.
     */
    private lateinit var reloadStopButton: Button

    /**
     * The LOADING SPINNER beside the control: an indeterminate [ProgressBar] that
     * says werust is WORKING, the half the URL-bar progress line cannot say. The
     * bar reports how FAR a load got; a load that has reported no phase yet paints
     * a bare sliver, and the long pre-content name-resolution window (the
     * `ronan.eth` wait) is exactly where a user needs to see motion.
     *
     * Its visibility is the core's [WerustCore.Chrome.loadSpinnerVisible] and
     * nothing else. Its slot is PERMANENT (INVISIBLE, never GONE) so a load start
     * or end can never shove the URL bar sideways — the horizontal twin of the
     * geometry lesson [loadingProgress] learned vertically. No timer either: a
     * `ProgressBar` animates itself and stops when it is not visible, so the
     * existing chrome-refresh pump drives it and the Android ANR guard is
     * untouched.
     */
    private lateinit var loadingSpinner: ProgressBar

    /**
     * The GENERAL browser menu affordance: the ⋮ button every browser has, at the
     * end of the toolbar, opening a [PopupMenu] of the SHARED core's menu items
     * (task `general-browser-menu-with-version-and-debug-entry`).
     *
     * USER-FACING and always available — deliberately NOT debug-build-gated (the
     * native `chrome://inspect` inspector above is; this menu is not). It is a
     * CONTAINER meant to GROW: [showBrowserMenu] iterates whatever items the core
     * lists, so a future bookmarks/settings entry is a `werust-core` change plus
     * (only if it is an action) one branch in [onBrowserMenuItem].
     */
    private lateinit var menuButton: Button
    private lateinit var status: TextView
    private lateinit var trust: TextView
    private lateinit var errorBanner: TextView
    /**
     * The LOAD-PROGRESS line (the fine-grained loading signal, beside the coarse
     * [loadingSpinner]): a thin determinate bar directly under the URL-bar row,
     * whose progress advances with the real pipeline phase while a load is in flight
     * and which goes INVISIBLE (never GONE) once the load settles. It replaces the
     * loading BANNER, which was a full-height bar in this same vertical chrome:
     * showing/hiding it resized the weighted WebView on every navigation, so the
     * page jumped twice per load. Keeping the strip INVISIBLE rather than GONE means
     * its height is reserved permanently, so no load state changes the layout (task
     * `loading-progress-in-the-url-bar-not-a-banner`). CANCEL is the toolbar's ONE
     * reload/stop control ([reloadStopButton]), in its STOP mode exactly while a
     * load is in flight; the phase NAME stays in the
     * footer status line, which already names it. Driven by the existing
     * chrome-refresh pump (no new timer / poll / tight loop), so the Android ANR
     * guard is not regressed.
     */
    private lateinit var loadingProgress: ProgressBar
    private lateinit var invalidBadge: TextView
    private lateinit var webView: WebView

    /**
     * The IN-APP DEBUG VIEW: the full-screen tabbed Console + Network screen the
     * browser menu's Debug entry opens (task
     * `debug-view-console-network-tabs-mobile`). Built once, laid out MATCH_PARENT
     * OVER the whole browser chrome (an overlay on the root container), and hidden
     * until [openDebugView] shows it. It renders the ONE shared capture store over
     * the FFI ([WerustCore.debugJson]), so it needs this Activity's session: an
     * overlay, not a separate Activity (a session cannot cross an Activity
     * boundary, and an overlay needs no manifest entry). The SYSTEM Back button
     * closes it ([debugBackCallback]).
     */
    private lateinit var debugView: DebugView

    /**
     * The URL bar's DEFAULT text colour, captured once at creation so the
     * invalid-entry red can be reverted to it (rather than hard-coding a colour
     * that would fight the OS light/dark theme). Restored whenever the entry is
     * valid again.
     */
    private var defaultUrlBarColor: Int = 0

    /**
     * The EIP-1193 provider bridge preamble + the `werust-core` provider shim,
     * bundled once and injected at document start on every page so a page's
     * `window.ethereum` is the injected native provider. Empty if the core has no
     * provider shim to inject.
     */
    private var providerScript: String = ""

    /**
     * The SYSTEM/hardware Back handler: makes the Android Back button/gesture go
     * BACK ONE PAGE in history instead of exiting the app.
     *
     * WHY (task `android-hardware-back-button-navigates-history`, field finding
     * v0.2.5 "the android back button do not navigate back in history like it
     * should"): before this, only an on-screen `◀` button drove
     * [WerustCore.goBack]; nothing handled system Back, so it fell through to the
     * platform default and FINISHED the Activity even mid-history — the app just
     * quit.
     *
     * Since task `android-chrome-collapse-reload-stop-and-drop-history-buttons`
     * this is the ONLY back affordance on Android: the on-screen button is gone,
     * because duplicating the platform's own Back is not worth phone toolbar
     * width. That makes the wiring below load-bearing on its own rather than a
     * second view of a button's, which is why it keeps its own guard
     * (`crates/werust-android/rust/tests/system_back_wiring_shape.rs`).
     *
     * HOW IT STAYS FAITHFUL TO THE CORE:
     * * [OnBackPressedCallback.isEnabled] is the core's `chrome.canGoBack`, set
     *   in [refreshChrome] from the core's own capability fact. It starts
     *   DISABLED (no history at launch).
     * * When it is DISABLED (nothing to go back to) the dispatcher falls through
     *   to the platform default, so Back at the start of history EXITS the app,
     *   as a normal browser does.
     * * [handleOnBackPressed] drives the core through [driveCore] — the SAME
     *   off-UI-thread path every other session-driving action uses — so this Back
     *   entry point does NOT reintroduce a UI-thread-blocking core call and the
     *   ANR fix (task `android-anr-main-thread-diagnose-and-unblock`) is not
     *   regressed.
     *
     * WHY THIS API: [androidx.activity.OnBackPressedDispatcher] is the
     * non-deprecated route and the ONE implementation that works across versions
     * — it bridges to the Android 13+ `OnBackInvokedDispatcher` when the app opts
     * into predictive back (`android:enableOnBackInvokedCallback`, deliberately
     * NOT opted into yet: see the recorded decision at
     * `docs/spikes/android-hardware-back-button-navigates-history/README.md`), and
     * uses the legacy dispatch below that. The deprecated `onBackPressed()`
     * override is NOT used.
     */
    private val systemBackCallback = object : OnBackPressedCallback(false) {
        override fun handleOnBackPressed() {
            driveCore { core.goBack() }
        }
    }

    /**
     * The SYSTEM/hardware Back handler for the IN-APP DEBUG VIEW: while the debug
     * view is open, Back CLOSES it (the way back to the page) instead of
     * navigating page history or exiting. Registered AFTER [systemBackCallback],
     * and the dispatcher consults the most recently registered enabled callback
     * first, so an enabled [debugBackCallback] always wins over the history one
     * while the view is open. Enabled only while the debug view is open
     * ([openDebugView] / [closeDebugView]); disabled it falls through to the
     * history callback exactly as before.
     */
    private val debugBackCallback = object : OnBackPressedCallback(false) {
        override fun handleOnBackPressed() {
            closeDebugView()
        }
    }

    /**
     * The trust surface's TRUST-ON-FIRST-USE lines and action, cached from the
     * last [refreshChrome] so [showTrustExplanation] can build the surface WITHOUT
     * a fresh chrome read on the UI thread (which takes the native session lock,
     * and can wait behind an in-flight `ipfs://` retrieval, the ANR guard). All
     * three are the core's derivation, carried on the chrome JSON; this edge only
     * holds onto them between a paint and a tap.
     */
    private var trustPinDetailText: String = ""
    private var trustPinActionOffered: Boolean = false
    private var trustPinActionLabelText: String = ""

    /**
     * The trust EXPLANATION (the core's `trust_indicator_detail`), cached from the
     * last [refreshChrome] for the two places that surface it: the badge's
     * accessibility STATE description and the [showTrustExplanation] dialog.
     *
     * It is a FIELD rather than a widget property because the badge's own
     * accessibility text is no longer free to hold it: the `contentDescription` is
     * the badge's short STATE name, so a screen reader announces WHICH posture
     * this is first and always (task
     * `mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay`),
     * and Android's secondary slot (`AccessibilityNodeInfo.setStateDescription`)
     * is write-only from a node the framework hands us. Like the TOFU strings
     * above, it is the core's derivation held only between a paint and a tap.
     */
    private var trustDetailText: String = ""

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val browserChrome = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        // The toolbar row is at least a touch-target tall (48dp), so even though the
        // toolbar buttons render as a small square glyph they stay comfortably tappable.
        val toolbar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = dp(TOUCH_TARGET_DP)
        }

        // The core's chrome BEFORE anything is painted: the first paint of every
        // derived value below is the core's OWN derivation for the starting state,
        // never a Kotlin literal that happens to match it today.
        val initialChrome = core.chrome()

        // The ONE reload/stop control, where four buttons used to sit. There is no
        // on-screen ◀ / ▶ any more (the platform's own Back navigates history and
        // forward has no Android gesture to duplicate — spec
        // `chrome-conventional-controls`, stories 11/12), and Reload/Stop are the
        // single control every browser shows. Its glyph, its accessible name and
        // what its click DOES are all the core's one derivation
        // (`reload_stop_control`): this edge assigns values and dispatches, it
        // never asks which mode this is.
        //
        // It goes through `driveCore` like every other session-driving action: a
        // reload re-runs the load path (which may resolve a name over the network),
        // and even the stop mode takes the native session lock, so neither belongs
        // on the UI thread (the ANR guard, task
        // `android-anr-main-thread-diagnose-and-unblock`).
        reloadStopButton = compactNavButton(initialChrome.reloadStopControlLabel) {
            driveCore { core.activateReloadStopControl() }
        }
        reloadStopButton.contentDescription = initialChrome.reloadStopControlDescription
        // The LOADING SPINNER, immediately after the control and before the URL
        // bar: the toolbar's two loading surfaces (the control that stops a load,
        // the spinner that reports one) stay together, and neither goes near the
        // trust badge, which must never read as a claim about the load. The layout
        // the shared decision record fixes for every edge
        // (docs/spikes/reload-stop-collapse-and-loading-spinner-core-and-gtk/DECISIONS.md).
        //
        // Its slot is permanent: only its VISIBILITY follows the derivation, and
        // INVISIBLE (never GONE) keeps the width reserved, so starting or finishing
        // a load never shifts the URL bar. It carries no accessibility text of its
        // own: the load is already announced by the progress line's content
        // description (the core's phase hint) and named in the footer status line,
        // and a third announcement of the same fact is noise for a screen-reader
        // user, so the spinner is a decoration TalkBack skips.
        loadingSpinner = ProgressBar(
            this, null, android.R.attr.progressBarStyleSmall
        ).apply {
            isIndeterminate = true
            indeterminateTintList = ColorStateList.valueOf(0xFF1A5FB4.toInt())
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            layoutParams = LinearLayout.LayoutParams(dp(SPINNER_DP), dp(SPINNER_DP))
            visibility = View.INVISIBLE
        }
        // The general browser menu button. Opening the menu is a cheap, non-
        // blocking read of a BUILD constant (no session, no network), so it runs
        // inline on the UI thread — it cannot regress the ANR fix the way a core
        // session-driving action would.
        menuButton = compactNavButton("⋮") { showBrowserMenu() }
        urlBar = EditText(this).apply {
            hint = "Enter a URL and press Enter"
            layoutParams = LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f)
            imeOptions = EditorInfo.IME_ACTION_GO
            setSingleLine()
            setOnEditorActionListener { _, actionId, _ ->
                if (actionId == EditorInfo.IME_ACTION_GO) {
                    val entry = text.toString()
                    driveCore { core.navigate(entry) }
                    true
                } else {
                    false
                }
            }
        }

        // The small "invalid URL" badge sits in the toolbar next to the URL bar,
        // shown ONLY when the last entry was invalid (field finding D). Starts
        // hidden; when shown it pairs with the URL bar's text rendered invalid (a
        // red underline). The SAME surface desktop shows, from the same chrome fact.
        // Its WORDING is the core's `invalid_entry_badge_text`, painted in
        // [refreshChrome], not a Kotlin literal here (that literal existed, and its
        // iOS twin was set once at build and never refreshed at all).
        invalidBadge = TextView(this).apply {
            setTextColor(0xFFC01C28.toInt())
            gravity = Gravity.CENTER_VERTICAL
            visibility = View.GONE
        }

        // Capture the URL bar's default text colour so the invalid-entry red can be
        // reverted to it (keeping the OS light/dark theme's colour, not a hard-coded
        // one).
        defaultUrlBarColor = urlBar.currentTextColor

        toolbar.addView(reloadStopButton)
        toolbar.addView(loadingSpinner)
        // The URL bar is the weighted member of the row, so the width the two
        // history buttons and the second reload/stop button gave up goes to it
        // with no arithmetic anywhere — the point of removing them on the
        // width-starved surface (spec story 11).
        toolbar.addView(urlBar)
        toolbar.addView(invalidBadge)
        // The ⋮ menu sits at the END of the toolbar, where every other browser
        // puts it.
        toolbar.addView(menuButton)

        webView = WebView(this).apply {
            layoutParams = LinearLayout.LayoutParams(MATCH_PARENT, 0, 1f)
            settings.javaScriptEnabled = true
            // WEB STORAGE (task `android-enable-dom-storage-and-guard-web-platform-parity`,
            // field finding: on `mandalas.eth` `window.localStorage` was `null` on
            // Android while desktop worked).
            //
            // WHY THE DEFAULT IS WRONG HERE: Android's `WebSettings` default for
            // `domStorageEnabled` is FALSE, because a `WebView` is built for an APP
            // EMBEDDING a web view — where a page having durable per-origin storage is
            // an opt-in — not for a BROWSER, where it is table stakes. werust is a
            // browser, so the default is simply wrong for it. And what the WebView does
            // with the setting off is not even conformant: `window.localStorage` comes
            // back `null`, where the web platform allows exactly two answers, a
            // `Storage` object or a `SecurityError` throw on an opaque origin. That
            // `null` is also what ruled OUT the obvious suspect — an opaque origin
            // THROWS, so the `ipfs://` origin mapping was never the cause and was left
            // alone. The other four edges (WebKitGTK, WKWebView on iOS and macOS,
            // WebView2) set no storage setting at all because their engines enable it
            // by default, which is why the gap was Android-only.
            //
            // WHY IT IS SAFE HERE: web storage is partitioned by ORIGIN, so enabling it
            // is only safe while two different sites cannot share an origin. On Android
            // they cannot: `crates/werust-android/rust/src/origin_map.rs` gives each CID
            // its OWN subdomain (`https://<cid>.ipfs.werust.invalid`), so storage is
            // isolated per CONTENT ADDRESS, exactly as it is on the four platforms that
            // serve real `ipfs://<cid>` origins. Flipping this switch leaks nothing
            // across sites.
            //
            // SCOPE, measured rather than assumed (on-device evidence in
            // docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/MEASUREMENTS.md):
            // this setting governs `localStorage` ONLY. `sessionStorage`, IndexedDB and
            // cookies already worked without it. Guarded by the source-shape test
            // crates/werust-core/tests/web_storage_edge_wiring_shape.rs (which runs on
            // every push) and by the on-device probe `WebStorageTest.kt` (which does
            // not — there is no CI emulator leg).
            settings.domStorageEnabled = true
            // COLOR SCHEME follows the OS via the app THEME, not any WebView call
            // here (task webview-follow-os-color-scheme, docs/adr/0009). The System
            // WebView always sets the page's `prefers-color-scheme` from the app
            // theme's `isLightTheme`, and `Theme.Werust` has a light variant
            // (res/values) + a night variant (res/values-night) selected by the OS
            // night-mode qualifier, so `prefers-color-scheme` matches the OS
            // light/dark setting. Do NOT force-dark here (e.g.
            // WebSettingsCompat.setForceDark / algorithmic darkening): that would
            // override the OS-follow and the page's own declared `color-scheme`.
            // A live OS light<->dark toggle recreates this Activity (no
            // `configChanges` for uiMode), so the fresh WebView re-reads the theme.
            // WEB INSPECTOR (task enable-web-inspector-devtools-all-platforms):
            // make the page inspectable via `chrome://inspect` (Chrome DevTools —
            // console REPL + network — the SAME devtools desktop shows in-window)
            // over USB from a desktop Chrome. `setWebContentsDebuggingEnabled` is a
            // process-wide static, GATED on a DEBUG build so a RELEASE build is NOT
            // silently inspectable — the Android analogue of the desktop
            // `enable-developer-extras` debug gate and iOS's `#if DEBUG`. The debug
            // signal is `ApplicationInfo.FLAG_DEBUGGABLE` (true for the debug APK
            // this module builds; false for a future release APK) rather than
            // `BuildConfig.DEBUG`, so no extra `buildConfig` generation is needed.
            // See
            // work/notes/observations/web-inspector-devtools-gating-decisions-2026-07-23.md.
            if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0) {
                WebView.setWebContentsDebuggingEnabled(true)
            }
            webViewClient = CoreWebViewClient()
            // Route NEW-WINDOW requests (a `target="_blank"` link / `window.open`)
            // INTO THIS SAME WebView instead of dropping them: werust has no
            // tab/window model yet (task
            // blank-and-window-open-links-navigate-in-place, field finding C,
            // docs/adr/0010). `setSupportMultipleWindows(true)` makes the System
            // WebView fire `WebChromeClient.onCreateWindow` for such a request
            // (rather than silently dropping it); [CoreWebChromeClient] handles it
            // by loading the target URL back into THIS WebView. Loading it through
            // the WebView's NORMAL path keeps trust intact: an `ipfs://` target
            // still reaches `shouldInterceptRequest` (hash-verified) and an
            // unsupported scheme is still refused — the hook is a router, not a
            // trust bypass. Mirrors the desktop `create`-signal and iOS
            // `createWebViewWith` hooks; the shared in-place rule is
            // `renderer::new_window_action`.
            settings.setSupportMultipleWindows(true)
            settings.javaScriptCanOpenWindowsAutomatically = true
            webChromeClient = CoreWebChromeClient()
            // Wire the EIP-1193 provider bridge: a JS interface the injected shim's
            // `postMessage` calls (page -> native), plus the document-start shim
            // itself (the SAME `werust-core` provider shim desktop injects). The
            // native side answers each envelope keylessly and the bridge evaluates
            // the response JS back in the page to settle its pending Promise.
            addJavascriptInterface(ProviderBridge(), PROVIDER_INTERFACE)
        }
        providerScript = buildProviderScript()

        // The trust indicator, at the footer next to the status: painted from the
        // core's posture (the ACTUAL load path), the SAME four states desktop shows.
        //
        // The initial text of both labels is the core's OWN derivation for the
        // starting chrome, never a Kotlin literal that happens to match it: a
        // hard-coded "⚠ unverified origin" here would be one more hand-written twin
        // of `trust_indicator`, which is precisely what task
        // `mobile-chrome-presentation-from-one-derivation` removed from this edge.
        // Every later repaint comes from [refreshChrome]. (`initialChrome` is the
        // ONE snapshot taken at the top of `onCreate`, so every first paint agrees
        // with every other.)
        trust = TextView(this).apply {
            text = initialChrome.trustIndicator
            gravity = Gravity.END
            // The badge's accessibility LABEL is the badge's own text: the short
            // STATE name, so TalkBack announces WHICH posture this is first and
            // always — the one fact a sighted user reads instantly.
            contentDescription = initialChrome.trustIndicator
            // The trust EXPLANATION on a platform with no hover: the core's
            // `trust_indicator_detail` rides in the SECONDARY accessibility slot
            // (`stateDescription`, below), so it FOLLOWS the state rather than
            // replacing it, and a TAP shows the same sentence in a dialog. Both
            // are refreshed in [refreshChrome] from the same one derivation; the
            // tap affordance is wired once here.
            isClickable = true
            isFocusable = true
            setOnClickListener { showTrustExplanation() }
            // Android's secondary announcement slot, read by TalkBack AFTER the
            // content description. It arrived in API 30, and this app's floor is
            // API 21, so on older devices the badge announces its STATE only and
            // the explanation stays one tap away (the dialog above) — the same
            // deal desktop gives a mouse user, whose explanation is a HOVER away.
            // Never the other way round: dropping the state was the regression
            // (task
            // `mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay`).
            accessibilityDelegate = object : View.AccessibilityDelegate() {
                override fun onInitializeAccessibilityNodeInfo(
                    host: View,
                    info: AccessibilityNodeInfo
                ) {
                    super.onInitializeAccessibilityNodeInfo(host, info)
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                        info.stateDescription = trustDetailText
                    }
                }
            }
        }
        trustDetailText = initialChrome.trustIndicatorDetail
        status = TextView(this).apply {
            text = initialChrome.statusLine
            gravity = Gravity.START
        }
        val footer = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            addView(status, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
            addView(trust, LinearLayout.LayoutParams(WRAP_CONTENT, WRAP_CONTENT))
        }

        // The PROMINENT in-view error banner: a high-contrast red bar directly
        // under the toolbar and ABOVE the WebView, shown ONLY on a failed load,
        // carrying the accurate protocol-named reason so the user cannot miss why
        // nothing rendered (the fail-closed honesty fix — the footer status was
        // "not easily seen"). Starts hidden. The SAME surfacing desktop shows.
        errorBanner = TextView(this).apply {
            setBackgroundColor(0xFFC01C28.toInt())
            setTextColor(0xFFFFFFFF.toInt())
            setPadding(dp(12), dp(10), dp(12), dp(10))
            visibility = View.GONE
        }

        // The LOAD-PROGRESS line: a thin determinate bar in werust's blue, directly
        // under the URL-bar row (where every mobile browser puts it). Its progress
        // advances with the real pipeline phase; when nothing is in flight it goes
        // INVISIBLE rather than GONE, so the 3dp strip it occupies is reserved for
        // good and no load state ever resizes the WebView (task
        // `loading-progress-in-the-url-bar-not-a-banner`). CANCEL is the toolbar's
        // ONE reload/stop control, in its STOP mode exactly while a load is in
        // flight; the phase NAME is in the footer status line.
        loadingProgress = ProgressBar(
            this, null, android.R.attr.progressBarStyleHorizontal
        ).apply {
            max = 100
            isIndeterminate = false
            progressTintList = ColorStateList.valueOf(0xFF1A5FB4.toInt())
            progressBackgroundTintList = ColorStateList.valueOf(0x00000000)
            visibility = View.INVISIBLE
        }

        browserChrome.addView(toolbar, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
        // The progress line sits directly under the toolbar at a FIXED height that
        // is never given back (INVISIBLE, not GONE, when idle), so it cannot
        // displace the page. The error banner sits under it and ABOVE the WebView:
        // a FAILURE is the only load state allowed to displace the page (there is
        // nothing rendered to displace, and the user must act).
        browserChrome.addView(loadingProgress, LinearLayout.LayoutParams(MATCH_PARENT, dp(3)))
        browserChrome.addView(errorBanner, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
        browserChrome.addView(webView)
        browserChrome.addView(footer, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))

        // The IN-APP DEBUG VIEW: a full-screen overlay ABOVE the browser chrome
        // (added last so it draws on top), hidden until the menu's Debug entry
        // opens it. The root is a FrameLayout so the overlay can cover the whole
        // screen (toolbar included) without disturbing the chrome's own layout.
        debugView = DebugView(this, core) { closeDebugView() }
        debugView.visibility = View.GONE
        val root = FrameLayout(this)
        root.addView(browserChrome, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        root.addView(debugView, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        setContentView(root)

        // Handle the SYSTEM/hardware Back button: while the core has back history
        // it navigates history (the SAME `driveCore { core.goBack() }` the
        // on-screen `◀` runs); with no history left the callback is disabled and
        // the platform default exits the Activity. Registered with this Activity
        // as the lifecycle owner so it is removed on destroy.
        onBackPressedDispatcher.addCallback(this, systemBackCallback)
        // Registered after the history callback so that, while the debug view is
        // open (and only then), system Back closes the view first.
        onBackPressedDispatcher.addCallback(this, debugBackCallback)

        // Launch a browsing surface: drive the core to the start URL OFF the UI
        // thread (a `.eth`/ENS start URL would otherwise block onCreate on the
        // blocking resolve), then let the core surface it onto the WebView on the
        // UI thread.
        driveCore { core.navigate(START_URL) }
    }

    /**
     * Run a blocking session-driving core `action` on the [coreExecutor]
     * background thread, then post [afterCoreAction] back to the UI thread to
     * apply any pending load to the `WebView` and repaint the chrome.
     *
     * This is the ANR fix's dispatch: the core action (which may resolve an
     * ENS/IPNS name with blocking network I/O) never runs on the UI thread, while
     * `WebView.loadUrl` + widget mutation — which MUST be on the UI thread — are
     * posted back. A guard against a destroyed Activity: if the executor was shut
     * down (in [onDestroy]) the submit is rejected and skipped, and the UI post is
     * a no-op once the Activity is finishing.
     */
    private fun driveCore(action: () -> Unit) {
        if (coreExecutor.isShutdown) return
        coreExecutor.execute {
            action()
            runOnUiThread {
                if (!isFinishing && !isDestroyed) afterCoreAction()
            }
        }
    }

    /**
     * A COMPACT nav button: a small fixed square that strips the default Android
     * button's large min-width and horizontal insets ([minWidth]/[minimumWidth] 0,
     * no horizontal padding), so the two that remain (the reload/stop control and
     * the ⋮ menu) take a small fixed slice of the row and the weighted URL bar
     * keeps the rest — which since the history buttons went is most of it. The
     * button glyph is small, but the enclosing toolbar row is [TOUCH_TARGET_DP]
     * tall so the effective touch target stays >= 48dp.
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

    /**
     * Show the GENERAL browser menu: a [PopupMenu] anchored on the ⋮ button,
     * built from the SHARED core's menu items (the werust version line + the
     * Debug entry).
     *
     * The core owns the item LIST, so this method only maps each item's `kind`
     * onto a platform affordance: an `info` item (the `werust <version>` line)
     * becomes a DISABLED entry (shown, not tappable), an `action` item an enabled
     * one dispatched by its STABLE id (never its label) in [onBrowserMenuItem].
     * Adding a future menu item therefore needs no change here at all unless it is
     * an action with new behaviour — the "structured to grow" property.
     */
    private fun showBrowserMenu() {
        val menu = core.menu()
        val popup = PopupMenu(this, menuButton)
        menu.items.forEachIndexed { index, item ->
            val entry = popup.menu.add(android.view.Menu.NONE, index, index, item.label)
            // A non-interactive line (the version) is shown but not tappable.
            entry.isEnabled = item.isAction()
        }
        popup.setOnMenuItemClickListener { clicked ->
            val item = menu.items.getOrNull(clicked.itemId)
            if (item == null) false else onBrowserMenuItem(item.id)
        }
        popup.show()
    }

    /**
     * Dispatch an activated browser-menu entry by its STABLE core id. Returns
     * whether the entry was handled (an unknown id is not, so a core item this
     * build does not know about fails visibly rather than silently doing nothing).
     */
    private fun onBrowserMenuItem(id: String): Boolean = when (id) {
        WerustCore.Menu.ITEM_DEBUG -> {
            openDebugView()
            true
        }
        else -> false
    }

    /**
     * The OPEN-DEBUG-VIEW hook the browser menu's Debug entry calls: opens the
     * full-screen tabbed Console + Network debug view over the core's shared
     * capture store ([WerustCore.debugJson]). The menu task
     * (`general-browser-menu-with-version-and-debug-entry`) left this hook an
     * honest "not built yet" placeholder; THIS is the real view that fills it
     * (task `debug-view-console-network-tabs-mobile`). System Back while it is
     * open closes it ([debugBackCallback]); the ✕ affordance is the view's own.
     */
    private fun openDebugView() {
        debugView.open()
        debugBackCallback.isEnabled = true
    }

    /** Close the debug view (the ✕ affordance and the system Back button). */
    private fun closeDebugView() {
        debugView.close()
        debugBackCallback.isEnabled = false
    }

    /**
     * Show what the current trust posture MEANS: the core's
     * `trust_indicator_detail`, titled with the badge it explains.
     *
     * This is the trust EXPLANATION on a platform with no hover. Desktop shows it
     * as the badge's tooltip; for months neither mobile edge showed it AT ALL,
     * because each had hand-written its own `trustIndicator()` twin and simply
     * never wrote a `trustIndicatorDetail()` to go with it (`docs/adr/0011`,
     * `docs/adr/0006`: for a browser whose thesis is an honest, legible trust
     * posture, a badge with no explanation on the two platforms most users are on
     * is a real gap). Reading the sentence from the chrome means Android now shows
     * exactly what desktop shows, and a future rewording lands on both at once.
     *
     * TWO affordances, both from the same one field: this TAP (an explicit user
     * gesture, no hover needed) and the badge's accessibility STATE description
     * set in [refreshChrome] (TalkBack reads the meaning after the state, never
     * instead of it). The framework `AlertDialog` keeps the edge framework-only
     * (the single androidx dependency stays the back-dispatcher one), and the
     * dismiss button is the PLATFORM's localized OK rather than a string minted
     * here.
     *
     * It reads the LAST PAINTED snapshot rather than calling the core again: a
     * chrome read takes the native session lock, which on the UI thread can wait
     * behind an in-flight `ipfs://` retrieval (the ANR guard,
     * `work/notes/observations/mobile-chrome-reads-still-take-the-session-lock-2026-07-29.md`).
     * The badge and [trustDetailText] were both set by the last [refreshChrome]
     * from ONE chrome snapshot, so they cannot disagree with each other.
     */
    private fun showTrustExplanation() {
        val detail = trustDetailText
        if (detail.isEmpty()) return
        // The TRUST-ON-FIRST-USE section of the same surface (task
        // `ipns-tofu-pin-and-warn-on-change`): what this MUTABLE name resolves to
        // now, what the user blessed for it, and (when there is something new to
        // record) the bless action. The bless is reached FROM the trust indicator
        // by an explicit tap, never a first-visit prompt, which is why it lives
        // here rather than in a dialog of its own. Every string is the core's, read
        // off the last painted snapshot (never a fresh chrome read, which would
        // take the native session lock on the UI thread, the ANR guard).
        val message =
            if (trustPinDetailText.isEmpty()) detail else "$detail\n\n$trustPinDetailText"
        val builder = AlertDialog.Builder(this)
            .setTitle(trust.text)
            .setMessage(message)
            .setPositiveButton(android.R.string.ok, null)
        if (trustPinActionOffered) {
            // Blessing writes the pin file, so it goes through the SAME
            // off-UI-thread dispatch every session-driving action uses.
            builder.setNeutralButton(trustPinActionLabelText) { _, _ ->
                driveCore { core.blessName() }
            }
        }
        builder.show()
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
        // The INVALID-URL surface (field finding D): when the last entry was
        // invalid (a scheme-less garbage entry that did not navigate) show the small
        // badge and render the URL-bar text as invalid (red underline), keeping the
        // typed text for the user to fix. Toggled from the orthogonal `invalidEntry`
        // fact — distinct from the trust indicator and the load-error banner. The
        // SAME rule desktop applies, from the same chrome fact.
        if (chrome.invalidEntryBadgeVisible) {
            invalidBadge.text = chrome.invalidEntryBadgeText
            invalidBadge.visibility = View.VISIBLE
            urlBar.setTextColor(0xFFC01C28.toInt())
            urlBar.paintFlags = urlBar.paintFlags or Paint.UNDERLINE_TEXT_FLAG
        } else {
            invalidBadge.visibility = View.GONE
            urlBar.setTextColor(defaultUrlBarColor)
            urlBar.paintFlags = urlBar.paintFlags and Paint.UNDERLINE_TEXT_FLAG.inv()
        }
        // The SYSTEM Back button, which since the on-screen ◀ went is the ONLY
        // back affordance Android has: enabled exactly on the core's `canGoBack`.
        // Disabled = the platform default runs and Back EXITS (no history left to
        // walk). The history CAPABILITY behind it is untouched by the button
        // removal — this line is what turns it into an affordance.
        systemBackCallback.isEnabled = chrome.canGoBack
        // The ONE reload/stop control: the glyph it wears and what it announces
        // are the core's derived mode, carried on the chrome
        // (`reload_stop_control(state).label()` / `.description()`). There is no
        // `when` here and there must never be one: which mode this is was decided
        // ONCE, in the toolkit-free core, and the same decision drives what the
        // click does (`activateReloadStopControl`). Android has no hover, so the
        // words go in the platform's accessible-name slot rather than a tooltip.
        reloadStopButton.text = chrome.reloadStopControlLabel
        reloadStopButton.contentDescription = chrome.reloadStopControlDescription
        // The SPINNER: a SECOND presentation of the load the progress line already
        // reports (the core keeps both on one rule, so they cannot contradict each
        // other), covering what the bar cannot — a load that has reported no phase
        // yet, and the pre-content name-resolution window. INVISIBLE rather than
        // GONE keeps its slot, so no load state ever moves the URL bar.
        loadingSpinner.visibility =
            if (chrome.loadSpinnerVisible) View.VISIBLE else View.INVISIBLE
        status.text = chrome.statusLine
        // The trust indicator tracks the core's posture (the real load path),
        // matching desktop; the seam-default no-op is gone. The badge's
        // accessibility LABEL is the badge's own STATE name, so TalkBack announces
        // which posture this is FIRST and always; its EXPLANATION (the core's
        // `trust_indicator_detail`, which used to reach desktop only) follows in
        // the secondary slot (`stateDescription`, wired in `onCreate`) and in the
        // tap dialog: the platform-appropriate stand-in for desktop's hover
        // tooltip. Both strings are the core's, off this ONE snapshot — the edge
        // neither re-derives nor re-words either.
        trust.text = chrome.trustIndicator
        trust.contentDescription = chrome.trustIndicator
        trustDetailText = chrome.trustIndicatorDetail
        // The trust surface's TRUST-ON-FIRST-USE section, cached off this ONE
        // painted snapshot so the tap handler never takes the native session lock
        // on the UI thread (the ANR guard). Every string is the core's derivation:
        // this edge decides neither the wording nor whether the action is offered.
        trustPinDetailText = chrome.trustPinDetail
        trustPinActionOffered = chrome.trustPinActionVisible
        trustPinActionLabelText = chrome.trustPinActionLabel
        // The LOAD-PROGRESS line: its progress advances with the real pipeline phase
        // while a load is in flight (including the pre-content name-resolution
        // window, where the backend has not started yet), and it goes INVISIBLE —
        // never GONE — once the load settles, so the strip it occupies is never
        // given back and a navigation cannot resize the page (task
        // `loading-progress-in-the-url-bar-not-a-banner`). CANCEL is the toolbar's
        // ONE reload/stop control, in its STOP mode exactly while a load is in
        // flight; the phase NAME is in
        // the footer status line (and this line's content description). Driven by
        // this existing refresh, so no new timer / poll / tight loop (the Android
        // ANR guard is not regressed).
        //
        // The core hands over its progress FRACTION (0.0-1.0, the one shared unit);
        // scaling it onto this widget's own 0-100 range is the only arithmetic left
        // here, and it reads the bar's own `max` rather than restating 100.
        loadingProgress.progress = (chrome.loadProgressFraction * loadingProgress.max).roundToInt()
        loadingProgress.contentDescription = chrome.loadProgressHint
        loadingProgress.visibility =
            if (chrome.loadProgressVisible) View.VISIBLE else View.INVISIBLE
        // The PROMINENT error banner: shown ONLY on a failed load, carrying the
        // accurate protocol-named reason across the top of the view so the user
        // cannot miss why nothing rendered (the fail-closed honesty fix). Hidden
        // otherwise. The SAME rule desktop applies, from the same chrome fact.
        if (chrome.errorBannerVisible) {
            errorBanner.text = chrome.errorBannerText
            // A TRANSIENT/timeout failure (retryable) is a softer amber banner; a
            // hard failure is the prominent red one (task
            // `clearer-loading-and-error-indicator`). The SAME distinction desktop
            // shows, from the core's `retryable` fact.
            errorBanner.setBackgroundColor(
                if (chrome.retryable) 0xFFB5820A.toInt() else 0xFFC01C28.toInt()
            )
            errorBanner.visibility = View.VISIBLE
        } else {
            errorBanner.visibility = View.GONE
        }
        // The open IN-APP DEBUG VIEW refreshes on this SAME existing
        // chrome-refresh point: the mobile cadence is event-driven (after each
        // core action / page lifecycle signal), so the view tracks the store with
        // NO new timer and NO busy poll; the ANR fix is respected, and the FFI
        // debug document reads OFF the native session lock, so this refresh can
        // never block the UI thread behind an in-flight `ipfs://` retrieval.
        if (::debugView.isInitialized && debugView.isOpen()) debugView.refresh()
    }

    /**
     * Build the document-start provider script: a small preamble that defines
     * `window.webkit.messageHandlers.<channel>.postMessage` (the channel the
     * shared provider shim posts to) so it forwards to the `@JavascriptInterface`
     * [ProviderBridge] and evaluates the synchronous native response, followed by
     * the `werust-core` provider shim itself. Injected at document start (see
     * [CoreWebViewClient.onPageStarted]) so a page's `window.ethereum` is the
     * injected native provider from the first script. Empty if the core has no
     * shim to inject.
     */
    private fun buildProviderScript(): String {
        val shim = core.documentStartScript()
        if (shim.isEmpty()) return ""
        // The `window.webkit.messageHandlers.<channel>` shape the shared shim posts
        // to, bridged to the Android JS interface. The native resolve is
        // synchronous, so the bridge evals the returned response JS inline to
        // settle the page's pending Promise.
        val preamble = """
            (function () {
              window.webkit = window.webkit || {};
              window.webkit.messageHandlers = window.webkit.messageHandlers || {};
              window.webkit.messageHandlers.$PROVIDER_CHANNEL = {
                postMessage: function (m) {
                  var out = $PROVIDER_INTERFACE.postMessage(String(m));
                  if (out) { try { window.eval(out); } catch (e) {} }
                }
              };
            })();
        """.trimIndent()
        return "$preamble\n$shim"
    }

    /**
     * The `@JavascriptInterface` the provider bridge preamble calls: it hands a
     * page-posted EIP-1193 envelope to the Rust core and returns the response JS
     * the page evaluates to settle its pending Promise. Runs on a WebView
     * JS-interface thread; the native `SyncSession` mutex serializes it against
     * the UI thread exactly as the `ipfs://` interception is.
     */
    private inner class ProviderBridge {
        @JavascriptInterface
        fun postMessage(body: String): String =
            core.handleProviderMessage(PROVIDER_CHANNEL, body)
    }

    /**
     * Routes NEW-WINDOW requests (a `target="_blank"` link / `window.open(url)`)
     * into the SAME [webView], since werust has no tab/window model yet (task
     * `blank-and-window-open-links-navigate-in-place`, field finding C,
     * `docs/adr/0010`). The recorded decision is to navigate IN-PLACE until tabs
     * exist, so a `_blank` link behaves like an ordinary in-view navigation
     * instead of doing nothing.
     *
     * MECHANISM: with `setSupportMultipleWindows(true)` the System WebView fires
     * [onCreateWindow] with a transport [Message] instead of dropping the request.
     * The System WebView does NOT expose the requested URL directly here; the
     * idiomatic way to recover it is a throwaway transport `WebView` whose
     * [WebViewClient] receives the target URL as its first navigation. We hand
     * that URL to the MAIN WebView's normal [WebView.loadUrl] (so an `ipfs://`
     * target still routes through [CoreWebViewClient.shouldInterceptRequest] and
     * an unsupported scheme is still refused — a router, not a trust bypass) and
     * create NO real second window.
     *
     * Manual verification steps:
     * docs/spikes/blank-and-window-open-links-navigate-in-place/README.md.
     */
    private inner class CoreWebChromeClient : WebChromeClient() {
        /**
         * The ANDROID CONSOLE CAPTURE POINT: every `console.*` the page logs, into
         * the shared core store the in-app debug menu's Console tab renders (task
         * `debug-console-network-capture-per-platform`).
         *
         * Android is the ONE platform that uses its REAL native console callback
         * rather than an injected shim (desktop and iOS must inject one, since
         * neither WebKitGTK 6 nor WKWebView exposes a console callback). This hook
         * hands over the message, the level, the source id and the line number
         * directly, sees engine-emitted console output a page-side `console.*`
         * wrapper never could, and cannot be un-wrapped by the page. The recorded
         * decision is at
         * docs/spikes/debug-console-network-capture-per-platform/DECISIONS.md.
         *
         * READ-ONLY observation: returning `false` lets the WebView keep its
         * default handling (the message still goes to logcat and to a tethered
         * chrome://inspect console), so capture ADDS a surface rather than
         * replacing one.
         *
         * THREADING: this runs on the UI thread, so the native push deliberately
         * does NOT go through the session lock (`SyncSession::debug_capture`) —
         * `resolve_ipfs` can hold that lock for seconds on a worker thread during a
         * CAR retrieval, and waiting on it here would be exactly the ANR the
         * off-main-thread work fixed. The push itself is a bounded ring-buffer
         * insert.
         */
        override fun onConsoleMessage(message: ConsoleMessage): Boolean {
            core.captureConsole(
                message.messageLevel().name,
                message.message() ?: "",
                message.sourceId() ?: "",
                message.lineNumber(),
            )
            // The open debug view refreshes from this SAME event (this callback
            // is already on the UI thread, and the refresh reads the FFI debug
            // document off the session lock), so the Console tab tracks new log
            // entries with no timer or poll.
            if (::debugView.isInitialized && debugView.isOpen()) debugView.refresh()
            // Do NOT claim to have handled it: the platform's own console handling
            // (logcat, the remote inspector) stays exactly as it was.
            return false
        }

        override fun onCreateWindow(
            view: WebView,
            isDialog: Boolean,
            isUserGesture: Boolean,
            resultMsg: Message,
        ): Boolean {
            // A throwaway transport WebView: its only job is to catch the target
            // URL of the new-window request (delivered as its first navigation),
            // then load it into the MAIN WebView in place and discard itself.
            val transport = WebView(this@BrowserActivity)
            transport.webViewClient = object : WebViewClient() {
                override fun shouldOverrideUrlLoading(
                    v: WebView,
                    request: WebResourceRequest,
                ): Boolean {
                    // Load the `_blank`/`window.open` target IN THE CURRENT view
                    // through the normal load path (verification preserved), then
                    // tear down the throwaway transport WebView. The URL is mapped
                    // through the core's toWebViewUrl first: an `ipfs://` target
                    // must load on the internal https origin like every other
                    // load — a direct `ipfs://` main-frame load would land the
                    // page on the opaque origin where SvelteKit client nav dies
                    // (task mobile-ronan-eth-buttons-no-navigation).
                    webView.loadUrl(core.toWebViewUrl(request.url.toString()))
                    v.stopLoading()
                    v.destroy()
                    return true
                }
            }
            (resultMsg.obj as WebView.WebViewTransport).webView = transport
            resultMsg.sendToTarget()
            // Return true: we HANDLED the new-window request (routed it in place);
            // no real second window/tab was created.
            return true
        }
    }

    override fun onDestroy() {
        // Stop accepting new background actions and let the native session close.
        // `shutdown` (not `shutdownNow`) lets any in-flight action finish so the
        // native session is not freed out from under a running core call.
        coreExecutor.shutdown()
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
         * INTERCEPTION MECHANISM (Android): `shouldInterceptRequest` answering
         * from the core, with the page served on the INTERNAL `https://<cid>
         * .ipfs.werust.invalid` origin — the fallback recorded in
         * work/notes/observations/mobile-ipfs-interception-mechanism-2026-07-23.md,
         * promoted to THE mechanism by task
         * `mobile-ronan-eth-buttons-no-navigation`: an `ipfs://` document served
         * through this hook gets an OPAQUE origin in the System WebView (Blink
         * refuses `fetch(ipfs://…)` and dynamic `import()` before the network
         * stack and `localStorage` is null), which killed every SvelteKit
         * client-side navigation. The WebView therefore loads the internal
         * `https://` origin (a normal fetchable, `pushState`-able secure
         * context); every URL is translated between that origin and the core's
         * real `ipfs://` URLs by the Rust edge (`origin_map.rs`), so this hook
         * still receives real `ipfs://` requests and the core's history/URL bar
         * never see the internal origin. Diagnosis + on-device evidence:
         * docs/spikes/mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md.
         *
         * THREADING: this hook runs on a WebView WORKER thread, NOT the UI thread
         * that drives `navigate` / `onPageStarted` / `onPageFinished`. Both touch
         * the SAME native session, so the Rust edge wraps it in a `SyncSession`
         * (a `Mutex` around the single-threaded `CoreSession`): every WORKER-thread
         * native call — including this [WerustCore.resolveIpfs] — locks first, so
         * the two threads are serialized. The UI thread's page-signal callbacks
         * ([WerustCore.onPageCommitted] / [WerustCore.onPageFinished] /
         * [WerustCore.onPageFailed] / [WerustCore.onUrlChanged]) are the one
         * exception: they record through the backend's thread-safe clone handle
         * OFF the session lock, so a multi-second retrieval holding the lock here
         * can never freeze the UI thread (the ANR guard, task
         * `mobile-page-signal-callbacks-off-session-lock`). That is what makes
         * calling into the core from this off-UI-thread hook sound.
         */
        override fun shouldInterceptRequest(
            view: WebView,
            request: WebResourceRequest,
        ): WebResourceResponse? {
            val url = request.url.toString()
            val method = request.method ?: "GET"
            val mainFrame = request.isForMainFrame
            return when (val resolution = core.resolveIpfs(url)) {
                null -> {
                    // THE PASSED-THROUGH BRANCH — and the reason Android has the
                    // widest network reach of the three platforms: this hook sees
                    // EVERY request the WebView makes, including the `https://`
                    // ones werust does not intercept at all. Record it before
                    // returning null so the Network tab is the whole request
                    // stream, not just the content-addressed slice. It is honestly
                    // UNVERIFIED (werust did not fetch or verify these bytes; the
                    // WebView did), and its status/mime are unknown here because
                    // the response never passes through us — recorded as 0/"", which
                    // the core keeps as an honest "unknown", never a fake 200.
                    core.captureNetwork(method, url, 0, "", 0L, false, mainFrame)
                    null // not an intercepted scheme: let the WebView handle it
                }
                is WerustCore.Resolution.Ok -> {
                    // THE INTERCEPTED BRANCH: werust resolved these bytes itself, so
                    // the real status + MIME are known here. `verified` is true ONLY
                    // for a successful `ipfs://` resolution — the core decides what
                    // that earns (an `ipfs://` resolution that hash-verified is
                    // content-verified; a `werust://settings` page is not), so the
                    // trust posture tracks the ACTUAL load path and never the URL
                    // string (ADR-0006).
                    core.captureNetwork(
                        method,
                        url,
                        resolution.status,
                        resolution.mimeType,
                        resolution.body.size.toLong(),
                        true,
                        mainFrame,
                    )
                    if (resolution.status == 200) {
                        WebResourceResponse(
                            resolution.mimeType,
                            "utf-8",
                            ByteArrayInputStream(resolution.body),
                        )
                    } else {
                        // A NON-OK status WITH a body: the site's own error page,
                        // named by its `_redirects` (IPIP-0002) for a path that is
                        // not in its DAG. The bytes are the same hash-verified
                        // bytes; only the reported status differs, so the page
                        // renders as the not-found it honestly is (what a gateway
                        // does) instead of werust claiming 200 for a page the site
                        // declared missing.
                        WebResourceResponse(
                            resolution.mimeType,
                            "utf-8",
                            resolution.status,
                            statusReasonPhrase(resolution.status),
                            emptyMap<String, String>(),
                            ByteArrayInputStream(resolution.body),
                        )
                    }
                }
                is WerustCore.Resolution.Error -> {
                    // A FAILED resolution proved nothing, so it is recorded honestly
                    // UNVERIFIED with the fail-closed 502 the WebView is about to
                    // see. Capturing the failure is the point: the Network tab is
                    // where a user diagnoses why a page did not render.
                    core.captureNetwork(method, url, 502, "text/plain", 0L, false, mainFrame)
                    // Fail closed: an HTTP error status with no body, so unverified
                    // bytes never render and the failure is honest. The reason is
                    // carried in the reason phrase; the `WebView` surfaces the
                    // failed load via `onReceivedHttpError` on the UI thread. The
                    // native session is serialized by its `SyncSession` mutex (see
                    // the KDoc above), so calling into the core from this off-UI
                    // worker thread is safe.
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
        }

        /**
         * The HTTP reason phrase for a status a site's `_redirects` may ask for
         * (IPIP-0002 §2.3). `WebResourceResponse` rejects a blank phrase, so every
         * supported status has one and anything else falls back to a generic one.
         */
        private fun statusReasonPhrase(status: Int): String = when (status) {
            404 -> "Not Found"
            410 -> "Gone"
            451 -> "Unavailable For Legal Reasons"
            else -> "Status $status"
        }

        override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
            // Install the EIP-1193 provider shim as the FIRST script the document
            // runs, so `window.ethereum` is the injected native provider before any
            // page script. (WebView has no exact document-start user-script hook
            // without androidx; `onPageStarted` is the earliest edge hook.)
            if (providerScript.isNotEmpty()) view.evaluateJavascript(providerScript, null)
            core.onPageCommitted(url)
            refreshChrome()
        }

        override fun onPageFinished(view: WebView, url: String) {
            core.onPageFinished(url)
            // `afterCoreAction` (not a bare `refreshChrome`) because driving the
            // core here may produce a PENDING LOAD the WebView must perform: a
            // site's `_redirects` 3xx rule (IPIP-0002) is a NAVIGATION the
            // intercepted request cannot answer (`WebResourceResponse` refuses a
            // 3xx status outright), so the core queues the `ipfs://<rootcid><to>`
            // target and its pump turns it into an ordinary pending load. Draining
            // it here is what makes the redirect real — bar + history move, and the
            // target hash-verified by the fresh `shouldInterceptRequest` it
            // triggers (task `ipfs-redirects-3xx-navigation-support`).
            afterCoreAction()
        }

        override fun doUpdateVisitedHistory(view: WebView, url: String, isReload: Boolean) {
            // A SvelteKit SPA link click is a CLIENT-SIDE `pushState`/`replaceState`
            // navigation: the document does NOT reload, so `onPageStarted`/
            // `onPageFinished` never fire and the URL bar used to freeze on the
            // pinned `.eth` name. `doUpdateVisitedHistory` DOES fire on such
            // same-document history changes, so report the new URL as a
            // same-document change (NOT a load): the core follows it (dropping the
            // pin / re-deriving the ENS name) without faking a load lifecycle.
            // Task `track-webview-url-on-spa-clientside-navigation`.
            core.onUrlChanged(url)
            afterCoreAction()
        }

        override fun onReceivedError(
            view: WebView,
            request: WebResourceRequest,
            error: WebResourceError,
        ) {
            if (request.isForMainFrame) {
                core.onPageFailed(request.url.toString(), error.description.toString())
                // A `_redirects` 3xx answers the intercepted request fail-closed (no
                // page renders under the OLD url), so THIS is the signal that
                // follows it: drain the pending load the core's pump queued and
                // perform the redirect.
                afterCoreAction()
            }
        }
    }

    companion object {
        /** The URL the app opens on launch, so it shows a browsing surface. */
        private const val START_URL = "https://example.com/"

        /** The EIP-1193 provider script-message channel (matches `werust-core`). */
        private const val PROVIDER_CHANNEL = "werustProvider"

        /** The `@JavascriptInterface` name the provider bridge preamble calls. */
        private const val PROVIDER_INTERFACE = "werustProviderBridge"

        /** The compact nav-button square edge, in dp (small so the URL bar wins width). */
        private const val NAV_BUTTON_DP = 40

        /**
         * The loading spinner's square edge, in dp: smaller than a button, since
         * it is a status indicator rather than a touch target, and a permanently
         * reserved slot should cost the URL bar as little width as it can.
         */
        private const val SPINNER_DP = 20

        /** The toolbar row's minimum height, in dp, keeping touch targets >= 48dp. */
        private const val TOUCH_TARGET_DP = 48
    }
}
