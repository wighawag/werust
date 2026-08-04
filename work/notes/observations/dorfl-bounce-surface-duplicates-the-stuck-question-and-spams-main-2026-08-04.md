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
