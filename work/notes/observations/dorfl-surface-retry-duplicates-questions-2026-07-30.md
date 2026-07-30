---
title: "dorfl: the surface-retry loop wrote the SAME stuck question five times into one questions sidecar"
date: 2026-07-30
status: open
area: tooling/dorfl
---

## What was observed

When Gate 2 blocked `task:windows-renderer-ci-leg`, `dorfl do` tried to publish the stuck surface to `origin/main` and hit push contention, retrying five times:

```
>> push reported up-to-date / no change of our making — origin/main is not our commit — treating as rejected.
>> main advanced under us — surface refetch and retry (1/5)...
... (through 5/5)
>> surface for 'task:windows-renderer-ci-leg' did not land on origin/main (item missing on main, or contention exhausted after retries).
```

It then reported the surface as NOT landed. It had in fact landed — three times (`bbd438f`, `bd5bcc5`, `5a144e5` on `main`, each a full `surface task:… (stuck)` commit with the identical Gate-2 block quoted in the subject) — and `work/questions/task-windows-renderer-ci-leg.md` came out carrying **five identical entries**, `Q1` through `Q5`, each asking "'task:windows-renderer-ci-leg' was bounced — how should we proceed?" with the same quoted reason and its own `<!-- qN fields: id=qN kind=stuck -->` marker.

## Why it looks like a real bug, not noise

Two symptoms, probably one cause:

1. **The success detection is wrong for this path.** The retry logic treats "push reported up-to-date / origin/main is not our commit" as a REJECTION, but the commit it wanted was already on `main` (its own, from the previous attempt). So a successful publish is read as contention, and the loop re-publishes.
2. **The sidecar append is not idempotent per (item, reason).** Each retry appended a fresh `Q` entry instead of recognising an identical outstanding stuck question. The retry loop turned one question into five.

The blast radius is small but not zero: a human opening that sidecar sees five separate unanswered questions for one block, and `allAnswered=false` stays sticky until all five are answered. On a repo where a `run` daemon or an `advance` leg consumes the sidecar, five duplicates of one question is a worse input than one.

## Reproduction context

- dorfl 0.11.1, `do task:<slug> --isolated --allow-backlog --review --merge`, GitHub arbiter, Gate 2 returning a terminal block.
- The contention was self-inflicted and ordinary: the conductor had pushed a commit to `main` between claim and surface.

## Recommended fix (for whoever owns dorfl)

Before treating a push as rejected, check whether the intended surface commit is already an ancestor of `<arbiter>/main`; if it is, the publish SUCCEEDED. Separately, make the questions-sidecar append idempotent: an outstanding entry with the same `kind=stuck` and the same reason text for the same item should be updated, never duplicated.

## Not fixed here

Out of scope for the task being driven, and it is dorfl's own repo, not werust's. Recorded so the next person who sees five identical questions knows it is a tooling artefact and not five real blocks. The werust-side residue (a done task carrying `needsAnswers: true` plus a stale sidecar) is precedented and was left as found.
