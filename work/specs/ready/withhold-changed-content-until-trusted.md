---
title: "A trusted name that now points to different content is WITHHELD until the user decides"
slug: withhold-changed-content-until-trusted
taskedAfter: [trust-store-hardening, settings-location-on-every-edge, settings-mutations-require-user-intent]
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/ready/` tasks.

## Problem Statement

werust's trust-on-first-use for mutable names (ENS, IPNS) is **advisory, not blocking**. When a name the user previously trusted resolves to a *different* CID, the new content loads and renders immediately — scripts execute, storage is reachable — and only afterwards does a banner say it changed. A compromised name key or a rogue owner runs arbitrary code before the user has accepted anything. SSH refuses the connection when a host key changes; werust connects first and mentions it after.

The trust check happens on the pump, in `refresh_chrome`, i.e. after commit. The CID is available far earlier: `navigate_ens_name` resolves it synchronously, before `renderer.navigate()` is ever called.

**Protection is also opt-in, so almost nobody has it.** A name is only watched if the user found the trust surface and acted. Most never do, so the store stays empty and a repoint is never detected at all.

## Solution

### The content is withheld

1. Resolve the name to its CID (unchanged).
2. Consult the trust record — **re-read from disk**, not the launch-time cache (decision 2), keyed on the **normalized** name (decision 3).
3. Trusted, CID matches → proceed.
4. Trusted, CID **differs** → **do not navigate**. Raise a hold carrying the whole resolved target, and show a modal. The backend never starts loading, so nothing is fetched, parsed or executed.
5. No record → proceed; the record is written when the load SUCCEEDS (decision 4).

**Accept:** navigate the **held** target, and record on THAT load's success. The previous record survives until the new version has demonstrably loaded (decision 5) — otherwise accepting an unfetchable CID would destroy the only fallback the user has.

**Refuse:** load the previously trusted CID. Served from local retention where available, else re-fetched; if neither works the refusal fails honestly rather than falling through to the new content.

### Trust REFLOWS; there is no CID blocklist

A trusted page is already executing, so it navigating elsewhere is not an escalation — it could achieve the same by any other means. What must hold is that **every top-level navigation is evaluated on its own terms and never inherits a name's trust.** A page that navigates to a bare `ipfs://<cid>` gets the bare-CID posture and the CID in the URL bar, not the name's trust and not the name in the bar. So a page-initiated move to the withheld CID is permitted and honestly described, rather than blocked by a session blocklist.

This is only sound while the ORIGIN is the CID, which is why decision 10 makes this spec a prerequisite for `explore-name-as-origin`: once the origin is the name, a different CID under that name would share storage, and the reflow requirement becomes load-bearing rather than cosmetic.

### First use is recorded on a successful load, not prompted

The first successful load of a mutable name records its CID automatically, with no prompt — the SSH model. Deliberately not a first-visit dialog: a prompt on every new mutable-name site trains people to dismiss it, spending the one scarce interruption on the least informative event so the modal that matters gets clicked through too.

**The trade-off, in full.** Auto-recording trusts whatever version the user first reaches, even if already malicious — inherent to trust-on-first-use. Worse here: the recorded CID is whatever the Phase-1 trusted RPC answered, so a single hostile answer permanently poisons the record and the LEGITIMATE site then trips the modal forever, inverting the protection into a denial of service against the real owner. Because of that, a **forget-this-name** action is in scope (story 11) rather than deferred — without it the poisoned state is unrecoverable from inside werust, and on mobile the user cannot reach the file at all.

## User Stories

1. As a user whose trusted site was repointed, I want the new content WITHHELD until I decide, so that unknown code cannot execute or reach my data before I accept it.
2. As a user facing a changed name, I want a modal naming the name, what I trusted and when, and what it points to now, so that I can decide without hunting for a control.
3. As a user running two windows, I want the check to see a decision made in the other one, so that a stale snapshot cannot let changed content through unchallenged.
4. As a user who accepts an update, I want exactly the version the modal showed me to be the one loaded and recorded, so that a repoint while the modal was open cannot substitute another.
5. As a user who accepts an update that then fails to load, I want my previous trusted version still recorded, so that accepting a broken update does not destroy my only fallback.
6. As a user who refuses an update, I want to land on the version I trusted, so that refusing is safe rather than a dead end.
7. As a user who refuses an update that cannot be produced, I want to be told plainly, so that I am never silently given the content I rejected.
8. As a user whose first visit FAILED to load, I want nothing recorded, so that a version I never saw does not become the one I am held to.
9. As a user who never opens the trust surface, I want my first successful visit recorded automatically, so that I am protected from a later repoint without opting in.
10. As a user whose record was poisoned by a bad resolution, I want to forget a name's trust from the modal I am already looking at, so that the mistake is recoverable without editing files I cannot reach on a phone.
11. As a user browsing a version I kept after refusing, I want the chrome to say I am on a SUPERSEDED version, so that I do not mistake it for what the name points to now.
12. As a user who followed a link off a page, I want any pending decision and any superseded notice to clear, so that a stale warning is never painted over an unrelated page.
13. As a user revisiting an unchanged trusted site, I want no interruption, so that the modal stays rare enough to mean something.
14. As a user on any of the five edges, I want the same decision surface, so that the protection is not desktop-only.
15. As a macOS or Windows user, I want the trust surface my platform has never had — a readable explanation of why werust trusts this page — so that the decision modal is not the only thing that can ever tell me about trust.
16. As a user browsing content a name no longer points to, I want the chrome to say so however I got there — refusing, history, or a direct link — so that a superseded version is never described as current.
17. As a user reading a decision modal, I want the page underneath unable to dismiss it, so that a hostile page cannot make a changed name unreachable or race my answer.
18. As a user on Android, I want a load that FAILED never recorded as trusted, so that the edge that reports failures as successes does not poison my trust record.

