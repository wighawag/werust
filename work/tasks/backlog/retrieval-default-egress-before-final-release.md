---
title: "RELEASE GATE: the shipped IPFS egress default must not be a SILENT third-party gateway (make the privacy cost legible + guard the default; built-in verified retrieval remains the destination)"
slug: retrieval-default-egress-before-final-release
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

A privacy release-gate, in the form the human's 2026-08-01 answer settled (see Decisions below): the shipped default IPFS egress is a public trustless gateway, which sees every site the user visits. That is acceptable to SHIP only if it is **informed** rather than silent, and only while werust's own built-in verified retrieval does not exist yet.

So this task no longer changes WHICH backend ships by default. It makes the shipped default **honest and pinned**:

1. **Legible, at the point where it matters.** A user can see what their current retrieval backend can observe about their browsing, in werust's own words, without reading the source. The natural home is the retrieval surface that already exists (`werust://settings`, from `retrieval-backend-user-setting`), so this is an addition to a surface, not a new one.
2. **Pinned, so it cannot drift silently.** A guard asserts the shipped default is the DECLARED one and that its privacy characterisation exists alongside it, so a future change cannot quietly swap in a different third party (or add a second default per platform) without the guard reddening. This is the surviving, buildable half of the original "enforcement" criterion.

The DESTINATION is unchanged and is recorded here so it is not lost: werust's **built-in verified retrieval** (embedded-p2p / fetch-only, no third-party gateway) as the default. That depends on the embedded-p2p backend, which is Phase-2 work and belongs to the `trustless-ens-to-ipfs-phase2-3-helios-and-hardening` spec, not to this task. When it lands, the default flips there and this task's guard is what makes the flip visible.

## Decisions (from the human, 2026-08-01)

These answer and CLOSE the three open questions this task carried. They are recorded rather than deleted because they narrowed the task's scope substantially.

1. **Which final default: (a) built-in verified retrieval, never (b).** The first-run "choice from a curated community gateway set" option is **rejected outright** and must not be built, in any interim form. If (a) is not reachable in time, the interim is **the status quo** (the current trustless-gateway default), not a chooser.
2. **No community gateway set.** Question 2 is void: there is no curated list to source, keep current, or characterise, because (b) is not happening.
3. **Enforcement, narrowed honestly.** The original criterion ("the shipped default is not a single hard-coded third-party gateway") **cannot be asserted while the interim ships exactly that**, so a guard claiming it would be a lie. The guard therefore asserts the weaker true thing: the default is declared in one place, is the one that ships, and carries a user-facing privacy characterisation. The gate becomes **"no SILENT third-party default"** rather than "no third-party default".

**The trade-off this accepts, stated plainly:** this task was written so the default would be chosen deliberately at release rather than defaulted-by-omission. Answer (1) means the near-term outcome IS the status quo, so the gate is weaker than originally intended. It is not, however, defaulted-by-omission: the default is now chosen, declared, characterised to the user, and pinned by a guard. Reversing to the strict form is a one-line criterion change plus the guard's assertion, once (a) exists.

## Acceptance criteria

- [ ] A user can discover, from werust's own UI, what their active retrieval backend can observe about their browsing (the privacy/trust trade-off of the default is legible, not implied).
- [ ] The wording is derived from ONE source, the way every other chrome string is (a core derivation, not a per-edge literal), so the desktop and mobile edges cannot describe the same backend differently.
- [ ] A guard/test asserts the shipped default backend is the declared one and that its privacy characterisation exists, so a silent swap to a different third-party default reddens the gate.
- [ ] The guard's assertion states what it does NOT prove: that the default is not a third-party gateway (it currently is). No test or comment may claim the strict gate is met.
- [ ] Built-in verified retrieval as the DEFAULT is explicitly out of scope here and recorded as Phase-2 work (`trustless-ens-to-ipfs-phase2-3-helios-and-hardening`).
- [ ] Tests network-isolated; mirror the repo's style.
- [ ] Settings/pin writes stay isolated in tests (point `WERUST_SETTINGS_DIR` at a temp dir and assert the real one is untouched).

## Blocked by

- None. `retrieval-backend-user-setting` (the selector + custom-URL mechanism this describes the default of) is in `work/tasks/done/`, so this is startable now.

## Prompt

> Goal: werust ships a public trustless gateway as its default IPFS egress, and that gateway sees every site the user visits. The human has DECIDED (2026-08-01) that the destination is werust's own built-in verified retrieval (embedded-p2p / fetch-only) and that a first-run curated-gateway chooser must NEVER be built. Until built-in retrieval exists (Phase-2, spec `trustless-ens-to-ipfs-phase2-3-helios-and-hardening`), the interim default is the status quo. Your job is to make that interim HONEST rather than silent: (1) surface, in werust's own UI, what the active retrieval backend can observe about the user's browsing, and (2) add a guard so the shipped default cannot be silently swapped for a different third party.
>
> Read `docs/spikes/retrieval-backend-user-setting/DECISIONS.md` first: the selector, the `werust://settings` surface and the persisted `retrieval.json` already exist, and this adds to them rather than inventing a surface. Follow the repo's ONE-derivation rule for any user-facing string (`CONTEXT.md`, "chrome presentation / painter"): the wording belongs in the toolkit-free core so every edge reads the same string, never a Kotlin/Swift/GTK literal. Note the standing convention that a guard must not overstate what it proves: the strict "not a third-party default" claim is FALSE today, so assert the declared-default + characterisation-exists property and say so in the test's own words.
>
> FIRST, check this task against current reality (it is a launch snapshot and may have DRIFTED): confirm the retrieval seam, the settings surface and the default endpoint are still shaped as described, and that no Phase-2 embedded-p2p backend has landed in the meantime (if it HAS, this task is the wrong shape: route to needs-attention rather than building the interim).
>
> RECORD non-obvious in-scope decisions durably and link them from the done record. The characterisation WORDING is a judgement call with real consequences (it is a privacy claim shown to users); if you find yourself choosing how strong a claim to make about what a gateway operator can see, that belongs in a `## Decisions` block in the done record, or an ADR if it meets the gate in `work/protocol/ADR-FORMAT.md`.

---

### Claiming this task

```sh
dorfl claim retrieval-default-egress-before-final-release --arbiter origin
git fetch origin && git switch -c work/retrieval-default-egress-before-final-release origin/main
git mv work/tasks/ready/retrieval-default-egress-before-final-release.md work/tasks/done/retrieval-default-egress-before-final-release.md
```
