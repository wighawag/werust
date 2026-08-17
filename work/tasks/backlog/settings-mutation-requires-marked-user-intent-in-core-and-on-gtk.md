---
title: "A page must not be able to change werust's settings: the core mutation gate (main-frame AND chrome-marked intent), wired on GTK"
slug: settings-mutation-requires-marked-user-intent-in-core-and-on-gtk
spec: settings-mutations-require-user-intent
blockedBy: []
covers: [1, 2, 3, 4, 5, 7]
---

## What to build

`werust://settings?backend=<kind>&url=<endpoint>` is applied and PERSISTED straight off the request's query string by the internal settings handler, and the `werust` scheme is registered through the same scheme-handler seam that serves page content, on all five edges, for ANY request including sub-resources. There is no notion of a user gesture or a chrome-initiated navigation anywhere in the tree. So one tag in any page silently repoints the user's IPFS retrieval backend:

```html
<img src="werust://settings?backend=custom&url=http://attacker.example/">
```

The retrieval backend is an EGRESS choice: it sees every content-addressed site the user visits. Content stays hash-verified, so this is not a content-integrity break; it is a privacy and availability attack, and it is also how a hostile page would defeat the refusal path the trust work is built on (a dead endpoint makes "refuse the update" fail, leaving accept as the only working option).

Build the core rule, and wire the one edge the acceptance gate can compile:

**The gate is two conditions and BOTH are required.** A mutation is applied only for a MAIN-FRAME request whose navigation was marked as intended by the chrome. Main-frame alone is insufficient (a page can navigate the top-level frame); marked-intent alone is insufficient (a sub-resource request carries no navigation at all).

**A request that fails the gate still RENDERS the settings page, read-only, showing real current values, with the attempted change NOT applied.** The refusal is of the CHANGE, not of the surface: it keeps a blocked attack from looking like a broken browser, and it is what makes the behaviour observable and therefore testable.

**Intent is MARKED by the chrome, never inferred from the request.** Anything derivable from the request (a referrer, a header, the URL shape) is under page control. Two facts in this codebase make the marking cheap and are worth checking before you design anything: the shell's own `navigate` entry point is called by chrome code paths only (an in-page link click, a `window.open` and a `location=` never pass through it, which the redirect sink's documentation states and relies on), and every edge's URL bar commits its raw typed text to that same entry point (the `address-bar` row of `docs/platform-capability-matrix.toml`). The `_blank`/`window.open` hooks are the counter-example that must NOT be treated as intent: each edge routes those into the view's own load path, deliberately bypassing the shell.

**Reads stay unauthenticated.** Rendering the page requires nothing. Only mutation is gated.

Three hazards found while tasking, each of which would produce a wrong-but-compiling fix:

1. **The existing main-frame notion is QUERY-INSENSITIVE.** The redirect sink answers "is this intercepted request the main frame?" by comparing a normalized frame key, and that key strips the query and fragment for every URL. So while the user is legitimately ON the settings page, a sub-resource request for `werust://settings?backend=custom&url=...` reduces to the SAME key and would pass a naive main-frame check. Reuse the main-frame notion (do not mint a second one) but do not let the marked intent be satisfiable by a query-stripped match: the mark must be tighter than the frame key.
2. **The scheme handler runs OFF the main thread on GTK** (`docs/adr/0008`, the off-thread `ipfs://` boundary in `crates/webview-shared`), so whatever carries the intent must be readable from that thread. The redirect sink solves the same problem by being a shared handle; follow that precedent rather than inventing a channel.
3. **The request value on the seam carries only a URI** (`renderer::SchemeRequest`), and it is constructed by every edge, including halves compiled only on macOS, Windows, iOS and Android that the Linux acceptance gate CANNOT compile. If you widen that struct, you break those halves invisibly, and only `.github/workflows/macos-renderer.yml`, `windows-renderer.yml` and `mobile-ios.yml` would find out (there is no Android leg yet: `android-instrumented-ci-leg` creates one). Prefer a core-owned intent carrier the chrome marks and the handler consults; if you widen the seam anyway, say why and name which legs must be run to prove the edges still build.

**The parity matrix row is created ONCE, here.** Add a `settings-mutation-user-intent` capability to `docs/platform-capability-matrix.toml` with a cell for EVERY platform in one edit (the guard forces a cell per platform per capability): `desktop` implemented by this task, and `macos`, `windows`, `ios`, `android` `stubbed` naming their four sibling tasks exactly (`macos-marks-user-intent-for-settings-mutations`, `windows-marks-user-intent-for-settings-mutations`, `ios-marks-user-intent-for-settings-mutations`, `android-marks-user-intent-for-settings-mutations`). A `stubbed` cell whose task slug does not resolve in `work/tasks/{backlog,ready,done}/` reds the gate, so the slugs must match those files.

**State the residual window honestly in the done record.** After this lands, an edge that does not yet mark intent can still RENDER the settings page and (because its URL bar commits through the shell) still apply a change TYPED into the URL bar, but a change submitted from the settings page's own form is refused until that edge's task lands. That is the deliberate fail-closed direction; the four edge tasks close it.

## Acceptance criteria

