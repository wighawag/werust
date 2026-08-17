# Settings mutations require chrome-marked user intent: what is proved, and what is not

Task `settings-mutation-requires-marked-user-intent-in-core-and-on-gtk`, spec `settings-mutations-require-user-intent`. The rule is `docs/adr/0013`; the judgement calls are `DECISIONS.md` beside this file.

## What landed

- `werust_core::intent::NavigationIntent` — the shared, `Send + Sync` carrier holding at most one mark, and `take_mark_for(uri)`, the ONE call that answers both halves of the gate (the chrome marked exactly this navigation, AND this request is its main frame) and spends the mark.
- `werust_core::retrieval` — `apply_settings_request_with_intent` applies a change only through that gate; a refused attempt renders the page with its REAL current values and the `NOT_STARTED_BY_WERUST` reason. `apply_settings_request` (no carrier) renders and refuses. Reads are untouched.
- `BrowserShell::navigate` marks; `BrowserShell::with_navigation_intent` shares the carrier with the edge.
- GTK: `WebViewRenderer::install_settings_page` (the `werust://` scheme handler against the gated entry point, plus the `decide-policy` hook that marks a link/form navigation started inside werust's own page), wired in `crates/werust/src/main.rs`.
- `docs/platform-capability-matrix.toml` gains the `settings-mutation-user-intent` row; `crates/werust-core/tests/settings_user_intent_edge_wiring_shape.rs` pins the wiring and is structured for the four sibling edges to APPEND to.

## What is PROVED, and by what

Everything below runs on the plain Linux `verify` gate (`cargo fmt --check && cargo clippy --all-targets -D warnings && cargo build && cargo test`):

| Claim | Where |
|---|---|
| A sub-resource request for a mutating settings URL leaves the settings file unchanged byte-for-byte | `retrieval::tests::a_sub_resource_request_for_a_mutating_settings_url_changes_nothing` |
| A main-frame request with NO mark changes nothing; a marked navigation whose request is a sub-resource changes nothing | `retrieval::tests::a_main_frame_request_with_no_marked_intent_changes_nothing`, `…a_marked_navigation_whose_request_is_a_sub_resource_changes_nothing` |
| The mark is not satisfiable by a query-stripped frame-key match | `retrieval::tests::marked_intent_is_not_satisfied_by_a_query_stripped_frame_key_match`, `intent::tests::a_mark_does_not_carry_over_to_a_different_query` |
| A marked main-frame request applies and persists exactly as before | `retrieval::tests::a_main_frame_request_with_marked_intent_applies_and_persists` |
| A refused attempt still renders the page with real values, distinguishably from a success | `retrieval::tests::a_refused_attempt_still_renders_the_page_with_the_real_current_values` |
| Reads are not gated | `retrieval::tests::reading_the_settings_page_is_never_gated` |
| The chrome's front door marks; an OBSERVED page navigation does not | `werust_core::tests::the_chrome_front_door_marks_user_intent_and_a_page_started_navigation_does_not` |
| The `_blank`/`window.open` hook marks nothing; the seam request was not widened | `settings_user_intent_edge_wiring_shape.rs` |
| The tests never touch the user's own settings file | `retrieval::tests::the_gate_never_touches_the_users_own_settings_file` |

The shape guard's teeth were re-checked by mutation: pointing the GTK handler at the ungated `apply_settings_request` reds `gtk_serves_the_settings_page_through_the_gated_core_entry_point`, and changing the accepted navigation kinds reds `gtk_marks_only_a_navigation_activated_inside_a_surface_werust_drew`.

## HONEST LIMIT: no CI leg drives a real WebKitGTK settings-page submission

This repo's gate is headless and has no GTK/WebKitGTK smoke, so nothing here has actually loaded `werust://settings` in a real webview and clicked its form. The desktop evidence is the shared core's tests plus the source-shape guard over the wiring — NOT an observed form submit. Two specific things that only a running WebKitGTK can settle:

1. that `decide-policy` reports a settings-page form GET as `FormSubmitted` (and a `use this` link as `LinkClicked`) with `WebView::uri()` still on the `werust://` document, and
2. that the URI WebKit hands the scheme handler for that navigation reduces to the same `intent_key` as the URI the hook marked (the spellings `intent_key` collapses are the ones already observed elsewhere in this codebase, but the query is compared verbatim).

If either is false, the failure is FAIL-CLOSED and legible: the page renders and says `Not changed: werust did not start this change…`.

### Manual verification steps (a human at a GTK desktop)

Run against a scratch settings directory so the real one is untouched:

```
WERUST_SETTINGS_DIR=$(mktemp -d) cargo run -p werust
```

1. Type `werust://settings` in the URL bar and press Enter. The page renders and shows the active backend. (Read: ungated.)
2. Type a custom gateway URL into the custom-backend field and press `use this`. Expect `Saved: retrieval backend is now custom.` and the new endpoint shown as active. (Form submission inside werust's own page IS marked.)
3. Press `use this` on the default-gateway option. Expect `Saved: …`. (A link click inside werust's own page IS marked.)
4. Type `werust://settings?backend=default-gateway` straight into the URL bar. Expect `Saved: …`. (The chrome's front door marks.)
5. Reload the page from step 4. A reload re-requests the same mutating URL, so expect `Not changed: werust did not start this change…` and no second application of the change (the active backend is the one step 4 already saved). (Decision 2 in `DECISIONS.md`: marks are single-use and a reload does not mark.)
6. Load any page containing `<img src="werust://settings?backend=custom&url=http://attacker.example/">` and confirm `$WERUST_SETTINGS_DIR/retrieval.json` is byte-for-byte unchanged. (The attack.)
7. From that page, `window.open('werust://settings?backend=custom&url=http://attacker.example/')`. The settings page loads in place (ADR-0010) showing REAL values and `Not changed: …`; the file is unchanged.

## The residual window, stated plainly

Until an edge's own task lands, that edge's `werust://` handler has NO carrier, so it takes the ungated `apply_settings_request` path: its settings page still RENDERS with real current values, and EVERY change is refused — the settings page's own form AND a change typed into that edge's URL bar.

That is one step wider than the task predicted ("a URL-bar-typed change still works, because that path goes through the shell"). It is wider because the mark reaches a scheme handler only through the carrier the EDGE clones into it: `BrowserShell::navigate` writes the mark, but on an unwired edge nothing reads it. Closing that half without the edge's wiring would need a process-global carrier, which this repo has deliberately removed from exactly this kind of state and which would let one window's mark authorise another window's request (`DECISIONS.md`, and ADR-0013's rejected options). The direction is fail-CLOSED and the four sibling tasks close it; the `settings-mutation-user-intent` matrix row names them.