## Implementation Decisions

1. **The decision is in the core, before `renderer.navigate()`.** The edge renders the modal from chrome state and reports the answer. This is also the only workable placement: iOS's `decidePolicyFor` always allows, so there is no platform veto point.

2. **BOTH the check and the record re-read the store.** The in-memory snapshot is documented as a cache, not the truth, because another window may be writing the same file; it is refreshed only inside the bless path today. A check against that cache is the primary bypass: a record made elsewhere is invisible, so no modal fires. One small file read on a path that already performs RPC calls. A live backlog item, `pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns`, already prescribes the navigation-time re-read; this spec supersedes and absorbs it, and that disposition is recorded rather than left to whoever hits it first.

3. **Store semantics are INHERITED, not restated.** The normalized key, the fail-closed unreadable state, atomic writes, unknown-field preservation, write serialisation and canonical CID comparison are all delivered and tested by `trust-store-hardening`, which is why this spec is `taskedAfter` it. This spec consumes those guarantees and must not re-specify or weaken them: in particular, it may only record through the store's serialised insert-if-absent path, and it must treat the unreadable state as a decision input rather than as an empty store.

4. **The record is written when the load SUCCEEDS, and Android must first report failure honestly.** Recording at resolution time would stamp a CID that then failed to fetch. This depends on "success" being honest per edge, and on Android it is not: a fail-closed resolution is answered as a 502 intercepted response and there is no `onReceivedHttpError` override anywhere in the tree, so the core sees a finished page load. Story 17 owns that wiring, and the auto-record must not ship on Android before it — otherwise the edge records failures as trusted versions.

5. **Accept navigates the HELD target and records on that load's success.** The hold carries the full resolved target (CID, path, name, mutability), not just a CID: re-resolving would let a repoint during the modal substitute another version, and mutability is not derivable from a CID. The recorded posture comes from the accepted load, not the previous page's stale posture.

6. **Auto-record is INSERT-IF-ABSENT against the re-read store; replace stays behind an explicit accept.** The existing store replaces unconditionally, so an automatic record evaluated against a stale snapshot could overwrite the user's real decision, unattended. If the re-read reveals a DIFFERENT recorded CID at that point, content has already loaded and the honest response is to raise the change surface then rather than record silently — a case decision 2's re-read makes rare but does not eliminate.

7. **The store fails CLOSED and writes atomically.** Today an unreadable or unparseable store degrades to "no records", and individual malformed entries are dropped and then persisted. That was correct while the record was advisory; now it means one truncated file re-records every name with no modal. Unreadable must be a distinguishable third state (cannot determine trust) rather than nothing-trusted, unknown fields must survive a rewrite, and the save must be temp-file-plus-rename — writes now happen on every first successful load, so the exposure window scales with use.

8. **The superseded state is derived from what the name resolves to NOW, not from the refusal path.** After a refusal the loaded entry IS the trusted CID, so the record-based derivation reports trusted-and-unchanged and the chrome affirmatively claims "you trusted exactly this content" for a version the name has moved on from. Deriving it only from the refuse button is not enough either: the same state is reachable by a history move, a direct link, or an in-site link click, because the name is re-derived for any path under a root CID resolved earlier this session. So the state compares the loaded root CID against the name's CURRENT resolution and is correct however the user arrived. It must also stop the posture axis asserting the name binding — and because that binding is re-marked on EVERY pump from the session's name table, the suppression has to be consulted inside the chrome refresh, not applied once at load time.

