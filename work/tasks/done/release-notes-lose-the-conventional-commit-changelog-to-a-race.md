---
title: "The published release notes are GitHub's auto-notes, not the conventional-commit changelog the repo promises — an artifact leg wins the race and creates the Release first"
slug: release-notes-lose-the-conventional-commit-changelog-to-a-race
blockedBy: []
covers: []
---

## What to build

Found by the conductor while cutting v0.3.1 on 2026-07-31, by reading the published release body rather than assuming it.

## The defect

`CONTEXT.md`'s Conventions say, as a load-bearing rule: *"Releases generate their changelog FROM this git history — there are no per-change changeset files."* `.goreleaser.yaml` implements exactly that, carefully: `changelog.use: git`, `sort: asc`, excludes for `chore:`/`ci:`/`test:`/`docs:`, and groups titled **Features** / **Bug fixes** / **Others**.

**None of it reaches the release page.** The published bodies are GitHub's own auto-generated notes:

- `v0.3.1`: the body is one line, `**Full Changelog**: .../compare/v0.3.0...v0.3.1` — no Features section, though the range contains two `feat(...)` commits that the configured regexp matches.
- `v0.3.0`: `## What's Changed` listing a single PULL REQUEST (the CI-vehicle PR #2), which is GitHub's PR-based format, not GoReleaser's commit-grouped one — and a wildly misleading summary of a release that added two desktop platforms across 131 commits.

**The cause is a race, and it is a side effect of a deliberately good decision.** Each of the four artifact legs (`android-apk`, `ios-simulator-app`, `macos-desktop-app`, `windows-desktop-app`) is decoupled from the desktop leg by `needs: verify`, so a desktop failure cannot withhold a mobile artifact. To make that safe, each one guarantees the Release exists before uploading:

```sh
gh release create "${GITHUB_REF_NAME}" --generate-notes 2>/dev/null || true
```

On a tag, whichever job reaches that line first CREATES the Release, and `--generate-notes` fills the body with GitHub's auto-notes. GoReleaser then finds a Release that already exists and attaches its artifacts to it, leaving the body it would have written unused.

The decoupling is right and must stay — it is the fix `fix-release-native-x86-desktop-and-decouple-mobile` landed deliberately. The bug is `--generate-notes` on a create that can win a race it was never meant to win.

## What to build

Work out, and RECORD, which of these the repo wants, then implement it:

1. **Create the Release with an EMPTY placeholder body and let GoReleaser fill it.** Drop `--generate-notes` from the four legs (`--notes ""` or equivalent) and confirm GoReleaser actually WRITES the body of a pre-existing release rather than skipping it — check its `release.mode` semantics (`keep-existing` versus `replace`) and set what is needed. This preserves the decoupling and honours the stated convention. Likely the right answer; verify rather than assume.
2. **Let GoReleaser own creation and have the artifact legs only ever upload**, retrying briefly if the Release is not there yet. Restores a soft ordering without a hard `needs:` dependency, at the cost of a retry loop.
3. **Accept GitHub's auto-notes and delete the changelog config**, updating `CONTEXT.md`'s convention to match reality. Honest, and much worse: the whole point of the conventional-commit rule is that the changelog is derived rather than hand-written.

**Verify by dispatching, not by reasoning.** The `workflow_dispatch` dry run runs GoReleaser in `--snapshot` mode, which never touches the forge, so it CANNOT prove this. Say so plainly, and state how the fix will be confirmed on the next real tag (and what to check on the release page).

**While in this code, fix the sibling defect already recorded:** `work/notes/observations/goreleaser-leg-is-not-idempotent-on-rerun-2026-07-31.md`. `v0.2.9` is still red because a re-run fails with `422 already_exists` on assets its own first attempt uploaded, while the four artifact legs are idempotent by design. Same file, same area, same class (what happens when a release step runs twice).

**Consider backfilling `v0.3.0`'s notes** as part of this, since "one CI-vehicle PR" is a poor public record of the release that added macOS and Windows. Editing a published release body changes no artifact and re-triggers nothing, so it is safe; the tag annotation itself should NOT be rewritten (force-pushing a tag would re-fire the release workflow into GoReleaser's non-idempotency).


## SECOND DEFECT (added 2026-08-04 by the drive-tasks conductor): the exclude filters are not scope-aware

Found while assessing release-readiness after the ten chrome tasks landed. Even when GoReleaser DOES win the race and write the body (it did on `v0.3.2`), the body is dominated by noise, because `.goreleaser.yaml`'s changelog filters are inconsistent with its own groups:

- The GROUP regexps ARE scope-aware: `'^.*?feat(\(.+\))??!?:.+$'` matches `feat(some-scope):`.
- The EXCLUDE filters are NOT: they are bare `"^chore:"`, `"^ci:"`, `"^test:"`, `"^docs:"`, which do NOT match `chore(scope):`, `ci(scope):`, `docs(scope):`.

