---
title: "URGENT: the Windows release job fails to link — `app.manifest` uses `--` inside an XML comment, which XML forbids"
slug: windows-app-manifest-is-malformed-xml-and-breaks-the-release-link
blockedBy: []
covers: []
---

## What to build

A post-merge defect in `windows-release-packaging-leg`, found by the conductor on 2026-07-31 by firing the release dry run after that task merged. One character class wide, and it takes the whole Windows release artifact down.

## The failure, diagnosed exactly

[Release run 30624906073](https://github.com/wighawag/werust/actions/runs/30624906073), job `windows-desktop-app`, step "Build werust-windows (release, x86_64-pc-windows-msvc)":

```
\\?\D:\a\werust\werust\crates\werust-windows\app.manifest : general error c1010070:
    Failed to load and parse the manifest. Windows was unable to parse the requested XML data.
LINK : fatal error LNK1327: failure during running mt.exe
error: linking with `link.exe` failed: exit code: 1327
```

Every other job in that run is GREEN (`verify`, `goreleaser`, `android-apk`, `ios-simulator-app`, `macos-desktop-app`), so the Windows artifact is the only casualty — the leg's decoupling worked exactly as designed.

**The cause.** `crates/werust-windows/app.manifest` is not well-formed XML:

```
XML PARSE ERROR: not well-formed (invalid token): line 17, column 52
```

Line 17 sits inside the opening `<!-- ... -->` block:

```
(name + version + publicKeyToken + language) -- a wrong token silently
```

**XML forbids a double hyphen `--` inside a comment.** It is used there as a prose dash, which is this repo's ordinary house style in Rust and Markdown (the repo deliberately avoids the em dash character) — but an XML comment is the one place that style is illegal. `mt.exe` is right to refuse, and `link.exe` then dies.

## What to do

1. **Remove every `--` used as prose punctuation from inside the XML comments** in `app.manifest`. Rephrase properly (a comma, a colon, parentheses, or two sentences); do not substitute a spaced hyphen. **Check the WHOLE file, not just line 17** — there is at least one further comment block, and the parser stops at the first error, so a second occurrence would come back as a fresh red on the next run.
2. **Do not solve this by deleting the comments.** They are load-bearing and good: the comctl32 identity explanation, the "what it does NOT buy: dark mode" note, and the per-monitor-v2 DPI rationale are exactly what the next reader needs. The content was reviewed and approved; only its punctuation is illegal here.
3. **Add a guard, because nothing in this repo parses that file.** The Ubuntu gate cannot link a Windows binary, so `verify` could never notice a malformed manifest, and the existing shape test pins the comctl32 identity STRINGS without checking well-formedness. Assert that `app.manifest` parses as XML, so the next edit to those comments cannot red a Windows release job again. If the workspace has no XML parser to hand, prefer a dependency-free check (balanced tags plus no `--` inside a comment span) over adding a dependency for one file; say which you chose and why.
4. **Re-verify by DISPATCHING, not by reasoning.** `gh workflow run release.yml --ref <your branch>`, confirm `windows-desktop-app` goes GREEN and uploads its artifact, and record the run URL. The workflow is on `main`, so dispatch-by-ref is legal — that is the standing convention in `CONTEXT.md`.

**Everything else about the leg stands.** The job shape, the manifest's content, the version source and the honest artifact naming were all reviewed and approved; do not revisit them.

## Acceptance criteria

- [ ] `crates/werust-windows/app.manifest` parses as well-formed XML, with its explanatory comments intact and rephrased rather than deleted.
- [ ] A guard in the ordinary Ubuntu gate fails if the manifest stops being well-formed.
- [ ] A dispatched `release.yml` run shows `windows-desktop-app` GREEN with its artifact uploaded, and the run URL is recorded.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Prompt

> Goal: the `windows-desktop-app` release job fails at link — `mt.exe` cannot parse `crates/werust-windows/app.manifest` (`general error c1010070`, `LNK1327`, release run 30624906073) because line 17 uses `--` as a prose dash INSIDE an XML comment, which XML forbids. Rephrase every prose `--` inside those comments (comma, colon, parentheses or two sentences; not a spaced hyphen), checking the WHOLE file since the parser stops at the first error. Do NOT delete the comments: the comctl32 identity explanation, the dark-mode caveat and the per-monitor-v2 rationale are load-bearing and were approved. Then ADD A GUARD in the Ubuntu gate asserting the manifest is well-formed, because nothing in this repo parses that file today (the gate cannot link a Windows binary, and the existing shape test pins identity strings only) — prefer a dependency-free check over adding an XML dependency for one file, and say which you chose. Finally re-verify by DISPATCHING `gh workflow run release.yml --ref <your branch>` (legal: the workflow is on main, per CONTEXT.md), confirm `windows-desktop-app` is green with its artifact, and record the run URL. Nothing else about the leg changes.
