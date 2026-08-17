# The settings-mutation intent gate: the decisions this task baked in

Task `settings-mutation-requires-marked-user-intent-in-core-and-on-gtk`, spec `settings-mutations-require-user-intent`. The RULE itself is `docs/adr/0013` (it is a security boundary and hard to reverse). These are the in-scope judgement calls behind `crates/werust-core/src/intent.rs`, the gate in `crates/werust-core/src/retrieval.rs` and the GTK wiring in `crates/webview-renderer/src/backend.rs` + `crates/werust/src/main.rs`. Four sibling edge tasks (`macos-`, `windows-`, `ios-`, `android-marks-user-intent-for-settings-mutations`) inherit them, so they are recorded rather than buried.

## 1. The ungated entry point KEPT its name and signature, and now refuses mutations

**Chosen:** `retrieval::apply_settings_request(request)` is unchanged in name and signature and now means "no intent carrier: render the page, refuse any change". The gated entry point is a NEW function beside it, `apply_settings_request_with_intent(request, intent)`, and the directory-taking core `apply_settings_request_in` took the extra parameter (it has no caller outside this crate's tests).

**Why:** the four other edges call `apply_settings_request` from halves this Linux gate cannot compile (macOS, Windows, iOS, and an Android JNI module that is `cfg(target_os = "android")`). Changing its signature would edit four uncompilable-here files in a task whose acceptance is measured on Linux — the invisible-break hazard the task named — while giving those edges nothing, since none of them has a carrier to pass yet. Keeping it lets this task touch ZERO non-Linux source, and each sibling task moves its own edge over in one line as part of wiring the carrier it needs anyway.

**Alternatives considered:** (a) change the signature and pass a never-marked carrier from each of the four edges — same fail-closed behaviour, but four blind edits and four legs to re-run for no behaviour change; (b) rename the ungated one to `render_settings_request` for honesty and leave an alias — the alias is the same compatibility surface with an extra name in the vocabulary; (c) leave the ungated entry point APPLYING mutations so nothing changes for un-wired edges — rejected outright: that keeps the hole open on four of five edges, which is the whole thing the spec is about.

**Touches:** all four sibling edge tasks (each switches its handler to `apply_settings_request_with_intent`), and the residual-window note below.

## 2. The mark is SINGLE-USE, and a later navigation REPLACES it

**Chosen:** `NavigationIntent::take_mark_for` spends the mark on success, and `mark` overwrites any outstanding one. So at most one authorisation exists at a time, and it survives exactly one request.

**Why:** the request that spends it is the main document's own, which is the first request of that navigation; everything after it (the rendered page's sub-resources, a replay, a late duplicate) is something the user did not ask for a second time. Overwriting on the next navigation means a mark cannot lie in wait across a browsing session for a request that happens to name the same URL. Neither property is strictly required by the two-condition rule; both narrow the window at no cost to the user's own change.

**The visible consequence, accepted deliberately:** a RELOAD of a mutating settings URL, and a Back/Forward onto one, re-render the page WITHOUT re-applying the change (they go through `reload` / `go_back` / `go_forward`, which do not mark). A history move is not a fresh decision to change a setting, and the page says plainly that nothing was changed. If that ever reads as a bug rather than as caution, the fix is one more `mark` call at those entry points, not a change to the rule.

**Alternatives considered:** a mark that stays valid until the next navigation (simpler, but every sub-resource of the settings page would carry a live authorisation for that exact URL); a mark with a time bound (a clock in a security predicate, untestable without injecting one).

## 3. A mismatched request does NOT burn the outstanding mark

**Chosen:** `take_mark_for` clears the mark only when it authorises. A request that does not match leaves it alone.

**Why:** otherwise any page could spend the user's pending authorisation by requesting some other `werust://` URL first, turning the gate into a denial of the user's OWN change. The failure would look like "settings randomly stopped saving", which is exactly the kind of thing nobody would trace back to here.

## 4. The GTK edge marks from the navigation ORIGIN, not from WebKit's user-gesture flag

**Chosen:** the `decide-policy` hook marks when BOTH (a) the navigation type is `LinkClicked` or `FormSubmitted` and it is not a REDIRECT of one — the user activated something in the page, never a script's `location =`, which reports `Other` — and (b) the document it starts FROM is a `werust://` URL. `webkit_navigation_action_is_user_gesture()` is deliberately NOT part of the condition.

**Why the redirect exclusion is part of (a):** werust's own internal handler never issues a redirect, so no legitimate marking path is a redirect; and a redirect re-fires this callback carrying the ORIGINAL navigation type at a point where the view's active URI has already begun moving toward the destination, which is the one window in which (b) could read the destination instead of the source. Excluding it removes that reasoning entirely rather than relying on WebKit's exact update ordering.

**Why:** (b) is the load-bearing fact and it is not forgeable: web content can never be AT a `werust://` URL (only werust serves that scheme, and it serves one script-free page), so nothing a hostile page controls satisfies it. The gesture flag would add defence only against a script on a `werust://` page, which cannot exist unless werust itself puts one there — and it is a WebKit-reported boolean that this repo's Linux gate cannot exercise (there is no driven WebKitGTK settings-page test), so a false negative from it would silently stop the user's own form working with every test green. A fact we can reason about beats a flag we cannot observe.

**Alternatives considered:** (a) require the gesture flag as well — safer in theory, untestable here, and its failure direction is a silently broken settings form; (b) mark on ANY navigation started from a `werust://` document (drops condition (a)) — allows a hypothetical future script-driven self-navigation, and (a) is free; (c) let the page tell the chrome through the script bridge — that is a page claiming intent, which the ADR forbids.

**Touches:** the three sibling edges with a navigation-policy callback of their own (macOS/iOS `decidePolicyForNavigationAction`, Windows WebView2's navigation-starting event, Android's `shouldOverrideUrlLoading`) inherit this shape; each has its own navigation-kind vocabulary, so the shape guard asserts per edge rather than by shared string.

## 5. The intent key is the frame key PLUS the query, not a second normalization

**Chosen:** `intent::intent_key` runs the URL through the existing `frame_key` and re-attaches the query, then collapses the empty-authority and bare-trailing-slash spellings for non-`ipfs://` schemes (which `frame_key` leaves alone because it only ever saw `ipfs://` URLs).

**Why:** the task's first named hazard is that a naive main-frame check passes for a mutating sub-resource URL, because the frame key strips the query. The fix is not a second main-frame notion (the codebase deliberately has ONE), it is a key that is strictly tighter. Building it ON `frame_key` is what guarantees the two can never disagree about which document a URL names.

**Why the extra collapses:** WebKit hands the scheme handler its own spelling of the URL the chrome asked for. The `ipfs:///<cid>` authority-less form is already documented in `frame_key`, and the settings handler's own host parse already tolerates `werust://settings/?…`, so both spellings demonstrably occur. A raw string compare would refuse the user's own change on whichever edge normalises.

**Not adopted:** normalising the QUERY (sorting parameters, re-decoding percent escapes). It is compared verbatim: it is the part that carries the change, no edge has been observed to respell it, and a normaliser there is a parser in the authorisation path.