9. **The hold clears only on a SHELL-INITIATED transition; the superseded state clears when the root CID changes.** These have different lifetimes and must not share a rule. The withheld page's predecessor stays loaded and executing under the modal, and in-page navigation is detected from lifecycle events that live page can emit at will — so clearing the hold on any observed transition lets the outgoing page dismiss the modal repeatedly, or tear the hold down between the user's click and the answer arriving. The hold therefore clears on transitions the shell started (navigate, reload, history, the failure paths, the redirect follow) and on a genuine top-level commit of a document other than the held one, never on an event the outgoing document emitted. The superseded state is per-SITE, not per-entry, so it survives an in-site link click and clears when the root CID changes or the user leaves the site. The withhold path is also a non-navigating exit and must clear the pinned resolution step, or the chrome sticks in a permanent spinner and Reload becomes Stop while the modal is up.

10. **The macOS and Windows per-edge work builds a trust surface that does not exist there.** Those two edges have no clickable trust indicator at all — a plain label with no click target — so their modal task is also the first modal host on that platform, and story 15's read-only explanation lands with it. This is why the per-edge work is not uniform across five edges, and it must be sized accordingly rather than treated as a fifth copy of the GTK change.

11. **This spec creates the decision modal's capability row, once, with a cell per edge.** The row is minted with every cell pending and each per-edge task flips exactly its own, so five tasks do not collide in one file and the parity guard stays green at each step. The row cannot be created by the cleanup spec that follows: by then these per-edge tasks are finished and could not flip anything. This spec also corrects the existing change-warning row, whose description still says the warning is "never a hard block" and that the record "never steers a load" — both of which this spec inverts.

12. **This assumes the CID-scoped origin and is a PREREQUISITE for changing it.** Load-start paths other than the checked one are safe today only because they are CID-addressed: the three core navigate sites, the scheme handler serving subresources, and **five** platform new-window hooks — the desktop `create` signal, macOS `createWebViewWithConfiguration:`, Windows `add_NewWindowRequested`, Android's `loadUrl`, iOS's `wv.load`. (A stale doc comment in the seam says three; it is wrong.) `explore-name-as-origin` must re-audit that set of five, and this spec must land first.

13. **The record stops being advisory, which inverts documented invariants in two places.** `CONTEXT.md` and the module doc both state the pin "never chooses what to load". The refusal path makes it choose. This spec owns updating the glossary entry, the module's fail-safe note (which also says the record "never authorises a load"), `docs/adr/0006`'s follow-on note, and the stale seam comment that claims three OS edges have new-window hooks when there are five.

## Testing Decisions

- **The withhold** is core logic testable with no display, using a fake renderer that COUNTS navigate calls — proving nothing was asked to load is the checkable form of story 1.
- **Two-window staleness** (story 3) is tested by mutating the store on disk behind a live session and asserting the check sees it.
- **Accept** (stories 4, 5) is tested by repointing between raise and answer, and by an accept whose load fails, asserting the previous record survives.
- **Clearing** is tested per shell-initiated transition, by asserting a pump carrying the OUTGOING page's own url-change event does NOT wipe a live hold, and by asserting the superseded state survives an in-site link click and clears when the root CID changes.
- **iOS ships this protection on a build-verify plus WebKit port-equivalence argument**, with the residual risk named: nothing in this repo drives the iOS app, so behaviour there rests on a Swift source-shape guard plus the build leg. Stated as an accepted limit rather than left implicit.
- **Per-edge modals and the macOS/Windows surface** name their legs: GTK is gate-compiled; macOS names `macos-renderer.yml`; Windows names `windows-renderer.yml`; Android names `android-instrumented-ci-leg`; iOS is build-verified on `mobile-ios.yml`.
- **Isolation:** the store is supplied through the settings-location seam, never env, and the real store is asserted untouched.

## Out of Scope

- **Store semantics** — `trust-store-hardening`, which lands first and which this spec consumes rather than restates.
- **Gating settings mutations on user intent** — `settings-mutations-require-user-intent`, a prerequisite because a page that can repoint the retrieval backend can guarantee the refusal path fails and coerce an accept.
- **Retiring the old bless affordance** — `retire-the-bless-affordance`, which is `taskedAfter` this spec and needs an expand/migrate/contract shape of its own.
- **Local retention of trusted content** — `blessed-version-content-retention`.
- **Making the NAME the origin** — `explore-name-as-origin`, which this precedes.
- **Knowing WHEN a name last changed** (a freshness notice, or "changed \<when\>" in the modal). Needs an ENS indexer or archive-node access, with its own trust and privacy cost; the modal's detail is kept structured so the field can be added later.
- **A re-offer cadence after a refusal.** A refusing user stays on the older version until they reload. Recorded as a known consequence.
- **A full trust-management screen.** Story 11's forget action is deliberately narrow: the name in front of the user, from the modal already on screen.
