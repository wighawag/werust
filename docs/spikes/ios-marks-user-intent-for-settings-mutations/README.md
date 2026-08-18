# iOS supplies the user-intent signal for settings mutations: what is proved, and what is not

Task `ios-marks-user-intent-for-settings-mutations`, spec `settings-mutations-require-user-intent`. The RULE is `docs/adr/0013` (it landed with the core + GTK task); the judgement calls this edge made are `DECISIONS.md` beside this file.

## What landed

- `werust_ios_core::install_settings_page` registers the `werust` scheme against the GATED core entry point (`retrieval::apply_settings_request_with_intent`) and returns an `IntentMarker`: this edge's clone of the ONE `werust_core::intent::NavigationIntent` carrier, bundled with the redirect sink that answers the main-frame half. It was split out of `install_ipfs`, which used to register that scheme against the ungated entry point.
- The session hands that one carrier BOTH ways: to its `BrowserShell` (`with_navigation_intent`, so a change typed into the URL bar marks through the chrome-only front door `textFieldShouldReturn` commits to) and to the Swift navigation-policy hook (so werust's own settings form marks).
- `IntentMarker::note_page_navigation(target, document, main_frame, navigation_type)` is the iOS member of the per-edge marking rule ADR-0013 names. It marks only when ALL of them hold, and the load-bearing one is `document`: web content can never BE at a `werust://` URL.
- Swift: `WKNavigationDelegate.decidePolicyFor` REPORTS the facts (`navigationAction.request.url`, `navigationAction.sourceFrame.request.url`, `navigationAction.targetFrame?.isMainFrame == true`, `navigationAction.navigationType.rawValue`) as its FIRST, unconditional statement and returns `decisionHandler(.allow)`: read-only observation, so WebKit navigates exactly as it did before the report existed. `WKUIDelegate.createWebViewWith` (the `_blank`/`window.open` in-place router, `docs/adr/0010`) reports NOTHING.
- `WerustCore.notePageNavigation` is the C-ABI binding between them, over the new `werust_ios_note_page_navigation` export (declared in `crates/werust-ios/Sources/werust_mobile.h`).
- The `ios` cell of the `settings-mutation-user-intent` row in `docs/platform-capability-matrix.toml` flips `stubbed` -> `implemented`, stating the honest limit below.

## What is PROVED, and by what

Everything below runs on the plain Linux `verify` gate (`cargo fmt --check && cargo clippy --all-targets -D warnings && cargo build && cargo test`). The iOS Rust edge is a normal workspace crate, so its unit tests are gate-run; the Swift shell is reachable from that gate only by PARSING.

| Claim | Where |
|---|---|
| A submission from werust's own settings page applies and PERSISTS | `werust_mobile::tests::a_form_submission_inside_werusts_own_settings_page_applies_the_change` |
| A change typed in the URL bar applies through the carrier this edge hands its handler, and the mark is single-use | `…::a_url_bar_commit_marks_through_the_carrier_this_edge_hands_its_handler` |
| The REGISTERED `werust` scheme handler (the closure Swift's `WerustSchemeHandler` dispatches into) really consults the carrier the shell marks into (the wiring, not the rule) | `…::the_registered_werust_scheme_handler_consults_the_carrier_the_shell_marks_into` |
| A page-started navigation to a mutating settings URL changes nothing, byte-for-byte, and still renders the page read-only with real values | `…::a_page_started_navigation_to_a_mutating_settings_url_changes_nothing` |
| A sub-resource request changes nothing, even while the user is ON the settings page (the query-stripped frame-key case) | `…::a_sub_resource_request_for_a_mutating_settings_url_changes_nothing` |
| `window.open('werust://settings?…')` cannot mutate: the in-place route marks nothing | `…::the_blank_and_window_open_route_cannot_mutate_because_it_marks_nothing` (+ the shape guard below, which is what pins the Swift hook itself) |
| Every fact the mark requires is load-bearing (each flipped alone withholds the mark, including each non-activation `WKNavigationType`) | `…::every_fact_the_mark_requires_is_load_bearing` |
| The C-ABI export Swift calls marshals the facts and tolerates a null session | `…::the_c_abi_reports_a_page_navigation_and_tolerates_a_null_session` |
| The tests never touch the user's own `retrieval.json` | `…::the_intent_signal_never_touches_the_users_own_settings_file` (and the two probes that use the production directory can only ever READ it: they name an unparseable backend kind) |
| The iOS handler is the GATED entry point; the one carrier reaches both handler and shell; the rule requires activation + main frame + a `werust://` source document on BOTH sides; the Swift hook reports facts unconditionally and decides nothing; `createWebViewWith` reports nothing; none of the three iOS files reads a mark | `crates/werust-core/tests/settings_user_intent_edge_wiring_shape.rs` (the appended iOS block) |

The shape guard's teeth were re-checked by mutation: pointing the iOS handler at the ungated `apply_settings_request` reds `ios_serves_the_settings_page_through_the_gated_core_entry_point` (and the wiring unit test); dropping `.with_navigation_intent` reds `ios_hands_the_one_carrier_to_both_the_handler_and_the_shell` (and two unit tests); deleting the Swift report reds `the_ios_swift_shell_reports_the_facts_and_decides_nothing`; wrapping that report in an `if` reds the same test; adding a report to the `_blank` hook reds `ios_never_marks_the_blank_and_window_open_path`; dropping the source-document requirement from the rule reds `ios_marks_only_a_navigation_activated_inside_a_surface_werust_drew` plus two unit tests. The Swift block extractor has its own guard (`the_ios_swift_block_extractor_stops_at_the_matching_brace`) on a fixture with iOS's multi-line signatures, because the negative assertions are exactly the kind that pass vacuously on a mis-bounded slice.

## HONEST LIMIT: the iOS CI leg BUILDS and LAUNCHES but asserts NO behaviour, and there was NO Simulator run

`.github/workflows/mobile-ios.yml` cross-compiles the Rust core for `aarch64-apple-ios-sim`, links it into the Xcode app, launches that app on an iOS 17 Simulator, and re-produces the `.app` to assert the bundle carries the binary with the Rust core linked. It asserts **no behaviour at all**: it never opens `werust://settings`, never submits its form, and never runs a hostile page. So what that leg proves about this change is exactly one thing: **the Swift shell, its C-ABI binding, the new header declaration and the Rust export still COMPILE and LINK together**, which is not nothing (the Swift half is otherwise unbuilt by any gate) and is not the refusal.

**No Simulator or device run was performed, and none is claimed.** Nobody on this project has a Mac (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`), which is why the Swift half is pinned by a source-shape guard in the first place. Unlike the Android sibling (which HAND-RAN an instrumented probe and recorded readings), this edge has **no runtime reading**, so two facts remain unobserved on iOS:

1. what `navigationAction.sourceFrame.request.url` reports for a form submission inside werust's own `werust://settings` page (the fact the whole authorisation rests on), and
2. whether the URI WebKit hands the `WKURLSchemeHandler` is byte-for-byte the URI this hook reported (the core's `intent_key` collapses the spellings WebKit is known to introduce, so a difference is expected to reduce to the same key rather than to refuse).

If either reads the other way the failure is FAIL-CLOSED and legible: the settings page renders and says `Not changed: werust did not start this change, so nothing was applied (a page cannot change your settings).` The user's own change is refused, and nothing a page does applied. `DECISIONS.md` decision 2 records what to change if a run ever shows that.

### Manual verification steps (UNRUN, for whoever next has a Mac)

Written down so the evidence gap is a checklist rather than a memory. Nothing below has been walked.

1. `crates/werust-ios/build-and-run.sh` (builds the Rust static lib, the app, and boots the Simulator).
2. Type `werust://settings` in the URL bar. EXPECT: the page renders and shows the active backend. (Reads are ungated.)
3. Type a custom gateway URL into the custom-backend field and tap `use this`. EXPECT: the change APPLIED (`Saved: …` if the app has a settings directory) and the custom endpoint active. This is the path only this task supplies: the form is a page-initiated GET, so it marks through `decidePolicyFor`.
4. Tap `use this` on the default-gateway option (a link, not a form). EXPECT: applied, because `.linkActivated` is an activation too.
5. Type `werust://settings?backend=custom&url=http://127.0.0.1:7777` straight into the URL bar. EXPECT: applied. (The chrome's front door marks.)
6. Reload the page from step 5. EXPECT: `Not changed: …` and no second application (marks are single-use, decision 2 of the core task). An edge-swipe back onto such a URL reads the same way, for the same reason.
7. Load any page containing `<img src="werust://settings?backend=custom&url=http://attacker.example/">`, then re-open `werust://settings` and confirm the active backend is UNCHANGED. (The attack.) Tapping an in-page LINK on that page to the same URL renders the settings page read-only with `Not changed: …` and the real current values.
8. From that page run `window.open('werust://settings?backend=custom&url=http://attacker.example/')`. EXPECT: the settings page loads in place (`docs/adr/0010`) showing REAL values and `Not changed: …`.

If step 3 or 4 refuses, capture `sourceFrame.request.url` for that navigation before changing anything (`DECISIONS.md`, decision 2).

## What this closes, and what it does not

It closes the iOS quarter of the residual window the core task recorded: since the core gate landed, this edge's settings page RENDERED but refused every change (its own form AND a URL-bar-typed one) because its handler used the ungated entry point and no carrier reached it. Both work again, and nothing a page controls does.

It does not touch the two remaining edges (`macos-`, `windows-marks-user-intent-for-settings-mutations`), which still fail closed in the same way, nor anything about WHAT the settings are (`retrieval-default-egress-before-final-release` owns the default). It also does not add behavioural assertions to `mobile-ios.yml`: a CI-measurable criterion needs its leg on `main` first (`CONTEXT.md`, Conventions), so an iOS behaviour leg is its own change, not a side effect of this one.
