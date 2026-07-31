# Decisions: collapse the Kotlin and Swift chrome twins onto the one derivation

Task: `mobile-chrome-presentation-from-one-derivation` (`docs/adr/0011`, the third and last copy of the chrome presentation).

The measured cost of the mechanism chosen in D1 is recorded beside this file, in `MEASUREMENT.md`.

## D1. The derived strings ride the CHROME JSON; the per-field FFI is rejected

**Chosen:** mechanism (a), the task's prescribed default. `werust_core::chrome_json` now emits the chrome FACTS **plus** ten DERIVED fields (`statusLine`, `trustIndicator`, `trustIndicatorDetail`, `errorBannerVisible`, `errorBannerText`, `invalidEntryBadgeVisible`, `invalidEntryBadgeText`, `loadProgressVisible`, `loadProgressFraction`, `loadProgressHint`), each the return value of the core rule of the same name. Each mobile edge reads a field where it used to run a `when` / `switch`.

Nothing blocked (a). Both edges already decode this exact document on every chrome refresh, so no FFI surface was added at all; the derivation stays in one place in Rust; and both mobile bindings got SMALLER instead of growing, despite carrying a new field each (`WerustCore.kt` 565 -> 508 lines, `WerustCore.swift` 540 -> 489), because a rule became a property.

**Alternative considered:** (b), exposing each rule over the FFI and calling it per field. Rejected as the task named it: it multiplies FFI entry points (ten more JNI methods and ten more C exports, each with its own string lifetime) for values the edge needs on every refresh anyway, on a boundary the chrome already crosses at the same cadence. It would also have made a refresh N round trips instead of one, which is worse on the axis (the Android ANR guard) this repo actually cares about.

**Cost, measured:** encode + decode goes from ~2.1 µs to ~4.6 µs per refresh, on a document ~440 B larger (~258 B -> ~697 B). The refresh is event-driven (after each core action / page signal), so that is a handful of times per navigation. See `MEASUREMENT.md` for the table and for what is NOT measured.

**What it touches:** both mobile bindings, both mobile painters, and any FUTURE non-Rust edge, which now gets the whole derivation by decoding one document.

## D2. ONE encoder, in the core: the two `ffi_json` twins are deleted

**Chosen:** the chrome wire form moved to `werust_core::chrome_json`, beside `ChromeState` and the rules it carries. `crates/werust-android/rust/src/ffi_json.rs` and `crates/werust-ios/rust/src/ffi_json.rs` are gone; both `CoreSession::chrome_json` methods call the core.

This is slightly wider than "add fields to the chrome JSON", and it is deliberate: those two modules were byte-for-byte twins of each other (the iOS one said so in its own module docs), so adding ten fields under the task's own mechanism would have meant adding them TWICE, committing the exact duplication the task exists to remove, one level below the Kotlin/Swift twins. The core is also where the other two documents every edge decodes already live (`werust_core::menu::menu_json`, `werust_core::debug::debug_json`), so this removes an inconsistency rather than creating one.

The encoder is `serde_json`-based now, like those two siblings, instead of the hand-rolled `format!` + `escape` the mobile crates used to keep them dependency-light. That reason had already lapsed: both mobile cdylibs link `werust-core`, which pulls `serde_json` for the menu and debug documents regardless.

**Consequence, accepted:** JSON object KEY ORDER changed (serde_json emits sorted keys). Object order is not a contract (both edges decode with a real parser, `org.json` and `JSONSerialization`), so the exact-string assertions the moved tests carried became parsed-document assertions of the same substance (`the_chrome_json_document_is_exactly_the_facts_plus_the_derived_fields` pins the whole document, key set and values, order-independently). Any consumer that had been string-matching the wire form would notice; none does.

**Alternative considered:** keep both `ffi_json` modules and add the ten fields to each. Rejected: two copies of a wire form that must agree byte for byte is the same disease at a lower altitude.

**What it touches:** `werust-android-core` and `werust-ios-core` (both lost a module), and any later edge that wants the chrome document, which now calls one function.

## D3. The derived fields are named after the CORE RULES, not after `ChromePaint`

**Chosen:** `statusLine` / `trustIndicator` / `trustIndicatorDetail` / `errorBannerText` / `invalidEntryBadgeText` / `loadProgressFraction` and the rest: the core's `status_line`, `trust_indicator`, `trust_indicator_detail`, `error_banner_text`, `invalid_entry_badge_text`, `load_progress_fraction`, in the camelCase the rest of this document already uses (`canGoBack`, `loadStep`, `failureKind`).

