---
title: "Move the EXISTING provider inspector into the published site, so it is loadable by URL in werust"
slug: publish-the-provider-inspector-from-the-website
spec: werust-test-pages
blockedBy: [website-folder-landing-page-and-pages-deploy-workflow]
covers: [3]
---

## What to build

The Ethereum-provider test page is not a new page: `docs/dev/provider-inspector.html` already exists and does more than a fresh one would (it shows every property on `window.ethereum`, makes each EIP-1193 method callable, exercises EIP-6963 enumeration and logs interactions), and it has a companion `docs/dev/werust-provider-mimic.js` that installs a `window.ethereum` behaving exactly like werust's injected provider so a dapp can be tested in a normal browser.

Because `docs/` is deliberately NOT published, linking to it is not an option. MOVE it: the inspector and its companion script go to `website/test/`, `docs/dev/README.md` keeps a pointer to the new home, and the landing page gains its index entry. Publishing it is a gain in itself: it becomes loadable from a URL in werust rather than only from a local file path.

Keep the move a MOVE. Do not rewrite the page, do not "improve" what it inspects, and do not fork a second weaker version: the whole point of story 3 is that the tool that already exists is the one the tester gets. The only edits are the ones the new location forces:

- the page's reference to its companion script must resolve at the new path and under the site's `/werust/` sub-path (relative, never leading-slash absolute);
- `docs/dev/README.md` must point at the new home rather than describing a file that is no longer there (it currently describes both files as living in `docs/dev/`);
- anything else in the tree that references either file by path must be updated (grep first: the spec and this task are not the only possible referrers).

The mimic script keeps working from a local copy too (it is documented as something a tester copies into a dapp's static directory and activates with a query parameter or a local-storage flag): do not break that usage while moving it.

## Acceptance criteria

- [ ] `website/test/` holds the provider inspector and its companion script, and `docs/dev/` no longer holds them.
- [ ] Opening the moved page from the working tree shows the same thing it showed before the move: every property on `window.ethereum`, each EIP-1193 method callable, the EIP-6963 enumeration result, and the interaction log.
- [ ] The page finds its companion script when the site root is a sub-path (`/werust/`), demonstrated by serving `website/` from a sub-path locally.
- [ ] Loaded in werust, the page reports werust's real provider (`isWerust`, the empty accounts list, the refusals werust actually returns) rather than an error about a missing script.
- [ ] Loaded in a normal browser with the mimic active, it reports the mimicked provider, so the two can still be compared side by side.
- [ ] `docs/dev/README.md` points at the new home and describes no file that has moved away; no other reference in the tree still points at the old path (grepped, not assumed).
- [ ] The page content itself is UNCHANGED apart from what the new location forces; the diff shows a move plus path fixes, not a rewrite.
- [ ] The landing page gains a one-line index entry for it. If a sibling page task landed first, rebase and add only your entry.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green (a non-regression check: the gate cannot see the page, which is why the criteria above are human observations).

## Blocked by

- `website-folder-landing-page-and-pages-deploy-workflow`: `website/` and the landing index must exist before a page can be moved into it and indexed. That task also owns the base-path link rule this one must follow, and the landing page is the one file you share with the two sibling page tasks.

## Prompt

> Goal: publish the provider inspector that already exists, by MOVING it into the site rather than writing a second one. Read `work/specs/tasked/werust-test-pages.md` (story 3) first.
>
> Move `docs/dev/provider-inspector.html` and `docs/dev/werust-provider-mimic.js` into `website/test/`, fix the page's reference to its companion script so it resolves at the new path AND under the site's `/werust/` sub-path (relative links only), update `docs/dev/README.md` to point at the new home instead of describing files that are no longer there, and grep the tree for any other reference to the old paths. Add a one-line entry to the landing page's index.
>
> Do not rewrite the page. It already covers more than the spec would have specified (every `window.ethereum` property, each EIP-1193 method callable, EIP-6963 enumeration, interaction logging), and the mimic script is a faithful port of werust's injected provider (`crates/werust-core/src/provider.rs` and `ethereum.rs`) that a tester copies into a dapp and activates with a query parameter or a local-storage flag. Keep both usages working: loaded in werust the page must report werust's REAL provider, and loaded in a normal browser with the mimic active it must report the mimic, because comparing the two is the point.
>
> Verify by OBSERVATION, since the pure-Rust acceptance gate cannot see HTML at all: open the page from the working tree, serve `website/` from a sub-path and follow its script reference, load it in werust, and load it in a normal browser with the mimic on. Write what you saw in the done record. Keep the gate green as a non-regression (`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test`).
>
> You share exactly one file with the two sibling page tasks (`local-storage-test-page`, `trust-transition-test-page-and-its-publish-and-repoint-procedure`): the landing page's index. If one landed first, rebase and add only your own entry.
