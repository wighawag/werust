---
title: "Gate-3 verdict: windows-release-packaging-leg + windows-app-manifest-is-malformed-xml (APPROVE both) — five platforms ship, after the leg caught its own bug"
date: 2026-07-31
status: open
reviewOf: windows-release-packaging-leg
verdict: APPROVE
---

## Verdict: APPROVE ✅ (both tasks)

Reviewed together because the second exists only to fix a defect the first shipped, and the pair is the more useful record.

**werust now produces a release artifact on all five platforms.** [Run 30626912474](https://github.com/wighawag/werust/actions/runs/30626912474): `verify`, `goreleaser`, `android-apk`, `ios-simulator-app`, `macos-desktop-app` and `windows-desktop-app` all SUCCESS, with `werust-windows-desktop-app` at **1,986,546 bytes** — the first Windows artifact this project has ever produced.

## The defect, and why it is a good story rather than an embarrassment

`windows-release-packaging-leg` merged with Gate 1 and Gate 2 both green. I then fired the release dry run, per the convention ratified today, and `windows-desktop-app` FAILED:

```
app.manifest : general error c1010070: Failed to load and parse the manifest.
LINK : fatal error LNK1327: failure during running mt.exe
```

Cause, diagnosed locally in a minute: the manifest's own explanatory comment used `--` as a prose dash, and **XML forbids a double hyphen inside a comment**. `mt.exe` was right to refuse.

Three things worth keeping from this:

1. **No gate in this repo could have caught it.** The Ubuntu gate cannot link a Windows binary; the existing shape test pinned the comctl32 identity STRINGS without ever parsing the file. It was invisible until a Windows runner tried to link. That is exactly the class of failure the CI-leg-first convention exists for, and the convention worked: the leg was there, so the defect surfaced in a dry run instead of on a release tag.
2. **The decoupling worked.** Every other job in the failing run was green. A Windows failure withheld only the Windows artifact, which is precisely what the sibling-job design (inherited from `fix-release-native-x86-desktop-and-decouple-mobile`) was for.
3. **The cause is a collision between two house styles.** This repo deliberately avoids the em dash, so authors write `--` in prose; XML comments are the one place that is illegal. Worth remembering the next time any XML lands here (a `.plist`, a future manifest, an Android resource).

The fix task closed it properly: comments REPHRASED rather than deleted (they are load-bearing — the comctl32 identity, the dark-mode caveat, the per-monitor-v2 rationale), plus a **well-formedness guard in the Ubuntu gate**, so the next edit to those comments cannot red a release job again. That guard is the real deliverable; the punctuation was the symptom.

## Criteria (packaging leg), ticked

1. **A Windows artifact attached to the tagged Release beside the others.** MET (tag path by construction, dry-run path measured).
2. **Built on `windows-latest` for `x86_64-pc-windows-msvc`, decoupled with `needs: verify`.** MET, and the decoupling proved itself under real failure.
3. **The app manifest the chrome was waiting for** (comctl32 v6 + per-monitor-v2 DPI). MET, and honestly documented: it does NOT buy dark-mode buttons, so the `follow-os-color-scheme` parity cell correctly stays `stubbed` and points here.
4. **Version from the one source; honest unsigned naming.** MET (`werust-windows-x86_64-unsigned.zip`).
5. **Shape pinned from Linux.** MET.

**The forward-note I planted was honoured:** the exact-set `PULL_REQUEST_FILTER` pin forced this task to update the constant deliberately rather than accrete, which is what it was for.

## Criteria (manifest fix), ticked

1. **Well-formed XML, comments intact and rephrased.** MET, verified locally with two parsers and then by `mt.exe` itself.
2. **A guard in the ordinary gate.** MET.
3. **A dispatched run showing `windows-desktop-app` green, URL recorded.** MET — **by me, not by the build.** The agent left a placeholder saying the dispatch was the conductor's job under the new convention, which is a fair reading of the corollary, but the criterion said the run had to be recorded. I fired it and wrote the result into the spike README. Worth flagging as a small drift in how that convention is being read: "the conductor obtains the measurement" is a backstop for when an agent CANNOT, not a licence to leave an acceptance criterion for someone else when the branch could have been pushed and dispatched.
