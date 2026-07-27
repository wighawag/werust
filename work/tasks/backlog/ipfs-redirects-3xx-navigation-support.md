---
title: "Support 3xx redirect rules in the IPFS _redirects file (301/302/303/307/308) — navigate to the target, updating the bar"
slug: ipfs-redirects-3xx-navigation-support
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
needsAnswers: true
---

## What to build

Follow-on from `ipfs-web-redirects-and-404-fallback-support` (its DECISIONS.md Decision 3): the `_redirects` parser already PARSES 3xx rules (301/302/303/307/308) but a MATCHING 3xx rule currently FAILS the load with a `RedirectNotSupported` reason rather than performing the redirect. This task delivers the deferred 3xx NAVIGATION: a matching 3xx rule should NAVIGATE to the rule's `to` target (updating the URL bar), the browser-idiomatic redirect behaviour, instead of erroring.

READ-FIRST / drift check: confirm `crates/werust-core/src/redirects.rs` still parses 3xx into a rule but the apply path (in `crates/werust-core/src/ipfs.rs`) returns `RedirectNotSupported` for a matching 3xx (Decision 3). Confirm the 200-rewrite / 404-custom-page / off-root-reject / same-root-confinement behaviour from the parent task is intact and build on it.

Fix: on a matching 3xx rule, resolve the `to` (with placeholder/`:splat` injection, confined to the SAME root CID as the parent task requires) and NAVIGATE the shell to `ipfs://<rootcid>/<to>` (a real navigation that updates the bar + history), distinguishing 301/308 (permanent) vs 302/303/307 (temporary) only insofar as werust surfaces them honestly. Keep: same-root confinement (an off-root 3xx `to` is still rejected), verification intact (the redirected target is hash-verified through the same retrieval), and the loop guard (a `_redirects` that redirects to a path that itself redirects must not loop unboundedly — cap the chain, fail closed on a cycle). Coherence with the bar/ENS-name machinery: a 3xx within an ENS site keeps the site identity (compose with the root-CID-prefix ens_pages association).

## Acceptance criteria

- [ ] A matching 3xx rule (301/302/303/307/308) NAVIGATES to the rule's `to` target (bar + history updated), instead of failing with RedirectNotSupported.
- [ ] Placeholder/`:splat` injection into the `to` works for 3xx as it does for 200/404; the `to` is confined to the SAME root CID (an off-root 3xx `to` is still rejected).
- [ ] Verification intact: the redirected target is hash-verified through the same retrieval; a 3xx `to` that does not resolve fails closed.
- [ ] A redirect CHAIN is bounded (a cycle / over-long chain fails closed, no unbounded loop).
- [ ] The 200-rewrite / 404-custom-page / no-_redirects / off-root behaviours from the parent task are unregressed.
- [ ] Applied on desktop + mobile (shared core), or tracked per the parity guard. Tests cover a 3xx navigation, placeholder injection, off-root rejection, a cycle bounded, network-isolated.

## Blocked by

- None. (Builds on the landed `ipfs-web-redirects-and-404-fallback-support`.)

## Prompt

> Goal: deliver the deferred 3xx NAVIGATION for the IPFS `_redirects` file. The parser (`crates/werust-core/src/redirects.rs`) already parses 301/302/303/307/308 rules, but the apply path (`crates/werust-core/src/ipfs.rs`) currently fails a matching 3xx with `RedirectNotSupported` (parent task's DECISIONS.md Decision 3). Make a matching 3xx rule NAVIGATE to its `to` target instead (bar + history updated), the browser-idiomatic redirect.
>
> Keep everything the parent task established: placeholder/`:splat` injection, same-root-CID confinement (off-root `to` rejected), verification intact (redirected target hash-verified, missing `to` fails closed). ADD a bounded redirect chain (cycle / over-long chain fails closed, no unbounded loop). Compose with the root-CID-prefix ens_pages association so a 3xx within an ENS site keeps the site identity in the bar. Done = a matching 3xx navigates, placeholder injection + off-root reject + cycle-bounded + parent behaviours unregressed, network-isolated tests. FIRST re-check redirects.rs parses 3xx and ipfs.rs returns RedirectNotSupported for a matching 3xx.
