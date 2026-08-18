---
title: "A Gate-2 bounce surfaced the SAME stuck question five times and pushed five commits to main, while reporting that the surface had not landed"
date: 2026-08-04
status: open
kind: observation
tool: dorfl 0.11.1
noticedDuring: drive-tasks conductor run over the ten chrome-conventional-controls tasks
---

Noticed while driving `enable-the-ios-back-forward-swipe-gesture` (the drive-tasks conductor run, dorfl `0.11.1`, `do --isolated --review --merge`).

## What happened

Gate 2 blocked the task. The runner then tried to surface the block as a `work/questions/` sidecar, and its retry loop misfired:

```
>> push reported up-to-date / no change of our making — origin/main is not our commit — treating as rejected.
>> main advanced under us — surface refetch and retry (1/5)...
   ... (repeated through 5/5)
>> surface for 'task:enable-the-ios-back-forward-swipe-gesture' did not land on origin/main
   (item missing on main, or contention exhausted after retries).
```

Two things are wrong with that.

1. **The surface DID land — five times.** `main` carries five separate `surface task:enable-the-ios-back-forward-swipe-gesture (stuck): ...` commits, and the resulting `work/questions/task-enable-the-ios-back-forward-swipe-gesture.md` contained **five identical questions** (`Q1`..`Q5`), each the verbatim Gate-2 block text, each with its own empty "Your answer" slot. So the retry loop re-appended the question every round while concluding it had never landed at all.
2. **Each retry pushed to `main`,** so the five commits each triggered a full `verify` CI run (five green runs, ~2-3 min each) for what should have been one sidecar write.

The "no change of our making — origin/main is not our commit" test looks like the culprit: the push evidently succeeded, but the runner judged the result "not ours" (plausibly because it compares against its own expected head and `main` had legitimately moved), treated it as rejected, and retried, appending again each time.

## Why it matters

- A human answering that sidecar would face five copies of one question and would have to guess whether answering `Q1` sufficed.
- Worse for the autonomous path: the sidecar is written with `allAnswered=false`, so a later `advance`/`run` leg reading it sees an item with five unanswered questions **even after the task itself has been completed and merged**. Nothing garbage-collects the sidecar when the task later lands, so it under-reports progress and can strand an item that is actually done.
- The CI spam is a real cost on a repo whose macOS/iOS/Windows legs are the only evidence those platforms ever get.

## What I did about it here

The task was recovered with `requeue` (keep + continue) and a precise handoff, rebuilt, approved by Gate 2, and merged. Because the task is now in `work/tasks/done/`, its stale five-question sidecar was removed in the same commit as this note: it described an already-resolved block and would otherwise have read as five open questions against a completed task.

## Not fixed here

This is a dorfl runner behaviour, not a werust one, so nothing in this repo can fix it. Recorded rather than acted on. If it recurs, the two things to look at are the "is this push ours" comparison in the surface retry loop, and whether a completed item should reap its own `work/questions/` sidecar.

## Update 2026-08-17: recurrence, five duplicates, plus a backlog-driven wrinkle

Recurred on dorfl 0.11.1 while driving `task:pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns` from `work/tasks/backlog/` (`do --isolated --allow-backlog`). The gate failed for an ENVIRONMENTAL reason (the fresh-worktree gate built in `/tmp`, a 16G tmpfs, and hit `No space left on device`), so the bounce path ran.

What was observed, which sharpens the original signal:

1. The surface pushed FIVE times (`d0d169d`, `8167bc0`, `7fd6ff0`, `3c507cd`, `1c900cb` on `main`), one per retry, each landing a commit.
2. Every push was reported as REJECTED to the operator: `push reported up-to-date / no change of our making — origin/main is not our commit — treating as rejected`, followed by `main advanced under us — surface refetch and retry (n/5)`. So the retry loop was driven by a false negative: the pushes were landing while being reported as not landing. The post-detection ("is origin/main OUR commit?") appears to be what misfires, and it misfires deterministically once the first push succeeds, because from then on `main` genuinely is a commit the runner made but does not recognise as such.
3. The final message claimed `surface ... did not land on origin/main (item missing on main, or contention exhausted after retries)` when the surface HAD landed: `needsAnswers: true` was set on the body and a 156-line `work/questions/task-<slug>.md` sidecar existed.
4. A possible extra trigger for the "item missing on main" wording: the item was driven from `tasks/backlog/`, not the pool `tasks/ready/`. If the surface path looks the item up in the pool only, a `--allow-backlog` drive would always take the not-found branch, which would explain the mismatch between the message and the landed state.

Also seen in the same session, and probably worth its own look: `dorfl status` reported 487 in-flight locks (4 for this repo) while `git ls-remote origin 'refs/dorfl/lock/*'` returned NOTHING. The locks exist only in the local mirror under `~/.dorfl/repos/.../werust.git`. The arbiter is the authoritative record, so `status` is reporting stale mirror refs as live holds, which makes an operator believe work is in flight when none is.

Source: driving the werust board with the `drive-tasks` conductor, 2026-08-17.
