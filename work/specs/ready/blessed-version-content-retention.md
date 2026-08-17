---
title: "RETAIN trusted content locally, under its own bound, so refusing an update has somewhere to land"
slug: blessed-version-content-retention
taskedAfter: [settings-location-on-every-edge, withhold-changed-content-until-trusted]
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/ready/` tasks. (The technical-detail sections below are trimmed by `to-task` once the work is tasked — they move into tasks/ADRs and this spec settles to its durable framing: Problem / Solution / User Stories / Out of Scope.)

> **Vocabulary.** This spec says **RETAIN / RETENTION** for keeping content blocks locally, never "pin". `crates/werust-core/src/pins.rs` already reserves *pin* for two other senses — "held in place" (`url_override`, `pinned_root_key`) and the durable trusted-name record whose verb is **bless, never pin** — and its vocabulary note exists precisely so those senses cannot be confused at a call site. Local content retention is a THIRD thing and gets a THIRD word. (The IPFS ecosystem calls this "pinning"; werust deliberately does not — decision 8 records the glossary entry that keeps it that way.)

## Problem Statement

werust has **no persistent local content store**. Every load fetches blocks from the configured retrieval backend; the only store in the tree is a private test fixture implementing `ContentSource` in `crates/fetcher`, and a cross-load block cache was explicitly deferred by the per-resource CAR-scope spike. Nothing survives a restart.

That leaves `withhold-changed-content-until-trusted`'s refusal path unable to do its job:

1. **A refusal usually has nowhere to land, exactly when it matters.** Refusing an update loads the previously trusted CID instead. But a site owner typically stops pinning the old version *when they publish the new one*, so the old CID disappearing is **correlated with** the repoint that triggered the refusal. The failure is not a rare edge case; it is the common case. "I do not trust this update" then degrades into "you get nothing", pressuring the user into accepting an update they distrusted.

2. **A trusted site is only as available as a stranger's retention policy.** werust's thesis is content-addressed trust, yet a user who has said "I trust THIS version" cannot open it offline, or once the gateway drops it. The bytes are hash-verified and immutable; there is no reason to re-fetch them from a third party on every visit.

## Solution

### Two records, two bounds — this is the load-bearing decision

The trust RECORD and the retained CONTENT are different sizes and get different bounds:

- **The trust record** (`pins.json`: name, CID, date, posture) is a few hundred bytes. It is **always kept and never evicted**. This is what DETECTS a change.
- **The retained content** (blocks) is megabytes per site. It is **bounded by a BYTE BUDGET**, evicted least-recently-used by name. This is what SERVES a refusal. A count of names could not bound disk at all: retention captures what the user browsed, so one site can be gigabytes.

The consequence is the point: **detection never degrades.** The modal always fires and the user always gets the choice, however small the content budget is. Only the ability to SERVE the older bytes locally can degrade — and when it does, werust re-fetches, and if that fails it says so plainly rather than quietly loading the version the user just rejected.

This is what makes retaining-on-every-first-use affordable. Retention must follow the automatic first-use record, not an explicit accept: the FIRST repoint of a name is the most important refusal there is, and at that moment the user has never explicitly accepted anything, so a retention set keyed on explicit accepts would be empty precisely when first needed. Keying on the auto-record instead means "every content-addressed site ever visited" becomes retention-eligible — which is why the name-level LRU exists, and why the content bound cannot be the trust record's bound.

### The store

A block store on disk under the settings directory (beside `pins.json` and `retrieval.json`, resolved through the explicit settings-location seam established by `settings-location-on-every-edge`), holding hash-verified blocks keyed by CID.

It is a NEW **block-level seam inside the CAR retriever**, consulted before the network fetch and written by the CAR walk as blocks verify. That placement is a decision, not an implementation detail, and the alternative was rejected for a specific reason:

- **Block-level (chosen).** Blocks are stored keyed by their OWN CID, so every block re-hashes against its CID on the way out and verification parity is STRUCTURAL. Cost: this is surgery inside the retriever, where the block store is currently a private per-call struct — it is a new seam at a new granularity, NOT a decorator behind an existing one.
- **Resource-level (rejected).** A decorator around the existing `ContentRetriever` would be cheap and file-orthogonal, but that seam's unit is a REASSEMBLED resource, which does not hash to the CID it was requested under. Such a store could not re-verify what it serves, so a tampered store would render unverified bytes.

The rejection is load-bearing because the withhold makes a trusted CID CHOOSE what loads on a refusal, so the store becomes a tamper target for the first time. A store that cannot re-verify would break the one invariant werust rests on. Note also that `ContentAddressedFetcher` / `VerifyingContentFetcher` — the block-granular types that already exist — have NO production consumers today; they are exercised only by their own crate's tests, so they are a precedent for the shape, not a seam already on the load path.

### What retention actually captures

Retrieval is **per-resource** (`dag-scope=entity`), deliberately: whole-DAG fetching was removed to fix a field bug. So a completed load fetches the entities the page requested, NOT the reachable DAG. Retention therefore captures **what the load fetched**, and a refusal serves those blocks locally and may still need the network for a sub-resource the user never reached (a lazily-loaded image, a page behind a link). Truly offline-complete retention needs a scoped whole-DAG fetch, which is exactly the traffic the per-resource narrowing removed, and is a recorded follow-on rather than a silent assumption here.

## User Stories

1. As a user who refused an update, I want the version I trusted to be served from my own machine, so that refusing works even though the site owner stopped hosting it the moment they published the new one.
2. As a user who refused an update whose retained blocks are incomplete, I want werust to re-fetch what it lacks and tell me plainly if it cannot, so that I am never quietly given the version I rejected.
3. As a user revisiting a page whose blocks are retained, I want it to open with no network, so that trusting a version means having the part of it I actually used.
4. As a security-conscious user, I want blocks served from local storage hash-verified exactly like network blocks, so that tampering with my store cannot render unverified bytes now that a stored CID can choose what loads.
5. As a user with finite disk, I want retained content bounded independently of my trust records, so that being protected on many sites does not mean storing all of them.
6. As a user who keeps being protected on sites I no longer visit, I want their retained content released before my recent ones, so that the budget follows what I actually use.
7. As a user with a disk budget in mind, I want to set how much space retained content may use, so that protection on many sites does not mean storing all of them without limit.
8. As a user considering raising the budget, I want to see what the store is currently using, so that the knob is not a blind commitment.
9. As a developer running the test suite, I want the store isolated to a scratch directory, so that tests never read or write my real trusted content.

## Implementation Decisions

1. **Two bounds, stated once.** The trust record is unbounded-but-tiny and never evicted; retained content is bounded by a byte budget. Detection is therefore never a function of the content budget. The budget is settable through the surface that already carries the retrieval-backend choice, which is safe to use for a security-relevant knob only because `settings-mutations-require-user-intent` closes the page-reachable mutation hole first.

2. **Retention follows the automatic first-use record, not an explicit accept.** The first repoint is the refusal that matters most and there is no explicit accept before it. The name-level LRU is what makes this affordable.

3. **Local-first, network-fallback, always verified.** The store is consulted before the network and its blocks are hash-checked on the way out. A local block that fails verification is discarded and re-fetched, never served.

4. **Retention captures what the load fetched, not the reachable DAG.** Retrieval is per-resource by design. Claiming whole-DAG retention would either be false or would silently reinstate the traffic the per-resource narrowing removed. A refusal serves what was retained and re-fetches the rest.

5. **Eviction is per NAME, but blocks are SHARED, so release needs reference counting.** Under pressure the least-recently-used name's retained content is released. Blocks are content-addressed and therefore shared across names and versions — the retriever already relies on that dedup — so releasing a name's blocks as a flat set would delete blocks another retained name still needs, producing exactly the half-a-version state this decision forbids. Release must therefore be refcounted (or driven by a reverse index), and the retriever, which sees only CIDs, must be told which name a load belongs to. That coupling changes the seam's signature, so it is a decision rather than an implementation detail.

6. **Releasing content never touches the trust record.** A name whose blocks were evicted is still watched, still fires the modal, and still refuses correctly — it just re-fetches. This is the property that lets the content budget be small without weakening the security guarantee.

7. **Exactly ONE trusted version per name is retained; the byte budget is the user-facing knob.** Keeping more than one only pays off with a surface for choosing an older one, which is out of scope, so a multi-version format would add a migration and an eviction rule to serve a feature nothing can reach. The trust record's shape is therefore UNCHANGED by this spec, and multi-version retention plus its format, its within-name eviction and its selection UI are one follow-on taken together.

8. **The glossary RECORDS the word.** `CONTEXT.md` gains the RETENTION term beside the existing pin senses, so the next author cannot re-fork "pin" into a third meaning.

9. **No trust-file migration happens here.** The record shape is unchanged (decision 7), so this spec adds no migration on top of the location move (`settings-location-on-every-edge`) and the key re-keying (`trust-store-hardening`). Deliberate: three concurrent migrations of one small file was an avoidable hazard.

10. **Size reporting is programmatic here.** The core can report how much the retained set is using against its budget; presenting it in a management UI is a follow-on. The story is satisfied by the figure being available and correct, not by a screen. There is deliberately no retained-versus-cache split to report: everything in the store is inside one accounted total (decision 4), and no separate cache exists.

## Testing Decisions

- **Verification parity** is proven with a tampered local store as the negative control, mirroring the existing verifying-fetcher tests: a local block that does not hash to its CID is discarded, never served.
- **Refusal-from-local** is tested end to end with the network backend unavailable: a refused update still lands on the retained version.
- **Incomplete retention** is tested by retaining a partial set and asserting the refusal re-fetches the remainder, and fails honestly when it cannot.
- **Eviction** is tested by exceeding the budget and asserting the least-recently-used NAME is released whole, that a retained version is never left partial, and that the evicted name's TRUST RECORD survives and still fires the modal.
- **Isolation:** every test drives a real store in a scratch directory via the settings-directory mechanism and asserts the developer's real store is untouched (the shared-write rule).

## Out of Scope

- **Whole-DAG retention / true offline-complete sites.** Needs a scoped `dag-scope=all`-shaped fetch, which is the traffic the per-resource spike deliberately removed. Recorded as a follow-on with that cost named.
- **Serving content to other peers.** This is a local store, not a node.
- **Keeping more than one version per name, and any surface for selecting one.** `N` is fixed at 1 here (decision 7); multi-version retention plus its ordered-list format, its within-name eviction and its selection UI are one follow-on, taken together so the format is not migrated for a feature nothing can reach.
- **A storage management UI** (sizes on screen, manual release). Decision 10 keeps the figure programmatic.
- **Changing the default retrieval backend** — `retrieval-default-egress-before-final-release`. This spec reduces how often the network is consulted; it does not change which backend is used.
