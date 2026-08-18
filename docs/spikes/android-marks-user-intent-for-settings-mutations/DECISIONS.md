# The Android intent signal: the decisions this task baked in

Task `android-marks-user-intent-for-settings-mutations`, spec `settings-mutations-require-user-intent`. The RULE is `docs/adr/0013` and the decisions it baked in are `docs/spikes/settings-mutation-requires-marked-user-intent-in-core-and-on-gtk/DECISIONS.md`; this file records only what the ANDROID edge had to decide on top of them. Its siblings (`macos-`, `windows-`, `ios-marks-user-intent-for-settings-mutations`) face the same two questions in their own platform's vocabulary, so they are recorded rather than buried.

## 1. Android's `hasGesture()` IS part of the condition, where GTK's user-gesture flag deliberately is not

**Chosen:** `IntentMarker::note_page_navigation` marks only when `request.hasGesture()` is true (plus main-frame, not-a-redirect, and a `werust://` source document).

**Why the sibling edge chose differently, and why this one cannot follow it:** the GTK decision (decision 4 of the core task) rejected `webkit_navigation_action_is_user_gesture()` because WebKitGTK offers something better — a navigation TYPE vocabulary (`LinkClicked` / `FormSubmitted` versus `Other` for a script's `location = …`), which is a fact about WHAT the navigation is rather than a flag about the input queue. Android's `shouldOverrideUrlLoading` has no such vocabulary: `WebResourceRequest` exposes the URL, the frame, the method, the headers, the redirect bit and the gesture bit, and nothing that says "this was a form submission". So `hasGesture()` is the ONLY Android spelling of "the user activated this in the page", and dropping it would leave a script's `location = 'werust://settings?…'` indistinguishable from a tap on werust's own form.

**The risk this takes, named:** Android documents that `hasGesture()` "may return false even though the sequence of events which caused the request to be created was initiated by a user gesture". A false negative silently stops the user's OWN settings form applying — which is precisely the failure direction the GTK decision refused to accept from its flag. Three things make it acceptable here: it is FAIL-CLOSED and LEGIBLE (the page renders and says `Not changed: werust did not start this change …`, so the user sees a refusal, not a broken browser); the URL-bar path is unaffected (it marks through `BrowserShell::navigate`); and the reading is MEASURABLE — `SettingsIntentSignalTest.kt` asserts it against the real System WebView, which is exactly why that probe exists.

**MEASURED 2026-08-18 (API 36, System WebView 142.0.7444.174): `hasGesture()` is `true` for a real tap on werust's own settings form and `false` for a script's `location = …` on the same page** (`MEASUREMENTS.md`). The input separates exactly the two shapes it was chosen to separate on the engine werust ships against, so this decision stands as taken.

**If a later hand-run shows `hasGesture()` false for a real tap on werust's own form,** the fix is to drop that ONE input from the condition and rely on the load-bearing fact plus the frame and redirect facts, exactly as GTK does: web content can never BE at a `werust://` URL, and werust's settings page ships no script, so the source-document check is what carries the authorisation. Do not paper over it with a second signal from the page.

**Alternatives considered:** (a) drop the gesture, as GTK did — a narrower condition, but on this edge it would admit a hypothetical script-driven self-navigation with nothing to distinguish it, and Android gives no replacement fact; (b) have Kotlin decide the shape (inspect the form, the method, the referrer) — the request's own contents are under page control at the point the gate reads them, which ADR-0013 forbids; (c) let the settings page tell the chrome through the script bridge — a page claiming intent, also forbidden.

**Touches:** the three sibling edge tasks (each picks its own platform's spelling of "activated in the page": WebKit's navigation type on macOS/iOS, WebView2's `NavigationStarting` + `IsUserInitiated` on Windows), and the probe.

## 2. An UNKNOWN redirect fact reads as a redirect (so API 21–23 refuses the settings form)

**Chosen:** Kotlin reports `redirect = request.isRedirect` on API 24+, and `true` below that, where the platform does not expose the fact at all (`BrowserActivity.isRedirectOrUnknown`; this app's `minSdk` is 21). The Rust rule refuses to mark a redirect, so on API 21–23 the settings page's own form submission is REFUSED.

**Why:** an input to an authorisation must take its fail-closed reading when it is unknown, and the redirect exclusion is not decorative — it is what stops a page navigating to a site that 302s into `werust://settings?backend=…` while `WebView.getUrl()` is still reporting werust's own page (the one window in which the source-document check could read the wrong document). Reporting "not a redirect" because the platform is silent would open exactly that window on the oldest, least-updated devices.

**The user-visible consequence, accepted deliberately:** on an API 21–23 device the settings page renders with real values and its own form says `Not changed: …`, while a change TYPED into the URL bar still applies (that path marks through the shell). That is the same fail-closed shape every unwired edge is in today, on a shrinking slice of devices, and it is legible rather than silent.

**Alternatives considered:** (a) report the convenient answer below API 24 — reopens the redirect window on the devices least likely to get a WebView update; (b) raise `minSdk` to 24 — a shipping decision this task has no business making, and it would drop devices for a corner of one page; (c) pass "unknown" as a third state so the core could decide — the core's answer would be the same refusal, at the cost of a tri-state in the FFI and in the rule.

## 3. The marking RULE lives in the Rust edge, not in Kotlin

**Chosen:** Kotlin's hook reports five facts over one JNI call and applies no condition of its own; `IntentMarker::note_page_navigation` (in `crates/werust-android/rust/src/lib.rs`) decides whether they add up to a mark. A shape guard asserts the Kotlin hook contains no `if`/`when` at all.

**Why:** this repo already forces that split for the chrome derivation — the mobile edges READ `werust_core::chrome_json` rather than re-deriving it, because the hand-written Kotlin/Swift twins had drifted (`docs/adr/0011`). The same argument is far stronger for an authorisation: a drifted copy of a display rule is a wrong glyph, a drifted copy of this one is a hole. Keeping the rule in Rust also puts it inside the Linux `verify` gate, where it is unit-tested with each fact flipped in turn; a Kotlin-side condition would be reachable only by parsing.

**Alternative considered:** mark from Kotlin (a `core.markIntent(url)` call made only when Kotlin judges the navigation worthy). Rejected: it moves the whole authorisation into the one layer this gate cannot execute, and it makes the FFI a marking primitive any future call site could reach for.

## 4. The carrier is bundled with the frame sink in ONE edge type (`IntentMarker`)

**Chosen:** a small Android-crate type holding the `NavigationIntent` and the `RedirectSink` together, cloned beside the session mutex (like the debug store and the backend handle) and handed to Kotlin's hook.

**Why:** the core's `mark(url, frames)` needs both, and both are `Arc`-shared handles, so bundling them once at construction is what lets the UI-thread hook mark WITHOUT the session lock — the ANR guard this edge has already been bitten by twice. It also keeps the Android session from minting a second main-frame notion: the sink it carries is the one `install_ipfs` returned and the shell reports into.

**On the NAME:** "intent" in this codebase already means chrome-marked navigation intent and nothing else (`CONTEXT.md`, `werust_core::intent`), and "mark" is that module's own verb, so `IntentMarker` extends the existing vocabulary rather than forking it. It is deliberately NOT called a "signal": on this edge "page signal" already means the WebView's load-lifecycle callbacks (`on_page_committed` & co.), and re-using the word for the authorisation would blur two unrelated things.

**Alternative considered:** two separate clones on `SyncSession` and a free function taking both. Same behaviour, but the rule and the pair it needs would sit apart, which is how a later caller ends up marking against a sink that is not the shell's.
