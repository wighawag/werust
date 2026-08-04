# The release notes lost the conventional-commit changelog to a race

Task: `release-notes-lose-the-conventional-commit-changelog-to-a-race`. Files changed: `.goreleaser.yaml`, `.github/workflows/release.yml`, `crates/werust-core/tests/release_plumbing_shape.rs` (criterion 12), `CONTEXT.md` (the standing rule), `crates/werust-core/Cargo.toml` (one dev-dependency). One observation note is discharged by deletion: `work/notes/observations/goreleaser-leg-is-not-idempotent-on-rerun-2026-07-31.md`, whose signal now lives in "The sibling defect" below.

`CONTEXT.md`'s Conventions promise that "releases generate their changelog FROM this git history", and `.goreleaser.yaml` implements exactly that (`changelog.use: git`, grouped Features / Bug fixes / Others). None of it was reaching the release page. This is what was wrong, what was changed, and the one thing that can only be confirmed on a real tag.

## Two defects, one file each

**Defect 1: a race for who CREATES the Release.** Each of the four artifact legs (`android-apk`, `ios-simulator-app`, `macos-desktop-app`, `windows-desktop-app`) is decoupled from the desktop leg by `needs: verify`, so a desktop failure can never withhold a mobile artifact. To make that safe each one guaranteed the Release existed before uploading into it, with `gh release create "$tag" --generate-notes 2>/dev/null || true`. On a tag, whichever leg got there first CREATED the Release and `--generate-notes` filled the body with GitHub's own auto-notes. GoReleaser then found a Release that already existed and attached its artifacts without writing the changelog it had prepared. `v0.3.1` published a body that is one compare link; `v0.3.0`, the release that added macOS and Windows across 132 commits, published one PR title.

**Defect 2: the exclude filters were scope-blind.** The changelog GROUP regexps always knew about scopes (`'^.*?feat(\(.+\))??!?:.+$'`), but the excludes were bare `^chore:` / `^ci:` / `^test:` / `^docs:`, and this repo scopes nearly every housekeeping commit. Measured, not guessed (see the numbers below): over `v0.3.2..main` those four patterns removed **one** commit out of 77. So even when GoReleaser DID win the race and write the body (it did on `v0.3.2`), the body was noise.

## What changed

1. **The four legs create the Release EMPTY-bodied**: `gh release create "$tag" --notes "" 2>/dev/null || true`. The `--notes ""` is not decoration: `gh release create` needs a body flag to run non-interactively at all, and an empty one is the placeholder GoReleaser then fills. The decoupling (`needs: verify`) and the idempotent, non-fatal `|| true` are untouched: three of the four legs lose the race on every tag, and losing it is the normal case.
2. **`.goreleaser.yaml` sets `release.mode: replace`**, so the body of a pre-existing Release is GoReleaser's to write regardless of what put text there.
3. **`.goreleaser.yaml` sets `release.replace_existing_artifacts: true`**, which fixes the recorded sibling defect (the section below).
4. **The changelog excludes are scope-aware and drop the runner's bookkeeping commits.** Each housekeeping pattern now mirrors the groups' own `(\(.+\))??!?:` shape rather than inventing a second convention, and two new patterns remove the `work/`-contract and `dorfl` lifecycle subjects that no group regexp would ever have claimed.

## GoReleaser's behaviour on a pre-existing Release: CHECKED, not assumed

The task said to confirm this rather than assume it. Both halves are from GoReleaser's own source and docs, at the versions the workflow pins (`goreleaser-action@v6`, `version: "~> v2"`):