- [ ] A sub-resource request for `werust://settings?backend=...` does NOT change the persisted settings: asserted with the negative control that matters, the settings file on disk unchanged byte-for-byte after the attempt.
- [ ] A main-frame request carrying NO marked intent does not change the persisted settings, and neither does a marked navigation whose request is a sub-resource.
- [ ] A main-frame request WITH marked intent applies and persists the change exactly as it does today (a user's own settings change is unaffected).
- [ ] Every refused attempt still returns the settings page with the REAL current values and no "saved" confirmation; a refused attempt is distinguishable in the rendered page from a successful one.
- [ ] The marked intent cannot be satisfied by a query-stripped frame-key match (the "user is on the settings page while a sub-resource asks for a mutating URL" case is covered by a test).
- [ ] The intent mark is not derivable from the request, and the `_blank`/`window.open` in-place navigation path does NOT mark it; covered by a test or a shape guard.
- [ ] The GTK edge marks intent for the navigations werust's own chrome starts, and for a submission from the settings page's own form; the wiring is gate-compiled and pinned by a new source-shape guard under `crates/werust-core/tests/` in the existing style of the edge-wiring guards.
- [ ] `docs/platform-capability-matrix.toml` gains the `settings-mutation-user-intent` row with a cell per platform in ONE edit, `desktop` implemented and the other four `stubbed` naming the four sibling task slugs; the parity guard stays green with no weakening.
- [ ] Reading `werust://settings` with no intent still renders the page (reads are not gated).
- [ ] Tests isolate the settings location to a scratch directory (the directory-taking apply/load/save cores, no process-global `WERUST_SETTINGS_DIR` mutation) and assert the real settings file is UNTOUCHED after the run.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- None: can start immediately.

## Prompt

> Goal: make a settings MUTATION require user intent established by the chrome, so a web page cannot repoint werust's IPFS retrieval backend. Read `work/specs/tasked/settings-mutations-require-user-intent.md` for the framing; this task is the core rule plus the GTK edge plus the parity row.
>
> Where things are: the internal settings page and its apply path are `werust_core::retrieval` (`apply_settings_request` and its directory-taking core, the page HTML renderer, and the backend-choice parsing/validation); the `werust` scheme is registered per edge through the same scheme-handler seam as `ipfs://`; the main-frame notion the redirect logic uses lives in `werust_core::ipfs`'s redirect sink and is re-exported by the shell. The seam types are in `crates/renderer`.
>
> The rule: apply a mutation only for a MAIN-FRAME request whose navigation the chrome MARKED as intended. Both halves required. A refused request still serves the page read-only with real current values, which is both the honest behaviour and what makes it testable. Reads are never gated.
>
> Three hazards, verified while tasking, that a naive fix gets wrong:
>
> 1. The frame key the main-frame check uses STRIPS the query, so a sub-resource request for a mutating `werust://settings?...` reduces to the same key as the settings page the user is legitimately viewing. Reuse the main-frame notion (do not mint a second one) but make the intent mark tighter than that key.
> 2. On GTK the scheme handler runs OFF the main thread (`docs/adr/0008`, `crates/webview-shared`), so the intent carrier must be readable from there. The redirect sink already solves this by being a shared handle: follow that precedent.
> 3. `renderer::SchemeRequest` carries only a URI and is constructed by edge halves the Linux gate cannot compile (macOS, Windows, iOS, Android). Widening it is an invisible break whose only detectors are `.github/workflows/macos-renderer.yml`, `windows-renderer.yml` and `mobile-ios.yml`, and Android has NO leg yet. Prefer a core-owned intent carrier; if you widen the seam, justify it and name the legs that must run.
>
> Two facts make the marking cheap: the shell's own `navigate` is a chrome-only entry point (in-page link clicks, `window.open` and `location=` do not pass through it, as the redirect sink's docs state), and every edge's URL bar commits raw typed text through it (the `address-bar` matrix row). The `_blank`/`window.open` hooks route into the view's own load path and MUST NOT mark intent: assert that.
>
> Also do: add the `settings-mutation-user-intent` row to `docs/platform-capability-matrix.toml` in ONE edit with a cell per platform (`desktop` implemented, `macos`/`windows`/`ios`/`android` `stubbed` naming `macos-marks-user-intent-for-settings-mutations`, `windows-marks-user-intent-for-settings-mutations`, `ios-marks-user-intent-for-settings-mutations`, `android-marks-user-intent-for-settings-mutations`); a stubbed cell with an unresolvable slug reds the gate. Create the new per-edge source-shape guard file under `crates/werust-core/tests/` with the GTK block only, in the style of the existing edge-wiring guards: the four sibling tasks will APPEND their own block to it, so leave it structured for appending.
>
> State in the done record what the residual window is: until an edge marks intent, its settings page's own form submission stops applying (a URL-bar-typed change still works, because that path goes through the shell). That is the deliberate fail-closed direction.
>
> Out of scope: new settings, a new settings surface, gating READS, any other page-reachable internal surface, and the retrieval default itself (`retrieval-default-egress-before-final-release`, an unpromoted backlog task that edits the same settings page and module: if it lands first, rebase rather than reverting its wording).
>
> Two more files you share: `crates/werust-core/src/lib.rs` is this repo's worst collision point (8000+ lines) and `trust-store-fails-closed-instead-of-reading-as-nothing-trusted` is deliberately blocked on YOU so the two writers are serialised. You own the navigation/intent region; that task owns the pin-store and bless region. RECORD the intent mechanism as an ADR: it is a security boundary, hard to reverse, and surprising without context.
