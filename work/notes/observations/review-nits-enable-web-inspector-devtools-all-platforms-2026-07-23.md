---
title: review-gate non-blocking nits for 'enable-web-inspector-devtools-all-platforms' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: enable-web-inspector-devtools-all-platforms
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'enable-web-inspector-devtools-all-platforms' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Task frontmatter says covers:[2] but spec story 2 is ENS name resolution (ronan.eth namehash->contenthash), which is unrelated to the web inspector. This task is the v0.2.2 human field-test request, not spec story 2. Should covers:[2] be corrected/removed so the spec-coverage map stays honest?
  (work/tasks/done/enable-web-inspector-devtools-all-platforms.md frontmatter vs work/specs/tasked/ens-to-ipfs-resolution-phase1-rpc-skeleton.md story 2 (line 42))
- Ratify: Android gates on ApplicationInfo.FLAG_DEBUGGABLE, but the recorded decision note (Decision 2) said BuildConfig.DEBUG. The code comment explains the swap (avoids extra buildConfig generation, equivalent debug signal). Confirm FLAG_DEBUGGABLE is acceptable as the durable gate.
  (BrowserActivity.kt: if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0); note Decision 2 names BuildConfig.DEBUG)
- Ratify recorded Decision 1 (F12 desktop shortcut, avoids GTK debugger Ctrl+Shift+I/D), Decision 2 (debug-build gate on all three platforms), Decision 3 (capability name web-inspector). All three are recorded, correct, and coherent with the parity matrix.
  (work/notes/observations/web-inspector-devtools-gating-decisions-2026-07-23.md)
