---
title: "EXPLORATION: make the NAME the web origin (ronan.eth, not ipfs://<cid>) — measure what each edge's engine actually permits before committing"
slug: explore-name-as-origin
taskedAfter: [android-instrumented-ci-leg]
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/ready/` tasks. (The technical-detail sections below are trimmed by `to-task` once the work is tasked — they move into tasks/ADRs and this spec settles to its durable framing: Problem / Solution / User Stories / Out of Scope.)

## Problem Statement

### Why this is an EXPLORATION spec, not a build spec

The DESTINATION is decided and is not in question: **`ronan.eth` should be the web origin**, because that is the identity the user sees in the URL bar, the identity they made a trust decision about, and the identity they expect their data to belong to. A user who stores something on `ronan.eth` expects it to still be there tomorrow, not to be wiped because the site published a new version.

What is NOT known is whether the platforms permit it, and at what cost. This repo has already MEASURED that engine capabilities on custom-scheme origins are **scheme-gated** rather than derived from the origin being well-formed:

> On a REGISTERED `ipfs://` origin with `HasAuthorityComponent` + `TreatAsSecure` — a real, secure tuple origin where `fetch` and `pushState` both work — Blink still rejects `navigator.serviceWorker.register('/sw.js')` with `InvalidStateError`.
> (`docs/spikes/windows-ipfs-origin-probe-on-ci/probe-report-2026-07-30.json`, WebView2 150.0.4078.65; `work/notes/observations/service-worker-registration-differs-by-ipfs-serving-origin-2026-07-30.md`)

The same repo also has a MEASURED counter-example of the "the engine surely does the right thing" assumption failing: Android's WebView returned `null` for `window.localStorage` on the `ipfs://` origin until a task explicitly enabled DOM storage — while the same code worked on GTK.

So a build spec written now would be fiction on five different engines. Changing the origin scheme could silently **lose** capabilities that `ipfs://` currently has, and the loss would be per-edge and invisible until a user hit it. This spec's done is therefore **confidence plus a de-risked build plan**, not a shipped capability. The build is a follow-on spec this exploration writes the plan for.

## Solution

### The safety constraint (load-bearing, not a preference)

**Name-as-origin must NOT SHIP before `withhold-changed-content-until-trusted`.** That ordering is a security property. Note it is deliberately NOT expressed as `taskedAfter` on this spec: `taskedAfter` gates TASKING so a spec's tasks can reference another's slugs, and this exploration ships no capability and references no task of that spec — serialising the long-pole measurement behind an unrelated build would cost time without enforcing anything. The constraint binds the follow-on BUILD spec that the final story emits, which must carry it (see the ordering note in this section). The reasoning:

- **Today (CID-scoped):** every version of a name is a separate origin, so a repointed name gets FRESH storage and cannot read the previous version's data. Version isolation is accidental, but it is real.
- **After name-scoping:** all versions of `ronan.eth` share one origin, so a repointed name inherits the previous version's `localStorage`, IndexedDB, cookies and any granted permissions.

Shipping name-scoping without the TOFU gate would therefore CREATE a data-theft path that does not exist today: a name whose key is compromised could serve content that reads everything the trusted version stored. The gate is what makes name-scoping safe, so the gate lands first. The constraint is therefore recorded HERE and restated by the build spec story 9 emits; it is not expressed as `taskedAfter` on this spec.

### What must be answered

1. **What origin shape do we adopt?** The candidates, to be evaluated rather than assumed:
   - A name-bearing scheme, e.g. `ens://ronan.eth/path`, where the scheme handler resolves the name to a CID internally and the origin tuple is `(ens, ronan.eth)`.
   - Keeping `ipfs://` for direct-CID loads (unchanged, CID IS the identity there) and introducing the name scheme ONLY for name-resolved loads. The user's stated model: `ipfs://<hash>` typed directly keeps the hash as origin; a name keeps the name.
   - Any approach that maps a stable origin onto the existing `ipfs://<cid>` load without a new scheme, if one exists per engine.

