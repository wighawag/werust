# The Windows app manifest was malformed XML, and nothing in this repo parsed it

Task: `windows-app-manifest-is-malformed-xml-and-breaks-the-release-link`. Companion to `docs/spikes/windows-release-packaging-leg/README.md`, whose manifest this fixes; nothing else about that leg changes.

## The failure

[Release run 30624906073](https://github.com/wighawag/werust/actions/runs/30624906073), job `windows-desktop-app`, step "Build werust-windows (release, x86_64-pc-windows-msvc)":

```
\\?\D:\a\werust\werust\crates\werust-windows\app.manifest : general error c1010070:
    Failed to load and parse the manifest. Windows was unable to parse the requested XML data.
LINK : fatal error LNK1327: failure during running mt.exe
error: linking with `link.exe` failed: exit code: 1327
```

`crates/werust-windows/app.manifest` line 17 used `--` as a prose dash inside an opening `<!-- ... -->` block. XML forbids a double hyphen anywhere in a comment (XML 1.0 §2.5), full stop, so `mt.exe` was right to refuse and `link.exe` died behind it. Every other job in that run was green: the leg's sibling decoupling worked, and only the Windows artifact was lost.

The dash is this repo's ordinary house style (it deliberately avoids the em dash character), which is exactly why this was invisible: the punctuation is correct everywhere in the repo except in the one file that is XML.

## The fix

Three prose double hyphens, all inside the two comment blocks, rephrased in place. Nothing was deleted: the comctl32 identity explanation, the "what it does NOT buy: dark mode" caveat and the per-monitor-v2 DPI rationale are load-bearing and were reviewed and approved.

1. The comctl32 identity note became two sentences (`… + language). A wrong token silently leaves …`).
2. The DPI consequence became a parenthetical (`… on a 200% display (crisp, but small), where an unaware one …`).
3. The assembly-version note became a comma (`… this repo keeps removing, and could not represent it anyway …`).

The whole file was checked, not just line 17: a parser stops at its first error, so a second occurrence would have come back as a fresh red on the next tag. The manifest also gained a short EDITING note at the top, saying the double hyphen is illegal here and naming the guard, so the next editor meets the rule before writing the dash rather than after a release job dies.

## The guard, and why it is hand-written

`mt.exe` is the only thing that parsed this file, and it runs on a Windows runner at LINK time, on a tag. The Ubuntu `verify` gate cannot link a Windows binary, and the existing shape assertions pin identity STRINGS (`Microsoft.Windows.Common-Controls`, `PerMonitorV2`, …) which a malformed file satisfies just as happily. So the gate had no way to notice, and the feedback loop for a comment edit was "wait for a release".

The gate now PARSES the file: `the_windows_app_manifest_is_well_formed_xml` in `crates/werust-core/tests/release_plumbing_shape.rs`, on a dependency-free scanner in that same file.

**Decision: a hand-written scanner, NOT an XML crate.** What it touches: `crates/werust-core`'s dev-dependencies, and any later "we need XML somewhere else" choice.

- *Considered:* `quick-xml` (or `roxmltree`) as a dev-dependency of `werust-core`. Rejected: this workspace argues for every dependency by name in `Cargo.toml`, and a new parsing lineage carried by one crate's test suite, to read ONE 60-line file that no product code ever touches, is not a trade this repo makes lightly. The task itself asked for the dependency-free option to be preferred.
- *Chosen:* ~200 lines of scanner beside the test. It understands exactly the vocabulary an application manifest is made of (the XML declaration, comments, CDATA, elements with quoted attributes and self-closing tags, character data, entity/character references) and REFUSES anything outside it, loudly, rather than skipping it. That refusal is the point: a scanner that silently ignores a construct it does not parse would bless a malformed file, which is the exact failure being fixed. A `<!DOCTYPE …>` in this manifest therefore reds the gate with a message saying the guard does not parse it; if one is ever genuinely needed, that is the moment to reconsider the crate.
- *Not a validator.* It says nothing about namespaces, schemas, or whether `mt.exe` likes the CONTENT. It holds only the property that was violated: well-formedness.
- *It has teeth, provably.* `the_manifest_guard_has_teeth` feeds it the shape that actually cost run 30624906073 plus eight other ways this file could break (unterminated comment, a comment ending in a hyphen, mismatched/unclosed tags, an unclosed start tag, a run-on attribute value, a bare `&`, two roots, no root) and asserts every one is REJECTED, then asserts the ordinary manifest vocabulary is ACCEPTED. A guard whose failure nobody has ever seen is not a guard.

Cross-checked against a real parser while landing this: `python3 -c "import xml.dom.minidom; xml.dom.minidom.parse('crates/werust-windows/app.manifest')"` (expat) agrees the file is now well-formed, and reported the same line 17 before the fix.

## Re-verification by dispatch

**Status: NOT YET OBTAINED by the build agent, and it cannot be from here.**

The task asks for `gh workflow run release.yml --ref <branch>`, and dispatch-by-ref is legal because `release.yml` is on `main` (`CONTEXT.md`, "A CI-measurable criterion needs its CI LEG on `main` FIRST"). But the ref must EXIST on the remote, and the build agent may not perform git operations on this repo (no commit, no push): the runner owns every git-state transition, so at the time this change was written the branch existed only locally and `origin` carried nothing but `main`.

Per that same convention's corollary ("when a criterion still ends up unmeasured, obtaining the measurement is the CONDUCTOR's job, not the build agent's"), the dispatch is left to whoever pushes the branch. The exact command, once the branch is on `origin`:

```
gh workflow run release.yml --ref work/task-windows-app-manifest-is-malformed-xml-and-breaks-the-release-link
gh run list --workflow release.yml --limit 1
```

Expected: `windows-desktop-app` GREEN, uploading `werust-windows-x86_64-unsigned.zip` (the dry-run path uploads a workflow artifact and publishes no release).

## Dispatched run (conductor, 2026-07-31) — MEASURED, and it is green

[Run 30626912474](https://github.com/wighawag/werust/actions/runs/30626912474), `release.yml`, `workflow_dispatch` on `main` (the fix having landed there). **All six jobs SUCCESS**, and the job this task exists for is among them:

| job | result |
| --- | --- |
| `verify` | success |
| `goreleaser` | success |
| `android-apk` | success |
| `ios-simulator-app` | success |
| `macos-desktop-app` | success |
| **`windows-desktop-app`** | **success** |

Artifacts uploaded, including `werust-windows-desktop-app` at **1,986,546 bytes** — the first Windows artifact this project has ever produced. So the manifest is not merely well-formed by two local parsers; `mt.exe` accepted it, `link.exe` embedded it, and the zip exists.

The previous run, [30624906073](https://github.com/wighawag/werust/actions/runs/30624906073), is the recorded failure this task was cut from: the identical pipeline with every other job green and `windows-desktop-app` dead at `LNK1327`. The pair is the before/after, and it is why the well-formedness guard added here is worth its few lines.

What IS established locally without a runner: the manifest parses (two independent parsers, one of them expat), and the full `verify` gate is green.