- `release.mode` defaults to **`keep-existing`** ([customization/release](https://goreleaser.com/customization/release/), and `pkg/config/config.go`: `jsonschema:"...,default=keep-existing"`). The docs state the consequence outright: *"If you create the release before running GoReleaser, and the said release has some text in its body, GoReleaser will not override it with its release notes, unless you configure it to do so (e.g. `mode: replace`)."*
- The implementation is `internal/client/release_notes.go`:

  ```go
  func getReleaseNotes(existing, current string, mode config.ReleaseNotesMode) string {
      switch mode {
      case config.ReleaseNotesModeAppend:  return existing + "\n\n" + current
      case config.ReleaseNotesModeReplace: return current
      case config.ReleaseNotesModePrepend: return current + "\n\n" + existing
      default:                             // keep-existing
          if existing != "" { return existing }
          return current
      }
  }
  ```

  called from `createOrUpdateRelease` in `internal/client/github.go` on the update path (the path taken whenever a leg created the Release first).

Two things follow, and they are why the change is BOTH knobs and not either one:

- Dropping `--generate-notes` alone WOULD be sufficient today: with an empty existing body, even the default `keep-existing` falls through to GoReleaser's notes. But it is sufficient by accident. The changelog would survive only as long as nobody, no retry and no future leg ever puts any text there first, which is precisely the implicit coupling that caused this defect.
- Setting `mode: replace` alone would also work, but would leave every Release momentarily carrying auto-notes, and would leave `--generate-notes` in the file looking deliberate.

**The named cost of `mode: replace`:** a human edit to a published body is overwritten if that tag's workflow is ever re-run. That is the correct trade for a derived changelog, and it is why the `v0.3.0` backfill below is done by hand on an old tag rather than by re-firing its workflow. (Re-firing it would need a force-pushed tag, which the task rightly forbids.)

## The sibling defect: the goreleaser leg was not idempotent on re-run

Same file, same area, same class (what happens when a release step runs twice). This section is the durable home of a signal that was captured in `work/notes/observations/goreleaser-leg-is-not-idempotent-on-rerun-2026-07-31.md`; that note is discharged (deleted) by this change, per the `work/` contract's deletion-only note lifecycle.

**What was observed.** `v0.2.9`, the last real release before `v0.3.0`, is RED. Both its runs, [30464954386](https://github.com/wighawag/werust/actions/runs/30464954386) and [30465110439](https://github.com/wighawag/werust/actions/runs/30465110439), ended in `goreleaser failure` while `verify`, `android-apk` and `ios-simulator-app` all succeeded. It is not a build failure:

```
upload failed  error=POST .../releases/361853856/assets?name=checksums.txt:
               422 Validation Failed [{Resource:ReleaseAsset Field:name Code:already_exists}]
upload failed  error=POST .../releases/361853856/assets?name=werust_0.2.9_linux_amd64.tar.gz:
               422 Validation Failed [{Resource:ReleaseAsset Field:name Code:already_exists}]
⨯ release failed after 43s
```

**The mechanism.** GoReleaser refuses to overwrite an asset that is already attached, so the first attempt got far enough to upload the tarball and the checksums, and every re-run after that is guaranteed to fail on the assets its own predecessor uploaded. The four artifact legs were made idempotent on purpose (`gh release upload --clobber`), precisely so a re-run or a decoupled leg cannot fail on state a sibling created; the desktop Linux leg is the one that never got that treatment, and it is the one that is red. Why that matters beyond one tag: a permanently red release run stops being read, which is how the next real failure gets missed.

**The fix.** `release.replace_existing_artifacts: true`. Per GoReleaser's docs it is scoped exactly to this case: on a 422 it fetches the release's asset list, deletes the one whose name collides, and retries the upload. Nothing else in the leg changes, and there is no cost on a first attempt.

**What this does and does not do.** It makes future re-runs green. It does not retroactively turn `v0.2.9`'s runs green: those are historical records. Re-running that workflow today would now succeed, but nothing forces that, and it is not worth a tag push.

**Not verifiable on the dry run either**, for the same reason as everything else here: `--snapshot` uploads nothing to the forge, so no asset can collide. It is confirmed by a second run of a real tag build (check 4 in the next-real-tag list below).

## Alternatives considered

| | Why not |
|---|---|
| **Chosen: legs create an empty-bodied Release, GoReleaser owns the body (`mode: replace`).** | Keeps the decoupling exactly as landed, honours the stated convention, adds no new failure mode, and is two config lines. |
| **GoReleaser owns creation; the artifact legs only ever upload, retrying until the Release appears.** | Restores a soft ordering without a hard `needs:`, but pays for it with a retry loop in four legs and a new failure mode: if the desktop leg is red, no Release ever appears and the mobile artifacts are withheld after all, which is the exact coupling `fix-release-native-x86-desktop-and-decouple-mobile` was landed to remove. |
| **Accept GitHub's auto-notes and delete the changelog config**, updating `CONTEXT.md`. | Honest but strictly worse: the point of the load-bearing conventional-commit rule is that the changelog is DERIVED rather than hand-written, and auto-notes are PR-based, which reads as one line per merge in a repo whose work lands as commits. |
| For defect 2: **a second, bespoke filter convention** (e.g. `(?i)^(chore\|ci)\b`). | Rejected in favour of mirroring the groups' existing `(\(.+\))??!?:` shape, per the task and the repo's coherence rule: one spelling for "a conventional-commit type, scoped or not". |

## What the dry run CANNOT prove, and what to check on the next real tag

The `workflow_dispatch` dry run runs GoReleaser with `--snapshot`, which **never touches the forge**. No Release is created, no body is written, and `release.mode` / `replace_existing_artifacts` are not exercised at all. A green dispatch therefore says nothing whatsoever about the release notes, and nobody should read it as confirmation. The same is true of the `verify` gate: `crates/werust-core/tests/release_plumbing_shape.rs` pins the CONFIGURATION and the FILTER BEHAVIOUR, not the forge's response to it.

On the next real tag (`v0.3.3` or later), read the published page and check:

1. **The body is GoReleaser's**, i.e. it starts with `## Changelog` and carries `### Features` / `### Bug fixes` sections with `* <sha> <subject>` lines. GitHub's auto-notes look nothing like it: they are `## What's Changed` with PR titles, and/or a bare `**Full Changelog**: …/compare/…` link.
2. **Nothing in the body is bookkeeping**: no `chore(...)`, `ci(...)`, `docs(...)`, `task:`, `notes(...)`, `surface task:` line anywhere, and ideally no `### Others` section at all.
3. **Every `feat(...)` / `fix(...)` in `git log <previous-tag>..<tag>` appears**, under the right heading. Run `docs/spikes/release-notes-lose-the-conventional-commit-changelog-to-a-race/check-changelog-filters.sh <previous-tag>..<tag>` and compare it with the page.
4. **All five jobs are green, including `goreleaser`**, which is the `replace_existing_artifacts` half. If the run needs a re-run for any reason, the SECOND run must also be green; a `422 already_exists` on `checksums.txt` or the tarball means the knob did not take.

## The measurement (defect 2), reproducible

`check-changelog-filters.sh` reads the patterns out of `.goreleaser.yaml` (never a second copy) and applies them to `git log --format=%s <range>` the way GoReleaser does: excludes first, then each surviving subject to the first group, in config order. It matches with `grep -P`, because the patterns use the lazy `??` quantifier that RE2 has and POSIX ERE does not. `--legacy` classifies with the scope-blind set this task replaced, so the before/after is reproducible rather than asserted.

```
$ ./docs/spikes/release-notes-lose-the-conventional-commit-changelog-to-a-race/check-changelog-filters.sh v0.3.2..HEAD --legacy
range:    v0.3.2..HEAD  (77 commits)
filters:  the SCOPE-BLIND set this task replaced

EXCLUDED (a pattern removed it; it never reaches a release page)
     0  ^chore:
     0  ^ci:
     0  ^test:
     1  ^docs:
     0  Merge
     1  TOTAL EXCLUDED

  76 of 77 commits published, 1 filtered out
```

```
$ ./docs/spikes/release-notes-lose-the-conventional-commit-changelog-to-a-race/check-changelog-filters.sh v0.3.2..HEAD
range:    v0.3.2..HEAD  (77 commits)
filters:  .goreleaser.yaml

EXCLUDED (a pattern removed it; it never reaches a release page)
     7  ^chore(\(.+\))??!?:
     0  ^ci(\(.+\))??!?:
     0  ^test(\(.+\))??!?:
    23  ^docs(\(.+\))??!?:
     0  ^(tasking|task|notes|obs|findings|spec|review)(\+[a-z]+)?(\(.+\))??!?:
    30  ^surface task:
     0  ^dorfl
     0  Merge
    60  TOTAL EXCLUDED

PUBLISHED (what the release body would say)

  ### Features (17)
    ...
  ### Bug fixes (0)
  ### Others (0)

  17 of 77 commits published, 60 filtered out
```

76 noise lines against 17 features becomes 17 features and nothing else. `### Others` is empty, which is the real test: `Others` is the catch-all, so anything the filters miss lands there in public.

Two patterns show 0 over this range because the range contains none of their subjects; they are not dead. Over the full history (`4b825dc..HEAD`, 620 commits) the same run gives `^ci` 2, `^(tasking|task|…)` 136, `^dorfl ` 1, `Merge ` 1, `^surface task:` 190, so 473 excluded and 147 published (141 Features, 6 Bug fixes, 0 Others). And over `v0.3.1..v0.3.2`, the two commits that produced `v0.3.2`'s noisy body (`ci(android-apk): …` and `task: …`) are now both excluded, where the legacy filters excluded neither.

**Cross-checked against the engine GoReleaser actually uses.** The script matches with `grep -P` and the gate test with the Rust `regex` crate; GoReleaser compiles these with Go's `regexp`, which is RE2. A throwaway Go program (built outside the repo and deleted; `regexp.Compile` each pattern, then classify the same `git log --format=%s` input) accepts all eight patterns as valid RE2 and returns the identical counts, on both `v0.3.2..HEAD` (7 / 0 / 0 / 23 / 0 / 30 / 0 / 0, 60 of 77) and the full history (116 / 2 / 0 / 27 / 136 / 190 / 1 / 1, 473 of 620). Three independent engines, same answer, so the numbers above are not an artefact of the one used to produce them.

The gate-side half of the same check is `the_configured_filters_sort_real_history_the_way_the_release_page_should_read` in `crates/werust-core/tests/release_plumbing_shape.rs`, which COMPILES the config's patterns and runs them over a corpus of real subjects, one per distinct shape, on every `cargo test`. Reading a filter is what let defect 2 ship; the test runs it.

## Known limits, recorded rather than hidden

- **Duplicate subjects are not de-duplicated.** A `dorfl requeue` continuation commits again under the SAME task title, so `v0.3.2..main` contains three identical `feat(shortcuts-and-mouse-history-buttons-on-the-macos-edge): …` subjects (and two each of three others): 17 `feat` commits under 12 distinct subjects. GoReleaser has no de-duplication option (`filterEntries` only excludes and includes by regexp, and `formatEntries` renders one line per commit), so the only ways to collapse them are `--release-notes <file>` (which replaces the whole generated changelog with a hand-made one, abandoning the derived-changelog convention) or a `format` without `{{ .SHA }}` (which would make the duplicate lines byte-identical, i.e. worse, not better). Shipped as a **cosmetic limit**: `sort: asc` sorts by subject, so duplicates land adjacent and read as a repetition rather than as scattered noise, and the leading short SHA distinguishes them. The real fix is upstream of the changelog: fewer requeue-continuation commits reaching `main`.
- **A range that is all bookkeeping now yields an EMPTY body.** `v0.3.1..v0.3.2` is two commits, both noise, so the new filters publish `## Changelog` and nothing under it. That is honest (nothing user-facing shipped in `v0.3.2`) and is preferable to the noise it replaces, but it means an empty section is not evidence that the filters are broken.
- **The filters trust the commit TYPE.** `fix(backlog): rewrite versioned-gtk-app-id task …` is a task-file edit typed as a `fix`, so it appears under Bug fixes in `v0.2.9..v0.3.0`. No filter can distinguish it; the conventional-commit convention (`CONTEXT.md`) is what has to hold.

## The `v0.3.0` backfill (prepared, NOT applied)

`v0.3.0`'s published body is a single PR title, which is a poor public record of the release that added macOS and Windows. Editing a published release body changes no artifact and re-triggers nothing, so the backfill is safe. But the tag annotation must NOT be rewritten, since force-pushing a tag re-fires the release workflow.

`v0.3.0-release-notes.md` in this directory is the body `.goreleaser.yaml`'s CURRENT rules produce for `v0.2.9..v0.3.0` (41 entries: 39 Features, 2 Bug fixes, 0 Others), rendered in GoReleaser's own shape (`## Changelog`, `### <group>`, `* <sha> <subject>`, sorted ascending by subject, empty groups omitted). It is paste-ready.

It is deliberately **not applied here**: editing a published GitHub Release is a mutation of the forge, outside this repo, that no gate can review and no commit can revert. A human applies it with:

```sh
gh release edit v0.3.0 --notes-file docs/spikes/release-notes-lose-the-conventional-commit-changelog-to-a-race/v0.3.0-release-notes.md
```

`v0.3.1` could be backfilled the same way (`check-changelog-filters.sh v0.3.0..v0.3.1` shows what it should say); it is a two-commit range, so there is much less to gain.

## Decisions

1. **Both knobs, not one** (`--notes ""` on the legs AND `release.mode: replace`). Alternative: rely on the empty body alone, since `keep-existing` falls through when the existing body is empty. Rejected because that makes the convention hold by accident. Touches: all four artifact legs and `.goreleaser.yaml`. Cost, named above: a hand-edited body is overwritten on a re-run of that tag.
2. **`replace_existing_artifacts: true` rather than a `gh`-shaped delete-then-upload step** for the re-run idempotency defect. It is GoReleaser's own vocabulary for the guarantee the other four legs already have (`gh release upload --clobber`), it is scoped to the 422 case (the docs note it costs an extra API round trip only on collision), and it adds no step to the workflow. Touches: the `goreleaser` leg only.
3. **The bookkeeping excludes cover the whole `work/`-contract prefix family**, not just `surface task:`. The task named `surface task:` and `task:`; the same range also carries `notes(...)`, `obs(...)`, `findings(...)`, `spec(...)`, `review(...)`, `tasking(...)` and the `+` compounds (`task+notes:`, `findings+task:`), which are the same class (an ITEM moved between status folders) and would all have landed in `### Others`. One pattern covers them, spelled in the same scope-aware shape. This is a USER-VISIBLE default (it decides what a release page says), so it is recorded rather than buried. Reversing it is one line. Touches: nothing outside `.goreleaser.yaml`; it cannot reach a `feat`/`fix` subject, and `no_exclude_filter_can_eat_a_feature_or_a_fix` is the guard that keeps it that way.
4. **`regex` added as a DEV-dependency of `werust-core`** so the shape test can APPLY the configured patterns instead of eyeballing them. Alternative: assert the pattern STRINGS, which is what "the filter looks right" already proved insufficient. GoReleaser compiles these with Go's `regexp` (RE2); the Rust `regex` crate implements the same syntax family, and the patterns stay inside the common subset. Dev-only, exactly like the existing `serde_yaml` and `toml` dev-dependencies in the same crate, for the same test file.
5. **The `v0.3.0` backfill is prepared, not applied** (see above). Alternative: run `gh release edit` from the build. Rejected: a forge mutation from an autonomous build is unreviewable and unrevertable by the gate that is supposed to check this work.
