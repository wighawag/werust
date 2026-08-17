---
title: "The trust-transition test page: two versions with distinguishable markers, and the publish-and-repoint procedure documented on the page itself"
slug: trust-transition-test-page-and-its-publish-and-repoint-procedure
spec: werust-test-pages
blockedBy: [local-storage-test-page]
covers: [4, 5, 6]
---

## What to build

The discriminating instrument for werust's central trust story: a page designed to be loaded under a MUTABLE name (an ENS name or an IPNS key the tester controls), published in TWO versions, so that a human can check by hand whether a repointed name's new version ran before it was trusted. The v0.4.0 field testing found exactly that broken, and this page is how it gets re-checked instead of re-discovered.

**The page** is dependency-free static HTML that does four small things and shows all of them: it stamps a VERSION MARKER into `localStorage`, logs that marker to the console, attempts a `fetch`, and displays what it wrote, what marker it found from a previous visit (if any), and whether the fetch succeeded. Two versions of the same page differ ONLY in their marker (v1 and v2), so "did v2's script run?" is answerable from three independent signals: the console line, the stored marker, and the fetch.

**The procedure is the test, and it lives ON the page.** The page documents, inline, how to run it: publish it to IPFS, point a mutable name at v1, visit that name in werust, publish v2, repoint the name, and visit again. A tester must be able to follow it without a separate document.

**Write the expected outcome for BOTH the current build and the one after withholding lands, and be accurate about today.** This is the part to get right rather than to copy from the spec's summary, because the spec describes the AFTER state:

- **Today** (what actually ships): a name's version is recorded when the user EXPLICITLY blesses it from the trust indicator, not automatically; a later resolution to a different CID shows the loudest chrome state werust has, the changed-name warning; and the new version's content HAS ALREADY RENDERED AND ITS SCRIPTS HAVE RUN when that warning appears. That last part is the defect the page exists to make visible, so state it as the expected TODAY outcome (a v2 console line, a v2 marker, and a completed fetch, alongside the warning).
- **After `withhold-changed-content-until-trusted` lands**: the changed version is WITHHELD and the user is offered a choice, so v2's script must NOT have run (no v2 console line, no v2 marker, no fetch) until the update is accepted; refusing lands the tester back on v1 and the page shows the v1 marker; accepting renders v2.

Verify the "today" branch against the code before writing it (the trust posture, the bless action's visibility rule and the warning text are all derived in the shared core), and do NOT invent the wording of any button or banner: whatever surface werust shows, the page quotes what SHIPPED rather than minting its own labels, because those strings come from one core derivation and a second copy on a web page is a copy that drifts.

**What the page must NOT claim.** Whether v2 can read v1's marker answers the ORIGIN-SCOPING question empirically (it can only do so if storage is name-scoped, which today it is not), and that is worth stating as an observation the tester should record. It is not a pass/fail criterion of this task, and the scoping DECISION belongs to `explore-name-as-origin`.

No IPFS publishing tooling: the tester publishes by hand (`ipfs add` or a pinning service). Automating it is a follow-on, as is any automated assertion of the withholding guarantee.

## Acceptance criteria

- [ ] Two versions of the page exist under `website/test/`, differing only in their version marker, each dependency-free static HTML with no framework, no bundler and nothing fetched beyond the deliberate test `fetch`.
- [ ] Opening either version shows: the marker it stamped, the marker found from a previous visit (if any), and whether its `fetch` succeeded; the same marker is logged to the console.
- [ ] The page documents its own publish-and-repoint procedure inline, in enough detail to follow without any other document (publish to IPFS, point a mutable name at v1, visit in werust, publish v2, repoint, visit again).
- [ ] The page states the expected outcome for TODAY'S build accurately (an explicit bless is what records a version; the changed-name warning appears on a later different CID; and v2's script HAS run by then, which is the defect being made visible) and for after the withholding work lands (v2's script must not have run until the update is accepted; refuse returns to v1; accept renders v2).
- [ ] The page quotes the labels and wording werust actually SHIPS for any surface it references, rather than inventing its own, and says that they come from werust's own derivation.
- [ ] The page invites the tester to record whether v2 could read v1's marker, framed as an observation about origin scoping rather than a pass/fail of the page.
- [ ] Both versions' links resolve under the site's `/werust/` sub-path, demonstrated by serving `website/` from a sub-path locally.
- [ ] The procedure was actually FOLLOWED once end to end (publish, point a name, visit, repoint, revisit) and what werust did at each step is recorded in the done record. If a step could not be done (no controllable name available), say which and why rather than implying it passed.
- [ ] The landing page gains a one-line index entry for it. If a sibling page task landed first, rebase and add only your entry.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green (a non-regression check: the gate cannot see the page, which is why the criteria above are human observations).

## Blocked by

- `local-storage-test-page`: only to SERIALISE the landing page's index, which both tasks edit. No logical dependency; the `website/` folder and the base-path link rule come from `website-folder-landing-page-and-pages-deploy-workflow`.

## Prompt

> Goal: build the trust-transition test page (two versions, distinguishable markers) and document its publish-and-repoint procedure ON the page, so a human can verify by hand whether a repointed name's NEW version ran before it was trusted. Read `work/specs/tasked/werust-test-pages.md` (stories 4 and 5) first.
>
> The page is small: stamp a version marker into `localStorage`, log it to the console, attempt a `fetch`, and display what was written, what previous marker was found, and whether the fetch succeeded. Two versions differing ONLY in the marker, so "did v2's script run?" has three independent signals. Dependency-free static HTML under `website/test/`, relative links (the site is served under `/werust/`).
>
> The hard part is the EXPECTED OUTCOMES, and you must verify them against the code rather than paraphrasing the spec, which describes the future state. Read `werust_core::pins` and the trust derivation in `werust-core` (the change-warning condition, the bless action's visibility and label, the banner text) plus `docs/adr/0006`, and write the TODAY branch from what ships: a version is recorded by an EXPLICIT bless from the trust indicator (not automatically); a later resolution to a different CID raises the changed-name warning, which is the loudest chrome state there is; and by the time that warning shows, v2 has already rendered and its scripts have already run. That last fact is the defect this page makes visible, so state it plainly as today's expected outcome. Then write the AFTER branch for when `withhold-changed-content-until-trusted` lands: v2 withheld, no v2 console line, no v2 marker, no fetch, until the user accepts; refuse returns to v1; accept renders v2. Do NOT invent any button or banner wording: quote what werust actually shows, and say that those strings come from werust's own derivation (a second copy on a web page is a copy that drifts).
>
> Invite the tester to record whether v2 could read v1's marker, as an ORIGIN-SCOPING observation (it can only happen if storage is name-scoped, which today it is not). That question belongs to `explore-name-as-origin`; do not decide it here.
>
> No IPFS publishing tooling and no automated assertion of the withholding guarantee: both are follow-ons. The tester publishes by hand.
>
> The acceptance gate is pure-Rust and Linux-only and CANNOT see this page, so your evidence is OBSERVATION: follow your own procedure once, end to end, with a name you control, and write down what werust did at each step. If you cannot complete a step (no controllable mutable name), say which and why. Keep the gate green as a non-regression.
>
> You share exactly one file with the sibling page tasks: the landing page's index. Add only your entry; if a sibling landed first, rebase.