**Alternative considered:** mirror `desktop_paint::ChromePaint`, the OTHER carrier of this same derivation (`status_text`, `trust_text`, `trust_detail`, `error_text`, `progress_fraction`). Rejected: those are a Rust struct's local abbreviations, legible because the struct's own name gives the context; a flat JSON document has no such context, and matching an edge field to its core rule by name is the property that makes a drift visible. The two carriers therefore differ in SPELLING but not in vocabulary: every field of both is the return value of the same named rule.

**What it touches:** both mobile edges (their properties carry these names), and the wire form any future non-Rust edge decodes.

## D4. COLOUR does not cross; the mobile edges keep choosing it from the FACTS

**Chosen:** the `*_css_class` identifiers (`trust-verified`, `error-banner-transient`, …) are NOT carried on the chrome JSON. Each mobile painter keeps picking its own native colour, from the FACTS already on the wire (`retryable` for the banner's severity, `trustPosture` for the badge).

This is the layering the core already states: class names are stable state IDENTIFIERS, and the stylesheet that gives one a colour stays in the edge that has a stylesheet (GTK `APP_CSS`) or a palette (`desktop-paint::CLASS_COLORS`, for AppKit and Win32, which have neither). Neither mobile edge has either; carrying the class name would force Kotlin and Swift to match magic identifier strings (`if (chrome.errorBannerCssClass == "error-banner-transient")`) to reach the same colour they already select from `retryable`: more drift surface, not less, and a *second* place where the severity rule is spelled out.

**Alternative considered:** carry `trustIndicatorCssClass` + `errorBannerCssClass` and have each mobile edge map identifier -> colour, as `desktop-paint` does. Not rejected on principle: it becomes the right move the day the mobile edges want the SAME hues as the desktops, which is a real (and currently unmet) parity question, since the mobile colours are hand-transcribed hex literals today. Deferred as its own change, because it is a colour-parity decision, not part of collapsing the presentation rules.

**What it touches:** any later "one palette across every edge" task; the mobile hex literals stay where they are until then.

## D5. The trust EXPLANATION on mobile: an accessibility description PLUS a tap

**Chosen:** on both mobile edges the trust badge now carries `trustIndicatorDetail` as its accessibility description (Android `contentDescription`, iOS `accessibilityLabel`) AND is tappable, showing the same sentence in the platform's standard alert (framework `AlertDialog` / `UIAlertController`), titled with the badge it explains.

**Superseded in part, 2026-07-31 (task `mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay`):** the SLOT was wrong. Putting the explanation in the badge's accessibility LABEL replaced the badge's own state name, so a screen-reader user heard the ~240-character sentence on every focus and never heard WHICH posture the badge was in. The explanation now rides in each platform's SECONDARY slot (Android `stateDescription`, iOS `accessibilityValue`), where it follows the state instead of replacing it; the tap affordance below is unchanged, and both strings still come from the same one derivation. See `work/notes/observations/mobile-trust-badge-accessibility-slot-decisions-2026-07-31.md`.

The task allowed "a tap/long-press affordance, an accessibility label, or an info row". Both were taken rather than one, because they serve different users and neither alone is enough: the accessibility description is what a screen-reader user gets (today they hear a glyph and three words), the tap is what a sighted user can discover. Both read the SAME one field, and the tap reads it back off the PAINTED badge rather than calling the core again, because a chrome read takes the native session lock, which on the UI thread can wait behind an in-flight `ipfs://` retrieval (`work/notes/observations/mobile-chrome-reads-still-take-the-session-lock-2026-07-29.md`).

**Alternatives considered:** a permanently visible INFO ROW (rejected: it spends scarce phone chrome height on a sentence that is only occasionally wanted, and the mobile chrome has already been trimmed once for crowding, `work/notes/observations/review-nits-fix-mobile-chrome-urlbar-crowded-by-buttons-2026-07-22.md`); a LONG-PRESS (rejected: undiscoverable, and Android long-press on a label conflicts with text selection); a TOAST (rejected: the explanation runs to ~240 characters and a toast truncates awkwardly and times out before it can be read).

**What it touches:** the `trust-explanation` row added to `docs/platform-capability-matrix.toml`, and the two desktop painters only in that they already had it (a tooltip).

## D6. An unreadable chrome shows NOTHING, not a re-stated default

**Chosen:** the Swift `Chrome.idle` fail-soft fallback (and every decoder default on both edges) leaves the DERIVED strings EMPTY, rather than restating the core's wording for an idle chrome.

`Chrome.idle` is what iOS returns when the C-ABI hands back no document at all (a freed session). Before this task it restated the facts only, which is harmless; a derived default would have to restate `"⚠ unverified origin"` in Swift: a fresh twin of the very rule this task removed, in the one file whose whole point is that it no longer carries one. Showing nothing is also the honest claim when the chrome could not be read: a trust badge must never be asserted from a fallback (`docs/adr/0006`).

**User-visible consequence, accepted:** in that unreachable path the footer would be blank instead of reading "idle" / "⚠ unverified origin". The path is not reached in a working build (the pointer is non-null and the document is core-generated), and the FACTS still decode to an idle chrome, so nav enablement is unaffected.

**Alternative considered:** literal defaults matching the core's idle derivation. Rejected: a silent twin, and one that would drift on the first rewording.

## D7. One UNIT for load progress: the core's fraction, scaled at the widget

**Chosen:** `loadProgressFraction` crosses as the core's 0.0-1.0 `f64`. Android multiplies by the `ProgressBar`'s own `max` at the paint site (`(chrome.loadProgressFraction * loadingProgress.max).roundToInt()`); iOS narrows to `Float` for `UIProgressView`, which is already on the 0-1 scale.

The Kotlin twin returned a PERCENT (25 / 45 / 70 / 90) while Rust and Swift returned fractions: the unit fork the task named. Scaling to a widget's range is arithmetic the widget owns, and reading `loadingProgress.max` instead of restating `100` keeps even that from being a second literal.

**What it touches:** nothing outside the Android painter; the wire and both other edges are now one unit.

## D8. The alert's dismiss label

**Chosen:** Android uses the PLATFORM's localized `android.R.string.ok`; iOS mints the string `"OK"`.

iOS has no equivalent platform constant for an alert action title, and this app has no localization table at all (every werust string is English today). Recorded rather than buried because it is the one user-visible string in this change that is NOT the core's: if werust ever localizes, this is a site that needs it, and the Android side already does the right thing.

## The DIVERGENCES the collapse found (acceptance criterion 3)

Each was a twin disagreeing with, or missing from, the Rust original. All are resolved toward the shared derivation.

1. **The trust EXPLANATION was missing from BOTH mobile edges.** `trust_indicator_detail` (including "werust is loading this page and is not yet asserting a trust level for it") existed only on desktop; neither Kotlin nor Swift had it in any form. This is the headline drift the task was written around: a trust badge with no explanation on the two platforms most users are on, in a browser whose thesis is an honest, legible trust posture. Resolved: carried on the chrome JSON and surfaced on both (D5), with a capability-matrix row.

2. **The load-progress UNIT forked.** `loadProgressPercent()` in Kotlin (25/45/70/90) vs `load_progress_fraction` / `loadProgressFraction()` in Rust and Swift (0.25/0.45/0.7/0.9). Resolved: one fraction on the wire (D7).

3. **iOS painted the invalid-entry badge from a BUILD-TIME literal it never refreshed.** `invalidBadge.text = "⛔ invalid URL"` was set once in `layoutChrome()` and never re-read from the rule, while Android and desktop painted it from `invalid_entry_badge_text` on every refresh. It agreed today by coincidence of transcription; the first rewording of that badge would have shipped everywhere EXCEPT iOS, silently. Resolved: painted from `invalidEntryBadgeText` in `refreshChrome()`.

4. **Both painters hardcoded the INITIAL chrome text.** `trustLabel.text = "⚠ unverified origin"` / `trust = TextView(this).apply { text = "⚠ unverified origin" }` and `statusLabel.text = "idle"` / `status = TextView(this).apply { text = "idle" }` were literal copies of `trust_indicator` / `status_line` for the starting chrome, set before the first refresh. Same class of bug as (3), one repaint away from being invisible. Resolved: both edges now paint their initial labels from `core.chrome()`'s own derivation.

5. **A latent one, worth naming even though nothing disagreed yet:** every mobile twin was TOTAL over the wire names it knew and fell through to a default for anything else (`else -> "⚠ unverified origin"`, `default: return ""`). So a FIFTH `TrustPosture` or a SIXTH `LoadStep` would have reached both mobile edges as the fallback (an unverified badge on a verified page) with every test green. That failure mode is gone by construction: the edges no longer branch on the wire names at all.

A guard now keeps them gone: `crates/werust-core/tests/mobile_chrome_presentation_shape.rs` drives the forbidden literals FROM the core (over `TrustPosture::ALL` / `LoadStep::ALL`, both kept complete by a compile-time check), so a fifth posture's badge and explanation join the forbidden list automatically, and an edge that hand-wrote either reds the gate.
