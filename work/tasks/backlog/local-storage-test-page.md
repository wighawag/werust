---
title: "A storage test page that shows whether a previous value survived, so storage scoping is observable instead of argued"
slug: local-storage-test-page
spec: werust-test-pages
blockedBy: [publish-the-provider-inspector-from-the-website]
covers: [2, 6]
---

## What to build

A single dependency-free static page under `website/test/` that writes a timestamped value into `localStorage`, reads it back, and displays BOTH: what it just wrote, and whether a value from a previous visit was found (and what it was). That is the whole page. Its value is that it makes storage scoping OBSERVABLE: today the Android edge maps `ipfs://` loads onto an internal `https://<cid>.ipfs.werust.invalid` host while other edges do not, DOM storage was `null` on Android until recently, and whether storage follows the NAME or the CID is an open question a later spec (`explore-name-as-origin`) exists to answer. A tester who can see the value survive or not on each edge can tell what today's behaviour is, and will be able to tell when it changes.

**Be honest, on the page itself, about what it cannot test.** It cannot demonstrate the withholding guarantee ("the new content's scripts did not run before I trusted it"). Under the current CID-scoped origin a new version IS a different origin and can never read the old version's storage, whether or not its scripts ran, so the observation is IDENTICAL when the guarantee holds and when it is broken. The discriminating instrument is the trust-transition page (the sibling task). Say that in a sentence on the page, so a tester cannot draw the wrong conclusion from a page that looks like it proves something stronger.

Also show, plainly, the things a tester needs to attribute a result: the document's origin as the engine reports it, and whether `localStorage` is a `Storage` object, a `SecurityError` throw, or `null` (that third answer is non-conformant and is exactly the fingerprint that identified the Android DOM-storage bug, so it must be reported as itself rather than collapsed into "unavailable"). Report `sessionStorage` the same way if it is cheap, since a fix that leaves one working and the other broken has fixed half the problem.

Dependency-free static HTML: one file, no framework, no bundler, nothing fetched. It must work on a slow or offline link, because the rig must never be the thing that fails.

## Acceptance criteria

- [ ] Opening the page shows the value it just wrote (with its timestamp) and, on a return visit, whether a previous value was found and what it was.
- [ ] The page states in its own text that it CANNOT test the withholding guarantee, and points at the trust-transition page as the discriminating instrument.
- [ ] The page reports the document origin as the engine sees it, and reports `localStorage` being `null` DISTINCTLY from it throwing and from it working (the `null` case is the Android fingerprint and must be visible as itself).
- [ ] Loaded in werust on the Linux edge, the page shows a value written and then, on reload, that previous value surviving; what was observed is recorded in the done record.
- [ ] Loaded in a normal browser, the page behaves the same way, so a tester can compare werust against Firefox or Chrome.
- [ ] The page is one self-contained HTML file with no external asset, no framework and no build step, and it works with no network access.
- [ ] Its links resolve under the site's `/werust/` sub-path, demonstrated by serving `website/` from a sub-path locally.
- [ ] The landing page gains a one-line index entry for it. If a sibling page task landed first, rebase and add only your entry.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green (a non-regression check: the gate cannot see the page, which is why the criteria above are human observations).

## Blocked by

- `publish-the-provider-inspector-from-the-website`: only to SERIALISE the landing page's index, which the two tasks both edit. There is no logical dependency, and the `website/` folder plus the base-path link rule come from the task above that one.

## Prompt

> Goal: write the storage test page for the published site, so a human can SEE what storage scoping does today and tell when it changes. Read `work/specs/tasked/werust-test-pages.md` (story 2, and the out-of-scope note about the scoping DECISION belonging to `explore-name-as-origin`) first.
>
> Build one dependency-free static file under `website/test/`: write a timestamped value to `localStorage`, read it back, and display both what was written and whether a previous visit's value was found. Report the document origin as the engine reports it, and report `localStorage` being `null` as its own distinct outcome, separate from a `SecurityError` throw and from working normally. That distinction is not pedantry: `null` is non-conformant and is the exact fingerprint that identified the Android DOM-storage bug (`work/notes/findings/android-localstorage-is-null-dom-storage-never-enabled-2026-07-31.md`), and Android is also the one edge that maps `ipfs://` onto an internal `https://<cid>.ipfs.werust.invalid` origin. Include `sessionStorage` if it is cheap.
>
> Put the honest limit ON THE PAGE: this page cannot test the withholding guarantee, because under a CID-scoped origin a new version is a different origin and can never read the old version's storage whether or not its scripts ran, so the observation looks the same when the guarantee holds and when it is broken. Point at the trust-transition page (sibling task `trust-transition-test-page-and-its-publish-and-repoint-procedure`) as the discriminating instrument.
>
> Constraints: one self-contained HTML file, no framework, no bundler, nothing fetched, works offline (the rig must never be the thing that fails); relative links only, because the site is served under the `/werust/` sub-path.
>
> The acceptance gate is pure-Rust and Linux-only and CANNOT see this page, so your evidence is OBSERVATION: open it in werust, reload it, open it in a normal browser, and write what you saw in the done record. Keep the gate green as a non-regression.
>
> You share exactly one file with the sibling page tasks: the landing page's index. Add only your entry; if a sibling landed first, rebase.
