package com.github.wighawag.werust

import android.content.Context
import android.graphics.Typeface
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.BaseAdapter
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.TextView
import java.util.Locale
import org.json.JSONObject

/**
 * The Android IN-APP DEBUG VIEW: a full-screen tabbed screen (Console +
 * Network) opened from the browser menu's Debug entry, rendering the ONE
 * shared capture store over the FFI ([WerustCore.debugJson]) (task
 * `debug-view-console-network-tabs-mobile`, spec
 * `in-app-debug-menu-console-and-network`).
 *
 * This is the no-tether debug surface: a phone user with no desktop opens the
 * ⋮ menu -> Debug and sees the page's console log and network requests IN-APP.
 * The native remote inspector (chrome://inspect over USB) stays as the deep
 * devtools; this is the standalone console+network subset, and it is READ-ONLY
 * by construction (every row is a [TextView]; no `EditText` exists here; a
 * typeable REPL is the remote inspector's job, spec Out of Scope).
 *
 * The recorded decisions this bakes in live in
 * `docs/spikes/debug-view-console-network-tabs-mobile/DECISIONS.md`; the short
 * form:
 *
 * * WHY AN OVERLAY VIEW, NOT A SEPARATE ACTIVITY: the store lives behind the
 *   [BrowserActivity]'s ONE [WerustCore] session, which cannot cross an
 *   Activity boundary; an overlay needs no manifest entry, and the SYSTEM Back
 *   button closes it through the same `OnBackPressedDispatcher` the rest of
 *   the shell uses. The Activity lays this view out MATCH_PARENT over the
 *   whole browser chrome (full-screen), hidden until the Debug entry opens it.
 * * TABS AS TWO TOGGLED LISTS: the task allowed "a `TabLayout` + a pager, or
 *   two toggled lists". Two toggle buttons switching ONE [ListView] needs no
 *   new dependency in a deliberately framework-only edge (the only androidx
 *   dependency is `activity`, for the Back dispatcher), and the iOS twin does
 *   the same with a `UISegmentedControl` over one table.
 * * REFRESH IS EVENT-DRIVEN, ON THE EXISTING CADENCE: the Activity calls
 *   [refresh] from its own `refreshChrome` (the existing chrome-refresh point)
 *   and from the console capture callback (already on the UI thread). NO new
 *   timer, NO self-rescheduling handler loop, NO busy poll: the Android ANR fix
 *   (`android-anr-main-thread-diagnose-and-unblock`) is respected, and the
 *   FFI debug document reads OFF the native session lock precisely so this
 *   refresh can never block the UI thread behind an in-flight `ipfs://`
 *   retrieval. Each refresh re-renders from the whole snapshot: the store is
 *   bounded (300 entries x 2000 chars) and the cadence is per page event, not
 *   per frame, so the incremental sequence-anchor the DESKTOP view needs on
 *   its 50ms pump is not needed here (the FFI document carries no sequence).
 * * THE NETWORK TAB SPEAKS THE TRUST INDICATOR'S EXACT VOCABULARY (ADR-0006):
 *   each row's trust is the indicator's glyph for the posture plus the core's
 *   wire name the debug JSON already carries (`content-verified`,
 *   `unverified-origin`, `name-via-trusted-rpc`, `mutable-name`), in the same
 *   hues the desktop stylesheet gives the `trust-*` classes, never a new
 *   label minted for the debug view.
 */
