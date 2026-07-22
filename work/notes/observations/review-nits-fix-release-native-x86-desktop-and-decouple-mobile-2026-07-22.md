---
title: review-gate non-blocking nits for 'fix-release-native-x86-desktop-and-decouple-mobile' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: fix-release-native-x86-desktop-and-decouple-mobile
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'fix-release-native-x86-desktop-and-decouple-mobile' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: each mobile job runs `gh release create ... 2>/dev/null || true`, which swallows ALL errors (auth, network, rate-limit), not only the already-exists case. A genuine create failure is masked until the following `gh release upload` fails. Acceptable since upload still fails loudly, but confirm this is the intended robustness posture.
  (.github/workflows/release.yml:199,261)
- Ratify concept re-meaning: ADR-0002 previously framed 'Zig-less' as language/renderer-only (a Zig cross-linker was fine); this task redefines it to Zig-less end to end for desktop. The ADR Update section supersedes the old framing explicitly with rationale, so it is coherent, but the glossary/thesis term now has a narrowed meaning worth a human nod.
  (docs/adr/0002 Update section)
