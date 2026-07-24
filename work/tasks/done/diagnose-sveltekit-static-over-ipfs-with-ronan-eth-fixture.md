---
title: "Diagnose + fix why a SvelteKit adapter-static site's nested-route data (blog __data.json) fails over ipfs:// (the ronan.eth blog 500); land a ronan-eth-derived regression fixture"
slug: diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

FIELD FINDING (v0.2.4, human, DESKTOP): on `ronan.eth`, clicking "blog" reaches the blog page but shows NO blog posts and prints a "500 error" - which is NOT a real server 500 (there is no server), it is SvelteKit's client-side error output when its data load fails. Clicking "portfolio" works fine. Root-cause source: `work/notes/observations/field-test-v0.2.4-spa-clientrouting-eth-path-blank-links-2026-07-23.md` (finding D). This is a DIAGNOSE-THEN-FIX; do the diagnosis first (`~/.agents/skills/diagnosing-bugs/SKILL.md`), record the root cause with evidence, then fix. Do NOT guess-patch.

SCOPE: the DESKTOP blog-data-over-ipfs failure only. The MOBILE half of finding D (blog/portfolio buttons do NOTHING on mobile) is PARKED for a human device re-test and is explicitly OUT of scope here - do not attempt the mobile no-navigation symptom (it needs a device to diagnose). Note that in the done-record + surface it as a parked follow-on.

The fixture: `ronan.eth` is a SvelteKit `@sveltejs/adapter-static` prerendered site (source `../ronan-eth/web`: `+layout.ts` has `prerender = true`, `trailingSlash = 'always'`, `ssr = true`; the build has `build/blog/index.html` AND `build/blog/__data.json`, `build/portfolio/...`, `build/_app/...`). On a SvelteKit client-side nav to `/blog`, the client router fetches `/blog/__data.json` to hydrate the route data. That fetch goes through werust's `ipfs://` scheme handler; when it fails (wrong bytes / wrong MIME / path not resolved / CAR-scope miss for the nested `blog/__data.json`), SvelteKit renders its error boundary = the "500", and no posts show. Portfolio working but blog not suggests a per-route data-shape difference (blog is a list route with its own `__data.json`; confirm what actually differs - it may be the nested path, the `__data.json` content, the trailing-slash form, or the MIME).

Diagnose (with evidence, not assumption):
- Reproduce the blog data fetch over the `ipfs://` path against a ronan-eth (or minimal SvelteKit-adapter-static) build served content-addressed. Capture what werust's `ipfs://` handler actually returns for `ipfs://<cid>/blog/__data.json` (and whatever exact URL SvelteKit requests - watch for a trailing-slash or query-string form): the status, the MIME, the bytes, vs what `build/blog/__data.json` contains.
- Determine WHY blog fails and portfolio works: is it the nested-directory `__data.json` path resolution in `resolve_ipfs_request` / the retriever's `dag-scope=entity` scoping for that path (`crates/werust-core/src/ipfs.rs`, `crates/fetcher/src/retriever.rs`), the MIME inference for `__data.json` (`mime_type_for_path`), the trailing-slash normalization, or a query-string SvelteKit appends?

Fix (only what the diagnosis proves): make werust's `ipfs://` retrieval + path/MIME resolution serve a SvelteKit adapter-static site's nested prerendered pages AND their `__data.json` client-nav data correctly, so the blog page renders its posts end to end. Keep verification/fail-closed intact (every block still hash-verified; a genuinely-missing resource still fails closed with its reason - the fix is correct resolution, not a bypass).

The regression fixture (the human's suggestion): land a network-isolated werust test that serves a SvelteKit-adapter-static build (a minimal one derived from `../ronan-eth/web/build`, or a purpose-built tiny one with the same shape: a root page, a nested list route with `index.html` + `__data.json`, and `_app/`) content-addressed, and asserts werust resolves + serves the nested page AND its `__data.json` correctly. This guards the whole SvelteKit-over-ipfs class going forward. Keep the fixture SMALL and committed (do not vendor all of ronan-eth); derive the minimal shape that reproduces the bug.

## Acceptance criteria

- [ ] The desktop blog-data-over-ipfs failure is DIAGNOSED and its root cause recorded durably (`docs/spikes/<slug>/DIAGNOSIS.md`) with evidence (the exact request SvelteKit makes, what werust's ipfs:// handler returns for it, and why blog fails while portfolio works).
- [ ] The `ipfs://` path/MIME/retrieval resolution is fixed so a SvelteKit adapter-static site's nested prerendered page (`/blog/`) AND its `__data.json` client-nav data resolve correctly; the blog page renders its posts (no SvelteKit error boundary / "500").
- [ ] Verification/fail-closed unchanged: every block still hash-verified; a genuinely-missing resource still fails closed with its legible reason (the fix is correct resolution, not a bypass).
- [ ] A SMALL, committed, network-isolated regression fixture (a minimal SvelteKit-adapter-static-shaped build: root + nested list route with index.html + __data.json + _app/) is served content-addressed and asserts werust resolves + serves the nested page and its __data.json - guarding the SvelteKit-over-ipfs class.
- [ ] The MOBILE no-navigation half of finding D is explicitly left PARKED for a human device re-test (recorded in the done-record as a named follow-on), NOT attempted here.
- [ ] Tests are network-isolated and ride `cargo test`.

## Blocked by

- None. (Best landed after the SPA-url-tracking + `.eth/<path>` tasks so the blog page is reachable + reflected, but the data-fetch fix is independent; land order flexible.)

## Prompt

> Goal: diagnose + fix why the ronan.eth BLOG shows a SvelteKit "500" with no posts over `ipfs://` while PORTFOLIO works (desktop). It is SvelteKit's client error boundary firing because its `/blog/__data.json` fetch over werust's `ipfs://` handler fails. DIAGNOSE first (`~/.agents/skills/diagnosing-bugs/SKILL.md`), record the root cause with evidence, then fix. SCOPE = desktop blog-data-over-ipfs ONLY; the MOBILE no-navigation half of the finding is PARKED for a device re-test - do not attempt it.
>
> Fixture: `ronan.eth` = SvelteKit `@sveltejs/adapter-static`, `../ronan-eth/web` (`prerender=true`, `trailingSlash='always'`; build has `build/blog/index.html` + `build/blog/__data.json`, `build/portfolio/...`, `build/_app/...`). Where to look: `crates/werust-core/src/ipfs.rs` (`resolve_ipfs_request` sub-path + directory-index resolution, `mime_type_for_path` - `json` is mapped), `crates/fetcher/src/retriever.rs` (the `dag-scope=entity` scoping for a nested `blog/__data.json`). Determine what werust returns for the EXACT URL SvelteKit requests (watch trailing slash / query string) vs `build/blog/__data.json`, and why blog fails but portfolio works.
>
> Fix only what the diagnosis proves; keep every block hash-verified + fail-closed. LAND a SMALL committed network-isolated regression fixture (a minimal SvelteKit-adapter-static-shaped build: root + nested list route with index.html + __data.json + _app/) served content-addressed, asserting werust serves the nested page AND its __data.json. Done = the acceptance list; the mobile half recorded as a parked follow-on.
