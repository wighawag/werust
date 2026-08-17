---
title: "Set the repository's Pages source to GitHub Actions, so the site actually publishes"
slug: enable-the-pages-source-so-the-site-publishes
spec: werust-test-pages
humanOnly: true
blockedBy: [website-folder-landing-page-and-pages-deploy-workflow]
covers: [7]
---

## What to build

One repository SETTING, which no agent can perform: set this repo's GitHub Pages source to GitHub Actions, so the workflow that uploads `website/` as the Pages artifact can actually deploy it. GitHub Pages is not enabled on this repository today, and until it is, the workflow's deploy step fails and the site sits unpublished waiting on a step no agent can take.

`humanOnly` here is by NATURE, not a review preference: it is a repo-administration action performed in the repository's settings by someone with admin rights. There is nothing to build and nothing to review in a diff.

Then confirm the outcome, because "the setting is flipped" is not the same as "the site is up": re-run the Pages workflow, wait for it to deploy, open the project Pages URL in a normal browser AND in werust, and check the landing page is what the repo's `website/index.html` says it is, with its links working under the `/werust/` sub-path.

If the deploy still fails after the setting, record WHY (the workflow's permissions, an environment approval, a first-deploy quirk) in the done record: that is the knowledge the next person needs, and it is the only part of this task that could produce a follow-on.

## Acceptance criteria

- [ ] The repository's Pages source is set to GitHub Actions.
- [ ] The Pages workflow completes, including its deploy step, and the run is named in the done record.
- [ ] The project Pages URL serves the landing page: opened in a normal browser it shows werust's description, the repo link and the test-page index, and every intra-site link works under the `/werust/` sub-path.
- [ ] The same URL loads in werust itself (this is the point of publishing: a test page reachable by URL rather than only from a local file path).
- [ ] Nothing from `docs/` is reachable under the published site (spot-check a known internal path, for example an ADR or a spike README, and confirm it is not served).
- [ ] If the deploy needed anything beyond the setting, that is recorded so the next person does not rediscover it.

## Blocked by

- `website-folder-landing-page-and-pages-deploy-workflow`: there is nothing to publish, and no workflow to deploy, until the folder, the landing page and the Pages workflow are on `main`.

## Prompt

> Goal (HUMAN, by nature): set this repository's GitHub Pages source to GitHub Actions, then confirm the site is actually published.
>
> Context: `work/specs/tasked/werust-test-pages.md` chose a dedicated `website/` folder over `docs/` (the internal engineering record stays internal), and because Pages' native branch-folder option offers only the repository root or `/docs`, publishing is done by a WORKFLOW that uploads `website/` as the Pages artifact. That workflow landed with `website-folder-landing-page-and-pages-deploy-workflow` and cannot deploy until the Pages source is GitHub Actions. This is one click in the repository's settings, and it is the only step in this whole spec that an agent cannot do.
>
> Then verify, in this order: re-run the Pages workflow and watch the deploy step; open the project Pages URL in a normal browser and check the landing page shows what `website/index.html` says, with every link working under the `/werust/` sub-path; open the same URL in werust; and spot-check that a known internal path (an ADR, a spike README) is NOT served, since keeping `docs/` unpublished was a deliberate decision. Record the run and, if the deploy needed anything beyond the setting, record what.
