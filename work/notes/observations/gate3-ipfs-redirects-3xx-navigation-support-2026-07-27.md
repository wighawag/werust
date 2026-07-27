---
title: "Gate-3 conductor review: ipfs-redirects-3xx-navigation-support (APPROVE)"
date: 2026-07-27
status: open
reviewOf: ipfs-redirects-3xx-navigation-support
verdict: approve
---

## Verdict: APPROVE

Merged as `652ba6d`. Gate-1 (acceptance) and Gate-2 (PR/code review) both green; this is the conductor's third-layer diff-vs-criteria pass, verified against what ACTUALLY landed on `origin/main` (not the branch, not intent). 251 `werust-core` tests re-run locally green in a throwaway worktree.

## Acceptance criteria, ticked against the merged tree

- [x] **A matching 3xx NAVIGATES instead of failing with RedirectNotSupported.** `redirects.rs` `resolve_target` now yields `FallbackAction::Redirect { path, status }` for the 3xx set; `ipfs.rs` pushes an absolute `ipfs://<rootcid><to>` into a shared `RedirectSink` and the shell follows it on its existing pump (`follow_pending_redirect` -> `renderer.navigate`), so the bar and history move. `RedirectsError::RedirectNotSupported` is GONE from the tree (grep-verified, not assumed). Tests: `a_matching_3xx_rule_queues_a_navigation_to_the_target_under_the_same_root_cid`, `every_3xx_status_navigates_and_a_defaulted_status_redirects_too`.
- [x] **Placeholder/`:splat` injection works for 3xx; the `to` is confined to the SAME root CID.** The expand-then-`within_root_path` order is unchanged from the parent task, so an off-root escape hidden in a capture is still caught. Tests: `placeholders_are_captured_and_injected_including_repeats`, `an_off_root_3xx_target_is_refused_and_never_queued_for_navigation`, `an_off_root_escape_hidden_in_a_capture_is_refused_too`.
- [x] **Verification intact; a 3xx `to` that does not resolve fails closed.** The redirect is performed as a real navigation, so the target is hash-verified by the fresh retrieval that navigation triggers: no bypass was added, and the `SchemeResponse` seam was NOT widened into a redirect channel (its doc comment forbidding that is respected). Tests: `a_redirect_target_that_does_not_resolve_fails_closed_on_the_next_hop`, `a_tampered_redirects_file_fails_the_load_and_never_falls_back_to_guessing`.
- [x] **The chain is BOUNDED.** `MAX_REDIRECT_HOPS = 5` with a `visited` set, so a cycle and an over-long chain both fail closed with a legible reason. Test: `a_redirect_chain_is_bounded_and_a_cycle_fails_closed`.
- [x] **Parent behaviours unregressed.** 200-rewrite, custom 404, 410/451, no-`_redirects`, and the jolly-roger catch-all all still pass.
- [x] **Applied on desktop + mobile, parity-tracked.** A new `ipfs-redirects-3xx-navigation` capability row was split out of `ipfs-web-pathing-fallback` (correctly: one SERVES in place, the other CHANGES the page identity), marked implemented on all three contexts, with desktop/Android/iOS edges wired.

## The three defects Gate-2 caught, and how each was closed

Gate-2 blocked this work three times. Every block was CORRECT and every fix was prescribed by the conductor as a requeue handoff rather than escalated, because each was a precisely-diagnosed, fixable defect rather than a human-decision fork:

1. **Chain state was session-scoped, not per-chain.** `visited` was only cleared by shell-level entry points, so an in-page link click never reset it: the same redirecting link worked once and was then refused as a cycle. Fixed by resetting the chain whenever a load that is NOT the chain's own target commits. Tests: `an_in_page_link_click_resets_the_redirect_chain_budget_too`, `unrelated_redirected_link_clicks_never_exhaust_the_hop_budget` (the pre-existing test only exercised `shell.navigate`, which is exactly why the gap survived).
2. **A 3xx on a SUB-RESOURCE yanked the whole page.** The scheme handler fires for the main document AND every sub-resource, but a match always queued a top-level navigation, so a stale image matching a rule navigated the browser away from the page being read. Fixed by inferring the main frame from the top-level URL in core (the seam carries no `is-main-frame` flag) and never queueing or spending budget for a sub-resource. Recorded as Decision 7 with the seam limitation named. Tests: `a_matched_3xx_on_a_sub_resource_never_navigates_and_spends_no_hop_budget`, `the_main_frame_check_survives_the_webkit_authority_less_url_form`.
3. **A back-trap.** The redirect PUSHES a history entry, so Back landed on the redirecting URL, whose rule re-fired and bounced the user forward. A true replace is unavailable (the seam has no replace-current-entry and WebKitGTK exposes no public API to replace/remove a back-forward entry), so the prescribed fix was the standard emulation: remember each hop's redirect SOURCE and have Back skip over it. Recorded as Decision 8 including the first-entry edge case. Tests: `back_after_a_redirect_skips_the_redirecting_entry_instead_of_bouncing_forward`, `back_over_an_ordinary_entry_is_untouched_by_the_redirect_skip`.

