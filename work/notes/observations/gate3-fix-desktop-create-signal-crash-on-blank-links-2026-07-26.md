---
title: "Gate-3 conductor review: fix-desktop-create-signal-crash-on-blank-links (APPROVE) + title/ledger cleanup"
date: 2026-07-26
status: approved
reviewOf: fix-desktop-create-signal-crash-on-blank-links
gate: gate-3-conductor
mergedCommit: bc893be
---

## Verdict: APPROVE

Conductor Gate-3 pass. Gate-1 + Gate-2 passed before merge. This is the fix for the reproducible SIGABRT in the shipped v0.2.5 desktop build (ronan.eth -> portfolio -> a `_blank` link -> crash). I built the webview-renderer locally to confirm the fix compiles and the routing test passes.

## Done-move + landing

- `work/tasks/backlog/fix-desktop-create-signal-crash-on-blank-links.md` -> `done/` on origin/main (feat merged, `bc893be`; the code landed via the kept branch `caf947c` after an API-overload recovery).
- Files (on the branch, now on main): `crates/webview-renderer/src/backend.rs` (the raw `connect_local("create")` handler), `webview-renderer/src/lib.rs` (routing + tests), `docs/adr/0010` + capability matrix + the blank-links spike README (mechanism corrected), a new `docs/spikes/fix-desktop-create-signal-crash-on-blank-links/README.md` (diagnosis + before/after repro), a GTK-init-once observation note.

## The crash is fixed correctly (verified)

The crashing return (`view_for_create.clone().upcast::<gtk4::Widget>()`) is replaced with `Some(None::<gtk4::Widget>.to_value())` - answering WebKitGTK's `create` signal with a real NULL widget via the RAW `connect_local("create", false, ...)` (the typed `connect_create` binding's non-nullable `gtk::Widget` return is what forced the crashing shape; the raw signal lets you answer NULL). The handler reads the NavigationAction's request URI from `args[1]`, applies the shared `new_window_action` rule, on `NavigateInPlace{url}` does `life.begin(&url)` + `view.load_uri(&url)`, and returns NULL so WebKit creates NO new view and applies NO `WindowFeatures` -> no empty-optional deref, no abort. I confirmed on origin/main: the old crashing `.upcast()` shape is GONE, `connect_local("create")` is present, the docs no longer claim "returns the existing view", and `cargo build -p webview-renderer` succeeds.

## The mechanism correction was RIGHT (the agent's STOP was valid)

My original task prescribed `decide-policy`. The build agent STOPPED rather than follow it, correctly: `decide-policy`/`NewWindowAction` fires for a `target="_blank"` link (FrameLoader -> checkNewWindowPolicy) but NOT for `window.open()` (LocalDOMWindow::open -> createWindow -> webkitWebViewCreateNewPage emits `create` directly, bypassing checkNewWindowPolicy) - so decide-policy-only would have SILENTLY REGRESSED `window.open` on desktop, breaking the ADR-0010 parity claim. The spike README documents this with primary-source WebKit internals + a scratch-GTK-harness measurement on WebKitGTK 2.52.3. I requeued with the corrected mechanism (raw create-returns-NULL, covering BOTH triggers through one hook); the delivered code is exactly that. This is the diagnosis-and-STOP discipline working as intended.

## Acceptance criteria (ticked)

- [x] The reproducible SIGABRT is gone: a `_blank`/`window.open` request loads in place, no WindowFeatures abort (guarded by a display-bound `#[ignore]` test that constructs a real WebViewRenderer + the display-free routing unit test `a_new_window_request_navigates_the_existing_view_in_place_no_second_view`, which passes).
- [x] The crashing `connect_create`-returns-existing-view is REMOVED; new-window routed via raw `connect_local("create")` returning NULL (bool/Value return, never a view).
- [x] BOTH `target="_blank"` AND `window.open(url)` load in the current view (both emit `create`; one hook covers both) - the correction over the decide-policy-only regression.
- [x] The in-place load uses the SAME `load_uri` so an `ipfs://`/ENS target is still hash-verified and an unsupported scheme refused; no second window; bar follows the URL. (See nit 5 for the pre-existing validate_url nuance.)
- [x] `docs/adr/0010` + matrix + blank-links README updated to the corrected mechanism (the stale "returns the EXISTING view" text is gone).
- [x] iOS/Android unchanged (native hooks, not `create`).

## Cleanups made in this Gate-3 commit

- Nit 1 (real): the done-record TITLE (and merge subject) still said "route new-window via decide-policy instead" - the SUPERSEDED mechanism, misdescribing what shipped. Corrected the done-record title to "answer the create signal with NULL via raw connect_local instead".
- Nit 2 (real): the task landed carrying `needsAnswers: true` + a live stuck sidecar (`work/questions/task-fix-desktop-create-signal-crash-on-blank-links.md`, 15 duplicated bounce entries) - the runner did not clear them on merge. Cleared both (protocol-mechanical, like a claim revert), same as the android-back task.

## Review-nits triage (Gate-2) - remaining flags

3. The crash's red/green guard is `#[ignore]`d (needs a GTK display; GTK init is once-per-process so it must be run filtered by name; CI never exercises it). RATIFY: a display-bound manually-run guard + the display-free routing unit tests is the accepted coverage for this crash class (a headless CI cannot construct a real WebView).
4. Silent fallthrough: if `args.get(1)` / the NavigationAction downcast ever fails, target is None -> Ignore -> NULL answered with no load and no log (the dead-link behaviour returns invisibly). FLAGGED: worth a debug log / comment naming that failure mode. Minor.
5. The in-place `load_uri` skips the seam's `validate_url` that `Renderer::navigate` applies (refusal of a malformed target relies on WebKitGTK). PRE-EXISTING (unchanged by this fix), but the trust-claim wording reads as if the full navigate path is used. FLAGGED for a future tidy (route the create-handler load through the validated path). Non-blocking.

## Net effect

The v0.2.5 desktop crash on any `_blank`/`window.open` request is fixed - werust answers WebKitGTK's `create` with NULL and loads in place, covering both triggers with verification intact. Warrants a release (v0.2.6). One task remains (`ipfs-web-redirects-and-404-fallback-support`).