This repo writes almost every housekeeping commit WITH a scope, so the excludes essentially never fire. Measured over the 76 commits in `v0.3.2..main`:

| exclude pattern | commits it actually matches |
|---|---|
| `^chore:` | 0 (all 7 are `chore(scope):`) |
| `^ci:`    | 0 |
| `^test:`  | 0 |
| `^docs:`  | 1 of 22 |

The visible consequence on `v0.3.2`'s published body is a `### Others` section whose entries are `ci(android-apk): ...` and `task: ...` — exactly the commits the filters were written to remove.

A THIRD source of noise is specific to the runner: `dorfl`'s surface-retry duplication (see `work/notes/observations/dorfl-bounce-surface-duplicates-the-stuck-question-and-spams-main-2026-08-04.md`) pushes batches of `surface task:<slug> (stuck): ...` commits to `main`. There are ~30 of them in `v0.3.2..main`. They are pure runner bookkeeping and have no place in a user-facing changelog.

Cutting a tag today would therefore publish roughly 58 noise lines against 17 `feat` lines.

## Additional acceptance criteria (from the second defect)

- [ ] The changelog exclude filters are SCOPE-AWARE, so `chore(x):`, `ci(x):`, `docs(x):` and `test(x):` are excluded exactly as their unscoped forms are. Mirror the groups' own style (e.g. `'^chore(\(.+\))??!?:'`) rather than inventing a second convention, and keep the unscoped forms working.
- [ ] The runner's bookkeeping commits are excluded: `surface task:...` (and any equivalent `dorfl` lifecycle subject that reaches `main`) must not appear in a released changelog.
- [ ] VERIFY the filters against real history rather than by eye: demonstrate, for the `v0.3.2..main` range, that the excluded categories drop out and every `feat(...)` / `fix(...)` survives. A tiny check or a recorded command output is fine; do not claim it untested.
- [ ] Note the DUPLICATE-SUBJECT effect and decide whether to act: a `dorfl requeue` continuation commits again under the SAME task title, so `v0.3.2..main` contains e.g. three identical `feat(shortcuts-and-mouse-history-buttons-on-the-macos-edge): ...` subjects. If they can be de-duplicated in the changelog cleanly, do it; if not, record it as a known cosmetic limit rather than silently shipping it.

## Acceptance criteria

- [ ] A tagged release's body is the conventional-commit changelog GoReleaser is configured to produce (grouped Features / Bug fixes), not GitHub's auto-notes.
- [ ] The four artifact legs stay decoupled (`needs: verify`), and a desktop-leg failure still cannot withhold a mobile artifact.
- [ ] The chosen approach is recorded against the alternatives, including whatever was found about GoReleaser's behaviour on a pre-existing release.
- [ ] The record states plainly that a `--snapshot` dry run cannot verify this, and names what to check on the next real tag.
- [ ] The goreleaser re-run idempotency defect is fixed or explicitly scoped out with a reason.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Prompt

> Goal: the published release notes are GitHub's auto-generated notes, not the conventional-commit changelog `CONTEXT.md` promises and `.goreleaser.yaml` configures (grouped Features/Bug fixes). Cause: each of the four artifact legs runs `gh release create "${GITHUB_REF_NAME}" --generate-notes 2>/dev/null || true` to guarantee the Release exists before uploading (a deliberate part of decoupling them from the desktop leg), so whichever wins the race creates the Release with GitHub's notes, and GoReleaser then attaches artifacts to an existing Release without writing the body it prepared. Evidence: `v0.3.1`'s body is only a compare link despite two `feat(...)` commits in range, and `v0.3.0`'s is a single PULL REQUEST — a poor record of a release that added two desktop platforms. KEEP the decoupling (`needs: verify`); it was landed deliberately. Prefer dropping `--generate-notes` so the legs create an empty-bodied Release and GoReleaser fills it — but CONFIRM GoReleaser writes the body of a pre-existing release (check `release.mode`, `keep-existing` versus `replace`) rather than assuming, and record the alternatives (GoReleaser owns creation with the legs retrying; or accept auto-notes and change the convention). Note plainly that the `--snapshot` dry run cannot verify this because it never touches the forge, and say what to check on the next real tag. While in the same file, fix the recorded sibling defect (`goreleaser-leg-is-not-idempotent-on-rerun-2026-07-31.md`: `v0.2.9` is red because a re-run hits `422 already_exists` on its own first attempt's assets). Backfilling `v0.3.0`'s release body is safe and worth doing; do NOT rewrite the tag annotation, since force-pushing a tag re-fires the release workflow.
