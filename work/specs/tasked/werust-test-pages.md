---
title: "werust feature-test pages: a small, published site for verifying werust-specific browser behaviour (storage, the ethereum provider, trust transitions)"
slug: werust-test-pages
humanOnly: true
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/ready/` tasks. (The technical-detail sections below are trimmed by `to-task` once the work is tasked — they move into tasks/ADRs and this spec settles to its durable framing: Problem / Solution / User Stories / Out of Scope.)
> Tasked 2026-08-17 (`to-task`): the Implementation / Testing detail this spec carried moved INTO the tasks it emitted (`work/tasks/`, carrying `spec: werust-test-pages`), which are the current truth for what to build. Durable rationale is recorded as ADRs by the tasks that decide it, not predicted here.

## Problem Statement

werust has no reproducible way to verify most of the browser behaviours that distinguish it from every other browser: content-addressed loading, storage behaviour across trust transitions, and the withholding guarantee's "content does not render before you trust it" guarantee. Each of these is testable only by finding a real site that happens to exercise the right path, which is fragile and unreproducible. The v0.4.0 field testing that found the TOFU load-before-trust bug and the lost mobile pin would have been faster and more reliable with a dedicated page that exercises each feature in isolation.

## Solution

A small static site, published via GitHub Pages from this repo, that provides werust-specific test pages. It is NOT a general test suite or a CI harness — it is a set of **manual test pages** a human (or the conductor doing field testing) opens in werust to verify a behaviour is working.

### Initial scope: four pages

1. **`/` (landing)** — a minimal index linking to each test page, with a one-line description of what each verifies. This is also the project's public face (the repo has no website today; GitHub Pages is not enabled).

2. **`/test/local-storage`** — sets a `localStorage` key with a timestamped value, reads it back, and displays the result. On a return visit, it shows whether the previous value survived. This is the page that would have caught "the new content's scripts ran before I trusted it" — if the TOFU gate is working, visiting this page under a *changed* name should NOT show the previous value (the new content's scripts never ran), and after trusting it, the new content's scripts run and can read/write their own local storage.

3. **`/test/ethereum`** — NOT a new page. `docs/dev/provider-inspector.html` already exists and does more than this spec would have specified (every property on `window.ethereum`, each EIP-1193 method callable, EIP-6963 enumeration, interaction logging). Because `docs/` is not published, linking is not an option: the page MOVES to `website/test/provider-inspector.html` (with its `werust-provider-mimic.js` companion), and `docs/dev/README.md` keeps a pointer to its new home. Publishing it is a gain in itself — it becomes loadable from a URL in werust rather than only from a local file path.

4. **`/test/trust-transition`** — a page designed to be loaded under a mutable name (an ENS name or an IPNS key the tester controls). It stamps a VERSION MARKER into `localStorage`, logs that marker to the console, and attempts a `fetch`. Publishing two versions of it (v1, v2) under one name makes the whole TOFU story checkable by hand:
   - Visit under the name at v1 → the version is auto-recorded (`withhold-changed-content-until-trusted`), the marker is written.
   - Repoint the name to v2, revisit → the gate must fire: the modal appears and **v2's script must NOT have run** (no v2 console line, no v2 marker, no fetch). This is the exact security property the v0.4.0 field test found broken, and this page is how it is re-checked.
   - Choose the refuse action → the tester lands back on v1 (the modal's exact button labels come from the core derivation, so the page quotes whatever shipped rather than minting its own), and the page shows the v1 marker.
   - Choose the accept action → v2 renders and its script runs.
   - Whether v2 can read **v1's marker** answers the origin-scoping question empirically: it can only do so if storage is name-scoped.

### Publishing

The site lives in a dedicated **`website/`** folder at the repo root, deliberately NOT in `docs/`. `docs/` holds werust's internal engineering record — every ADR, 87 spike folders with their probe reports and screenshots, the capability matrix, the conformance tiers — and none of that is intended as the project's public face. A separate folder also means the site's paths are its own: `website/index.html` is the site root and `website/test/<name>.html` are the test pages, with no internal tree to exclude.

**That choice picks the deployment mechanism, because it has to.** GitHub Pages' native branch-folder option offers only the repository root or `/docs` — an arbitrary folder is not selectable. So the site is deployed by a **Pages workflow** that uploads `website/` as the Pages artifact. Two things follow, and both are improvements:

- **No Jekyll.** A directly-uploaded artifact is served as-is, so nothing processes the files. The Liquid-looking `{{`/`{%` syntax in two spike READMEs becomes irrelevant, and no `.nojekyll` marker is needed — a class of build failure removed rather than worked around.
- **The one-off human action changes shape.** It is no longer "pick a folder" but "set the Pages source to GitHub Actions" — still a repo-settings action no agent can perform, still exactly one click, and still owned by a human task.

The site is served under the project path (`/werust/`), not the domain root, so intra-site links must be written so they resolve under a base path rather than assuming `/`.

The new workflow must not disturb the three existing tests that parse workflow files (`verify_gate_shape.rs`, `release_plumbing_shape.rs`, `windows_renderer_leg_shape.rs`) — they assert on specific legs, and a careless edit to a shared file reds the pure-Rust gate.

## User Stories

1. As a werust developer field-testing a build, I want a published landing page describing werust and linking each test page, so that verification starts from one known place instead of hunting for a suitable real site.
2. As a tester, I want a storage page that writes a timestamped value and reports whether a previous one survived, so that I can OBSERVE what storage scoping does today and confirm it changed when name-as-origin lands. (It cannot test the withholding guarantee: under the current CID-scoped origin a new version is a different origin and can never read the old version's storage whether or not its scripts ran, so the observation is identical when the guarantee holds and when it is broken. The trust-transition page is the discriminating instrument.)
3. As a tester, I want the EXISTING provider inspector moved into the published site, so that I can load it by URL in werust and use the tool that already exists rather than a second weaker one.
4. As a tester, I want a trust-transition page whose two published versions each stamp a distinguishable marker, so that I can verify a repointed name's new version did NOT run before I accepted it.
5. As a tester, I want that page to document its own publish-and-repoint procedure inline, so that the test is reproducible without a separate document.
6. As a tester on a slow or offline link, I want every page to be dependency-free static HTML, so that the rig itself never becomes the thing that fails.
7. As the repo owner, I want the one-off Pages-source setting called out as mine to do, so that the site does not sit unpublished waiting on a step no agent can take.
8. As a reader of werust's docs, I want the internal engineering record to stay unpublished, so that choosing to have a website does not silently make every ADR and spike report a public page.

## Out of Scope

- **Automated testing against these pages** (a werust smoke that drives the browser to each page and asserts) is a follow-on. This spec delivers the pages; a future spec can wire them into an interactive test harness.
- **IPFS publishing automation** (a script that publishes the pages to IPFS and returns CIDs) is a follow-on. The tester publishes manually for now.
- **A custom domain** (`werust.dev`) is a follow-on. The initial URL is the project Pages path.
- **Publishing `docs/`.** Decided against: the internal engineering record stays internal.
- **Comprehensive browser-feature tests** (CSS support, WebGL, WebRTC, etc.) are out of scope. These pages test werust-specific behaviours, not general web compatibility.
- **The local-storage scoping DECISION** is not made here. That the name should be the origin is decided in `explore-name-as-origin`; this spec only provides the page that makes the current and future behaviour visible to a human. The automated per-edge measurement is the probes' job (`explore-name-as-origin` story 2), not this page's.
- **A dependency on the withholding work.** The pages are static HTML and need none of it; only the trust-transition page's EXPECTED OUTCOME changes once withholding lands, which the page's inline procedure states for both before and after. So this spec is deliberately not ordered behind it.
- **Automated assertion of the withholding guarantee.** The trust-transition page lets a HUMAN see whether the new version's script ran. Asserting it in CI is a follow-on.