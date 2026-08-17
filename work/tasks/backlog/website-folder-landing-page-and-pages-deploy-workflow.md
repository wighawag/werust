---
title: "A website/ folder, a landing page that is werust's public face, and the Pages workflow that publishes it (docs/ stays internal)"
slug: website-folder-landing-page-and-pages-deploy-workflow
spec: werust-test-pages
blockedBy: []
covers: [1, 6, 8]
---

## What to build

werust has no website, and no reproducible way to verify the browser behaviours that distinguish it from every other browser. This task builds the foundation for both: a dedicated `website/` folder at the repo root, a landing page that is simultaneously the project's public face and the index to the test pages, and the workflow that publishes it.

**`website/`, deliberately NOT `docs/`.** `docs/` is werust's internal engineering record (every ADR, the spike folders with their probe reports and screenshots, the capability matrix, the conformance tiers) and none of it is intended as a public page. A separate folder also means the site's paths are its own: `website/index.html` is the site root, `website/test/<name>.html` are the test pages, and there is no internal tree to exclude.

**That choice picks the deployment mechanism, because it has to.** GitHub Pages' native branch-folder option offers only the repository root or `/docs`, so an arbitrary folder is not selectable. The site is therefore deployed by a Pages WORKFLOW that uploads `website/` as the Pages artifact. Two consequences, both improvements: a directly-uploaded artifact is served as-is, so nothing processes the files (the Liquid-looking `{{`/`{%` syntax in `docs/spikes/android-apk-signing/README.md` and `docs/spikes/release-notes-lose-the-conventional-commit-changelog-to-a-race/README.md` becomes irrelevant and NO `.nojekyll` marker is needed), and the one-off human action becomes "set the Pages source to GitHub Actions" rather than "pick a folder".

**The landing page.** Dependency-free static HTML, no build step, no framework, readable as a source file: a short description of what werust is, a link to the repo, and an index of the test pages with a one-line description of what each verifies. Only the pages that EXIST get an entry (three sibling tasks each add their own page and its entry). The site is served under the project path (`/werust/`), not a domain root, so every intra-site link must resolve under a base path: relative links, never a leading-slash absolute path.

**Two hazards, both real.**

1. The publish cannot work until a human sets the Pages source to GitHub Actions (`enable-the-pages-source-so-the-site-publishes`, which is a repo-settings action no agent can perform). Until then the deploy step fails. Do not treat a failed deploy as a code defect, do not disable the workflow to hide it, and do not claim a published URL in this task's evidence: the observable acceptance HERE is the page opened from the working tree, and the published URL is that human task's acceptance.
2. Three existing tests PARSE workflow files (`crates/werust-core/tests/verify_gate_shape.rs`, `release_plumbing_shape.rs`, `windows_renderer_leg_shape.rs`) and one (`toolchain_pin_shape.rs`) parses EVERY YAML under `.github/` and forbids any step that re-selects a Rust toolchain. Adding a new workflow is fine; editing theirs is not, and a careless edit to a shared file reds the pure-Rust gate.

This spec's own `humanOnly` flag gates only the TASKING of the spec. It does not apply to this task: building the folder, the page and the workflow is ordinary agent work.

## Acceptance criteria

- [ ] Opening `website/index.html` in any browser shows what werust is in a few sentences, a link to the repository, and an index of the test pages that exist, each with a one-line description of what it verifies.
- [ ] The page renders with no network access and no `npm install`: it is one self-contained HTML file with no framework, no bundler and no external asset it needs to fetch.
- [ ] Every intra-site link resolves when the site root is a sub-path (`/werust/`), not only at a domain root; demonstrated by serving the folder from a sub-path locally and following each link.
- [ ] A Pages workflow uploads `website/` (and only `website/`) as the Pages artifact and deploys it, with the permissions that requires; no `.nojekyll` marker is added and no Jekyll step exists.
- [ ] Nothing under `docs/` is published: the artifact contains the website files and nothing from the internal engineering record; verified by inspecting what the upload step is given.
- [ ] The workflow's comments state that the deploy cannot succeed until the Pages source is set to GitHub Actions, and name the task that owns that setting.
- [ ] `verify.yml`, `release.yml`, `windows-renderer.yml` and the three workflow-parsing tests are untouched, and `toolchain_pin_shape.rs` stays green over the new YAML.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green (a non-regression check: the gate cannot see the page at all, which is why the criteria above are human observations).

## Blocked by

- None: can start immediately.

## Prompt

> Goal: create `website/`, write werust's landing page, and add the GitHub Pages workflow that publishes the folder. Read `work/specs/tasked/werust-test-pages.md` first: it explains why the site lives in `website/` rather than `docs/` (the internal engineering record stays internal) and why that choice forces a Pages WORKFLOW rather than the native branch-folder option (Pages offers only the repo root or `/docs`).
>
> Build: `website/index.html`, dependency-free static HTML, no build step, no framework, auditable by reading it. It is BOTH the project's public face (a short description of werust, a link to the repo) and the index of the test pages, with a one-line description of what each page verifies. Only index pages that exist; three sibling tasks add their own page and their own entry after you, so leave the index easy to append to and expect them to edit this file.
>
> The site is served under the project path (`/werust/`), so write links that resolve under a base path (relative, never leading-slash absolute), and PROVE it by serving the folder from a sub-path locally and following every link. That is the failure this criterion exists to catch: links that only work at a domain root.
>
> Add the Pages workflow: upload `website/` as the Pages artifact and deploy it, with the permissions Pages deployment needs. A directly-uploaded artifact is served as-is, so there is no Jekyll step and NO `.nojekyll` marker is needed (two spike READMEs contain Liquid-looking `{{`/`{%` syntax, which is exactly why the as-is path was chosen). Do not upload anything from `docs/`.
>
> Two hazards. First, the deploy CANNOT succeed until a human sets the Pages source to GitHub Actions: that is `enable-the-pages-source-so-the-site-publishes`, a repo-settings action no agent can take. Say so in the workflow's comments, name that task, and do not claim a published URL as this task's evidence: your observable acceptance is the page opened from the working tree and served locally from a sub-path. Second, three tests PARSE workflow files (`verify_gate_shape.rs`, `release_plumbing_shape.rs`, `windows_renderer_leg_shape.rs`) and `toolchain_pin_shape.rs` parses EVERY `.github/**/*.yml` and forbids toolchain re-selection: add your workflow, edit none of theirs, and keep the pure-Rust gate green.
>
> Note what this task is NOT: not the test pages themselves (three sibling tasks), not IPFS publishing tooling, not a custom domain, not a smoke that drives werust against the pages. And note that the SPEC's `humanOnly` flag gates only the tasking of the spec: it does not make this task human-only, and only the Pages-source task is.
