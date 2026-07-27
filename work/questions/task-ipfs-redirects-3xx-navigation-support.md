<!-- dorfl-sidecar: item=task:ipfs-redirects-3xx-navigation-support type=task slug=ipfs-redirects-3xx-navigation-support allAnswered=false -->

## Q1

**'task:ipfs-redirects-3xx-navigation-support' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - A 3xx now pushes a NEW history entry on top of the redirected-FROM entry instead of replacing it, so Back after a redirect lands back on the redirecting URL. Since go_back resets the chain (lib.rs go_back -> redirects.reset + note_top_level_navigation), that URL matches its 3xx rule again and bounces the user forward to the target (or, if the main-frame inference misses on that reload, shows an error page for the old URL). Should the redirected-FROM entry be replaced (a replace-current-entry seam), or is the back-trap accepted and recorded as a named limitation plus a follow-up task, as Decision 7 does for the main-frame flag? Nothing in docs/spikes/ipfs-redirects-3xx-navigation-support/DECISIONS.md mentions history semantics at all. (crates/werust-core/src/lib.rs follow_pending_redirect calls renderer.navigate(target) (a PUSH; the seam has no replace-current-entry, cf. work/notes/observations/reload-re-resolves-ens-name-decision-2026-07-23.md). The shell test a_queued_redirect_navigates_the_shell_on_the_pump_and_moves_the_bar_and_history asserts can_go_back == true after the redirect as a SUCCESS, i.e. the redirecting entry is deliberately left in history. Introduced by this diff: before it, a matched 3xx simply failed and there was no target to trap on.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):

## Q2

**'task:ipfs-redirects-3xx-navigation-support' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - A 3xx now pushes a NEW history entry on top of the redirected-FROM entry instead of replacing it, so Back after a redirect lands back on the redirecting URL. Since go_back resets the chain (lib.rs go_back -> redirects.reset + note_top_level_navigation), that URL matches its 3xx rule again and bounces the user forward to the target (or, if the main-frame inference misses on that reload, shows an error page for the old URL). Should the redirected-FROM entry be replaced (a replace-current-entry seam), or is the back-trap accepted and recorded as a named limitation plus a follow-up task, as Decision 7 does for the main-frame flag? Nothing in docs/spikes/ipfs-redirects-3xx-navigation-support/DECISIONS.md mentions history semantics at all. (crates/werust-core/src/lib.rs follow_pending_redirect calls renderer.navigate(target) (a PUSH; the seam has no replace-current-entry, cf. work/notes/observations/reload-re-resolves-ens-name-decision-2026-07-23.md). The shell test a_queued_redirect_navigates_the_shell_on_the_pump_and_moves_the_bar_and_history asserts can_go_back == true after the redirect as a SUCCESS, i.e. the redirecting entry is deliberately left in history. Introduced by this diff: before it, a matched 3xx simply failed and there was no target to trap on.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q2 fields: id=q2 kind=stuck -->

**Your answer** (write below this line):