## Coherence

No trust vocabulary was re-meaned: the redirect rides the ordinary verified retrieval and the posture machinery is untouched. Decision 3 verifies (rather than assumes) that the root-CID-prefix `ens_pages` association already keeps a `.eth` identity across a redirect, confirmed by `a_redirect_inside_an_ens_site_keeps_the_eth_identity_in_the_bar`. The 3xx was deliberately kept OFF `SchemeResponse`, honouring that seam's own recorded boundary.

## Nit triage (the 5 non-blocking Gate-2 findings)

Full text in `review-nits-ipfs-redirects-3xx-navigation-support-2026-07-27.md`. Triage:

- **Nit 1 is a real doc-accuracy defect, worth fixing.** A matched 3xx on a sub-resource now falls through to `serve_default_404`, so a stale image receives the site's `404.html` bytes at status 404. Both Decision 7 and the `ipfs.rs` doc comment call this "exactly the pre-task behaviour", which is NOT accurate (pre-task, any matched 3xx failed closed hard). The BEHAVIOUR is defensible and low-impact (a 404 page rendered into an `<img>` fails either way); the CLAIM is wrong and should be corrected. Not blocking, not worth its own dispatch: fold into the next touch of this file.
- **Nit 2 (the residual Back bounce) is accepted as designed.** After browsing on from a redirect target, a later Back lands on the source once and bounces forward, then self-heals. Costs one press plus one retrieval. Widening the retention would mean unbounded per-session state, which is exactly what the bounded-store discipline forbids; Decision 8 should just name this residual alongside the first-entry case.
- **Nit 3 is a genuine (if rare) code/comment disagreement**: `pump` clears `last_error` on the marker before `follow_pending_redirect` runs, so a backend that cannot start the redirected load shows the user NO error. Rare, but it is silent failure, which this project otherwise refuses. Worth a follow-up.
- **Nit 4 (Android main-frame race) fails SAFE and is correctly flagged.** `shouldInterceptRequest` runs on a worker thread and can beat `onPageStarted`, so a link click onto a 3xx path may degrade to a not-found. Decision 7 reasons only about the desktop pump cadence while the matrix marks all three implemented. Needs device-time confirmation, and the debug view landing later in this drive is the natural instrument for it.
- **Nit 5 asks for a human nod on four in-scope decisions** (`MAX_REDIRECT_HOPS = 5` vs browsers' ~20; the marker-substring error suppression; removing the public `RedirectNotSupported` variant outright; splitting the capability row). All look right and all are reversible. Flagged for the human, changed nothing.

## Process note (why this task cost 9 dispatches)

Recorded because it is a reusable signal, not a one-off: the first TWO dispatches produced an EMPTY diff and were surfaced by the runner as "the agent produced no change", which reads like a drift/nothing-to-do STOP. It was not. Both agents burned their ENTIRE 16384-token output budget on reasoning alone (`output: 16384, reasoning: 16384` in the session transcript) and emitted no text and no tool call, after ~45 minutes of reading and web research, without ever editing a file. The premise was re-verified true by hand each time. What broke the loop was a requeue note that DECIDED the mechanism for the agent (a 3xx is a navigation, so it travels on a sink the shell drains, NOT on `SchemeResponse`) instead of leaving it to re-derive the fork; the very next dispatch started editing within a few tool calls. Two further dispatches were lost to environment interruptions killing the detached run mid-gate, and one to dorfl's internal deadline checkpoint. Lesson: an "empty diff" bounce deserves a transcript check before it is believed, and a task whose central design fork is genuinely open is cheaper to decide UP FRONT in the task body than to let each agent re-litigate.