class DebugView(
    context: Context,
    private val core: WerustCore,
    private val onClose: () -> Unit,
) : LinearLayout(context) {

    /** Which tab is showing: the CONSOLE log or the NETWORK requests. */
    private enum class Tab { CONSOLE, NETWORK }

    /**
     * One rendered row: the main line, an optional detail line (the network
     * URL, unbounded, on its own line so a phone-width screen keeps the
     * columns legible), its colour ([COLOR_DEFAULT] = the theme default), and
     * whether the main line is bold (console errors/warnings).
     */
    private data class Row(val text: String, val detail: String?, val color: Int, val bold: Boolean)

    private var tab = Tab.CONSOLE
    private val adapter = RowAdapter()
    private val listView: ListView
    private val consoleTab: Button
    private val networkTab: Button
    private val emptyLabel: TextView

    init {
        orientation = VERTICAL
        // Opaque: the view covers the browser chrome, so it must paint the
        // theme's own window background (never a hard-coded colour that would
        // fight the OS light/dark theme, per docs/adr/0009).
        val background = TypedValue()
        if (context.theme.resolveAttribute(android.R.attr.windowBackground, background, true)) {
            if (background.resourceId != 0) {
                setBackgroundResource(background.resourceId)
            } else {
                setBackgroundColor(background.data)
            }
        }

        // The header row: the title, the CLEAR action (empties BOTH buffers of
        // the shared store over the FFI), and the close affordance (the twin
        // of the system Back button, which the Activity routes here too).
        val title = TextView(context).apply {
            text = "Console + Network capture"
            textSize = 15f
            setTypeface(typeface, Typeface.BOLD)
            layoutParams = LayoutParams(0, WRAP_CONTENT, 1f)
        }
        val clearButton = Button(context).apply {
            text = "Clear"
            setOnClickListener {
                core.debugClear()
                refresh()
            }
        }
        val closeButton = Button(context).apply {
            text = "✕"
            setOnClickListener { onClose() }
        }
        addView(
            LinearLayout(context).apply {
                orientation = HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                addView(title)
                addView(clearButton)
                addView(closeButton)
            },
            LayoutParams(MATCH_PARENT, WRAP_CONTENT),
        )

        // The tab strip: two toggle buttons switching the ONE list between the
        // Console and Network tabs (the "two toggled lists" the task allows).
        consoleTab = tabButton("Console", Tab.CONSOLE)
        networkTab = tabButton("Network", Tab.NETWORK)
        addView(
            LinearLayout(context).apply {
                orientation = HORIZONTAL
                addView(consoleTab, LayoutParams(0, WRAP_CONTENT, 1f))
                addView(networkTab, LayoutParams(0, WRAP_CONTENT, 1f))
            },
            LayoutParams(MATCH_PARENT, WRAP_CONTENT),
        )

        emptyLabel = TextView(context).apply {
            text = "Nothing captured yet"
            gravity = Gravity.CENTER
        }
        listView = ListView(context).apply {
            setAdapter(this@DebugView.adapter)
            emptyView = emptyLabel
        }
        // The list plus its empty-state label, stacked so the label is centred
        // over the list area (the ListView toggles the label's visibility).
        addView(
            FrameLayout(context).apply {
                addView(listView, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
                addView(emptyLabel, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
            },
            LayoutParams(MATCH_PARENT, 0, 1f),
        )
    }

    /** Whether the debug view is currently showing. */
    fun isOpen(): Boolean = visibility == VISIBLE

    /**
     * Show the view and paint the store captured so far, landed at the BOTTOM
     * (the newest entries), the devtools-console idiom.
     */
    fun open() {
        visibility = VISIBLE
        refresh()
        post { listView.setSelection((adapter.count - 1).coerceAtLeast(0)) }
    }

    /** Hide the view (the close affordance and the system Back button). */
    fun close() {
        visibility = GONE
    }

    /**
     * Catch the view up with the store: re-read the FFI debug document and
     * re-render the active tab. Called by the [BrowserActivity] on the
     * EXISTING chrome-refresh points and from the console capture event:
     * event-driven, never a poll. Newest stays at the bottom; the scroll
     * sticks to the bottom only when the user is already there (a user
     * scrolled up reading an earlier entry is never yanked back down).
     */
    fun refresh() {
        val wasAtBottom = isAtBottom()
        val debug = core.debugJson()
        adapter.rows = when (tab) {
            Tab.CONSOLE -> consoleRows(debug)
            Tab.NETWORK -> networkRows(debug)
        }
        adapter.notifyDataSetChanged()
        restyleTabs()
        if (wasAtBottom) {
            // Post so the new rows are laid out before the scroll lands.
            post { listView.setSelection((adapter.count - 1).coerceAtLeast(0)) }
        }
    }

    /** Whether the list is showing (or is one row short of) its newest row. */
    private fun isAtBottom(): Boolean {
        val last = adapter.count - 1
        return last < 0 || listView.lastVisiblePosition >= last - 1
    }

    /** One tab toggle button; activating it switches the list and repaints. */
    private fun tabButton(label: String, target: Tab): Button =
        Button(context).apply {
            text = label
            setOnClickListener {
                tab = target
                refresh()
                // A tab switch starts at the BOTTOM (the newest entries).
                post { listView.setSelection((adapter.count - 1).coerceAtLeast(0)) }
            }
        }

    /** Emphasise the active tab (bold) so the toggle state reads at a glance. */
    private fun restyleTabs() {
        consoleTab.setTypeface(
            consoleTab.typeface,
            if (tab == Tab.CONSOLE) Typeface.BOLD else Typeface.NORMAL,
        )
        networkTab.setTypeface(
            networkTab.typeface,
            if (tab == Tab.NETWORK) Typeface.BOLD else Typeface.NORMAL,
        )
    }

    /** The CONSOLE rows of one FFI debug document, oldest first. */
    private fun consoleRows(json: String): List<Row> {
        val entries = JSONObject(json).optJSONArray("console") ?: return emptyList()
        val rows = ArrayList<Row>(entries.length())
        for (i in 0 until entries.length()) {
            val entry = entries.getJSONObject(i)
            val level = entry.optString("level", "log")
            rows.add(
                Row(
                    text = consoleRowText(
                        level = level,
                        message = entry.optString("message", ""),
                        source = entry.optString("source", ""),
                        // An unknown line is JSON `null`, never a fabricated 0.
                        line = if (entry.isNull("line")) null else entry.getInt("line"),
                    ),
                    detail = null,
                    color = consoleLevelColor(level),
                    bold = level == "error" || level == "warn",
                ),
            )
        }
        return rows
    }

    /** The NETWORK rows of one FFI debug document, oldest first. */
    private fun networkRows(json: String): List<Row> {
        val entries = JSONObject(json).optJSONArray("network") ?: return emptyList()
        val rows = ArrayList<Row>(entries.length())
        for (i in 0 until entries.length()) {
            val entry = entries.getJSONObject(i)
            // The debug JSON carries the posture as the core's wire name; the
            // label reuses the trust indicator's glyph for it (ADR-0006).
            val trust = entry.optString("trust", "unverified-origin")
            rows.add(
                Row(
                    text = networkSummaryText(
                        method = entry.optString("method", "GET"),
                        // Unknown status/size is JSON `null`, never a fake 0.
                        status = if (entry.isNull("status")) null else entry.getInt("status"),
                        mime = entry.optString("mime", ""),
                        size = if (entry.isNull("size")) null else entry.getLong("size"),
                        trust = trust,
                    ),
                    detail = entry.optString("url", ""),
                    color = trustColor(trust),
                    bold = false,
                ),
            )
        }
        return rows
    }

    /** The row list: one [TextView] (plus a detail line for the network URL). */
    private inner class RowAdapter : BaseAdapter() {
        var rows: List<Row> = emptyList()

        override fun getCount(): Int = rows.size
        override fun getItem(position: Int): Row = rows[position]
        override fun getItemId(position: Int): Long = position.toLong()

        override fun getView(position: Int, convertView: View?, parent: ViewGroup): View {
            val row = rows[position]
            val cell = LinearLayout(parent.context).apply {
                orientation = VERTICAL
                setPadding(dp(12), dp(6), dp(12), dp(6))
            }
            cell.addView(
                TextView(parent.context).apply {
                    text = row.text
                    textSize = 13f
                    if (row.color != COLOR_DEFAULT) setTextColor(row.color)
                    if (row.bold) setTypeface(typeface, Typeface.BOLD)
                },
            )
            if (row.detail != null) {
                cell.addView(
                    TextView(parent.context).apply {
                        text = row.detail
                        textSize = 12f
                    },
                )
            }
            return cell
        }
    }

    /** Density-independent pixels -> device pixels (mirrors the Activity's). */
    private fun dp(value: Int): Int =
        TypedValue.applyDimension(
            TypedValue.COMPLEX_UNIT_DIP,
            value.toFloat(),
            resources.displayMetrics,
        ).toInt()

    companion object {
        /** Sentinel for "the theme's default text colour" (no explicit colour). */
        private const val COLOR_DEFAULT = Int.MIN_VALUE

        // The palette is the DESKTOP stylesheet's (crates/werust/src/main.rs
        // `APP_CSS`), so a posture / a console level is the same colour on every
        // platform: trust-verified green, name-via-trusted-rpc blue,
        // mutable-name purple, unverified-origin amber; console info blue, warn
        // amber, error red, debug grey.
        private const val COLOR_TRUST_VERIFIED = 0xFF0A7D28.toInt()
        private const val COLOR_TRUST_NAME_RPC = 0xFF1A5FB4.toInt()
        private const val COLOR_TRUST_MUTABLE = 0xFF6C3FB4.toInt()
        private const val COLOR_TRUST_UNVERIFIED = 0xFF9A6A00.toInt()
        private const val COLOR_CONSOLE_INFO = 0xFF1A5FB4.toInt()
        private const val COLOR_CONSOLE_WARN = 0xFF9A6A00.toInt()
        private const val COLOR_CONSOLE_ERROR = 0xFFC01C28.toInt()
        private const val COLOR_CONSOLE_DEBUG = 0xFF5C5C5C.toInt()

        /**
         * The full text of one console row: `[<level>] <message>` plus the
         * `<source>:<line>` tail in parentheses when there is one. The level tag
         * is the store's OWN wire name, and an absent source/line stays honestly
         * absent (never a fabricated `:0`). The SAME mapping the desktop
         * `console_row_text` applies.
         */
        fun consoleRowText(level: String, message: String, source: String, line: Int?): String {
            val tail = when {
                source.isEmpty() -> ""
                line != null -> " ($source:$line)"
                else -> " ($source)"
            }
            return "[$level] $message$tail"
        }

        /** The colour of one console row by its level, [COLOR_DEFAULT] for log. */
        fun consoleLevelColor(level: String): Int = when (level) {
            "info" -> COLOR_CONSOLE_INFO
            "warn" -> COLOR_CONSOLE_WARN
            "error" -> COLOR_CONSOLE_ERROR
            "debug" -> COLOR_CONSOLE_DEBUG
            else -> COLOR_DEFAULT
        }

        /**
         * The per-request trust label of a network row: the mobile trust
         * indicator's glyph for the posture (`✓` / `◈` / `◇` / `⚠`, the SAME
         * four `Chrome.trustIndicator()` paints) plus the core's wire name the
         * debug JSON carries, never a new label minted for the debug view
         * (ADR-0006). TOTAL and fail-closed: an unrecognised posture renders as
         * the unverified one, never verbatim (a verbatim render could smuggle a
         * minted label into the one surface whose job is honest trust).
         */
        fun networkTrustLabel(trust: String): String = when (trust) {
            "content-verified" -> "✓ content-verified"
            "name-via-trusted-rpc" -> "◈ name-via-trusted-rpc"
            "mutable-name" -> "◇ mutable-name"
            else -> "⚠ unverified-origin"
        }

        /** The colour of a network row by its trust posture (the indicator's hues). */
        fun trustColor(trust: String): Int = when (trust) {
            "content-verified" -> COLOR_TRUST_VERIFIED
            "name-via-trusted-rpc" -> COLOR_TRUST_NAME_RPC
            "mutable-name" -> COLOR_TRUST_MUTABLE
            else -> COLOR_TRUST_UNVERIFIED
        }

        /**
         * The summary line of one network row: method, status, MIME, size and
         * the honest per-request trust label. An unknown field renders as `?`,
         * never a fabricated `0` (the store's own honesty rule).
         */
        fun networkSummaryText(
            method: String,
            status: Int?,
            mime: String,
            size: Long?,
            trust: String,
        ): String = listOf(
            method,
            status?.toString() ?: "?",
            if (mime.isEmpty()) "?" else mime,
            sizeText(size),
            networkTrustLabel(trust),
        ).joinToString("  ")

        /** A human byte count (`512 B`, `1.5 KB`, `2.0 MB`), or `?` when unknown. */
        fun sizeText(size: Long?): String = when {
            size == null -> "?"
            size < 1024 -> "$size B"
            size < 1024 * 1024 -> String.format(Locale.US, "%.1f KB", size / 1024.0)
            else -> String.format(Locale.US, "%.1f MB", size / (1024.0 * 1024.0))
        }
    }
}