2. **What does each engine PERMIT on that origin?** Per edge (GTK/WebKitGTK, macOS/WKWebView, Windows/WebView2, Android WebView, iOS/WKWebView), measure: `localStorage`, IndexedDB, cookies, service workers, `fetch`, `pushState`, secure-context status, and the EIP-1193 provider injection. The question is not "is the origin well-formed" (the ipfs:// finding proves that is not what decides it) but "what does this specific engine gate on this specific scheme".

3. **What is LOST relative to `ipfs://` today?** A capability that currently works on `ipfs://` and stops working on the new scheme is a regression that must be known BEFORE committing, not discovered by a user. This is the single most important output.

4. **How does the load path change, and where does the TOFU gate check move?** Today `navigate_ens_name` resolves the name to a CID and then navigates the backend to `ipfs://<cid>`. With a name origin, the backend would navigate to the name and resolution would move INTO the scheme handler. The gate's invariant ("verify the pin before any content renders") must survive that move; the exploration must say exactly where the check lands.

5. **What happens to existing user data?** Any user who has stored data under the current CID-scoped origins loses access when the origin changes. Is that acceptable (werust is pre-1.0 and this is a small user base), or is a migration needed? Decide explicitly rather than by omission.

6. **Do the mobile edges permit it at all?** Android's `shouldInterceptRequest` does not change the document's origin, and WKWebView's custom-scheme handling has its own constraints. If a name origin is not achievable on a mobile edge, the outcome may be per-edge divergence — which must be a recorded decision, not a surprise.

### The route: use the probes that already exist

The route reaches THREE edges directly, and the spec must not promise five it cannot measure. Desktop is ADDED as a first-class measurement (story 3) because it is the edge werust actually ships and daily-drives, and it is the CHEAPEST to measure: the Linux gate already runs on every commit, so a WebKitGTK origin probe needs no new runner. iOS is the one edge left to ARGUMENT rather than measurement (story 4), by WebKit port-equivalence with macOS, with the residual risk named — the same treatment `matrix-web-platform-rows-are-measured-on-every-edge` already applies to it. Three measured by extending existing probes (macOS, Windows, GTK), one measured by a DIFFERENT experiment (Android — see below), one argued (iOS). None assumed.

**Android needs its own question, not the same one.** Android does NOT serve `ipfs://` as the document origin at all: it maps every load to an internal `https://<cid>.ipfs.werust.invalid` host, because an intercepted `ipfs://` document gets an OPAQUE origin in the System WebView. So "register a second, name-shaped scheme in the existing probe" does not apply — there is no scheme registration API on that edge. Android's question is the cheaper and arguably more promising one: does swapping the mapped host from `<cid>` to `<name>` change what Blink permits? It also needs the `android-instrumented-ci-leg` leg to be measurable at all.

For the three that already have probes, this is much cheaper than it looks. `crates/macos-origin-probe` and `crates/windows-origin-probe` ALREADY run on real `macos-14` and `windows-latest` runners, ALREADY load a real `ipfs://` origin served by the platform's scheme handler, and ALREADY measure per-capability facts on it (`service_worker` is literally a field of `CaseFacts`). Android has an instrumented on-device probe from `android-enable-dom-storage-and-guard-web-platform-parity`.

So the measurement work is mostly **registering a second, name-shaped scheme in the existing probes and reading the same facts back on it**, then comparing the two columns. No new harness, no new hardware, no human at a Mac.

This also composes with `matrix-web-platform-rows-are-measured-on-every-edge` (backlog), which already intends to give the web-platform rows equal evidence across edges. The two should share a measurement route rather than inventing two.

## User Stories

1. As werust's architect, I want the candidate origin shapes written down with the exact authority form each produces, so that the choice is made on a comparison rather than a hunch.
2. As werust's architect, I want the existing macOS and Windows origin probes extended to measure a name-shaped origin alongside `ipfs://`, so that the decision rests on real runner evidence rather than on assumed engine behaviour.
3. As a desktop user on the edge werust actually ships, I want the GTK/WebKitGTK origin measured in CI, so that a regression on the primary edge is caught before the change rather than in the field. (Note this needs a NEW harness: there is no GTK probe crate, and `verify.yml` has no display server, so whether a GUI run belongs inside the acceptance gate or in its own leg is an open question this story must answer before it can be built.)
4. As werust's architect, I want Android measured by the host-mapping experiment rather than the scheme experiment, so that the measurement matches how that edge actually serves content.
5. As werust's architect, I want iOS answered by an explicit WebKit port-equivalence argument with its residual risk named, so that the one unmeasured edge is a recorded judgement rather than a silent gap.
6. As werust's architect, I want a per-edge capability DELTA that explicitly flags every REGRESSION against today's `ipfs://` behaviour, so that a capability we would silently lose is known before the change, not discovered by a user.
7. As werust's architect, I want the load-path change written down — where name resolution moves to, and exactly where the trust check lands so nothing renders before it runs — so that the withholding guarantee survives the rearrangement.
8. As werust's architect, I want every load-start site re-audited against the new origin, so that paths which are safe today only because they are CID-addressed are not silently unsafe once the name is the origin.
9. As an existing user, I want the fate of data stored under today's CID-scoped origins decided explicitly, so that it is migrated or abandoned deliberately rather than by omission.
10. As werust's architect, I want a build plan that names the chosen shape, the per-edge work, the known regressions and the ship-ordering constraint, so that the follow-on build spec is buildable rather than fiction.

## Out of Scope

- **Building the origin change.** This spec produces confidence and a plan. The capability build is the follow-on spec the final story emits.
- **Choosing between candidate schemes up front.** That is an OUTPUT of the measurement, not an input.
- **Changing `ipfs://<cid>` direct loads.** A CID typed directly keeps the CID as its origin; there is no name to scope to. Only name-resolved loads are in question.
- **A CI leg for Android** — `android-instrumented-ci-leg`, which this spec's Android measurement depends on.
- **The withholding change itself** — `withhold-changed-content-until-trusted`. Note the constraint is on SHIPPING name-as-origin, not on running this exploration: the measurement may proceed in parallel, and the build spec this exploration emits is what must not ship before the withholding does.
- **Service-worker support as a goal.** Whether service workers work on any werust origin is a separate question; here it is only one MEASURED FACT in the delta table.