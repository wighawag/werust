<!-- dorfl-sidecar: item=task:debug-view-console-network-tabs-desktop type=task slug=debug-view-console-network-tabs-desktop allAnswered=false -->

## Q1

**'task:debug-view-console-network-tabs-desktop' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The AppendFrom refresh path never drops the rows the ring buffer evicted from the front, so at the cap the view still goes stale (the second half of the Gate-2 defect). After a rebuild the view mirrors the store (300 rows, anchor = last sequence). Each at-cap push evicts one entry and appends one; tail_plan finds the anchor inside the snapshot and returns AppendFrom, which only APPENDS the new tail (crates/werust/src/main.rs DebugView::refresh). The only row removal is clear_list_box, reachable only via Rebuild. So the view's row count climbs 300 toward ~600, its top rows are entries the store already discarded, and it stays stale until the anchor itself is evicted (~300 pushes later) and a Rebuild fires. This violates the requeue's explicit requirement that pushing past the cap must not leave the view showing rows the store evicted, and falsifies DECISIONS.md Decision 2's claim that evicted rows are dropped from the view's top implicitly, plus README manual step 10's claim that row counts stay at 300. Fix: track the first-rendered sequence (or rendered row count) and, on AppendFrom, drop from the top of the ListBox the rows whose sequence is below the snapshot head (drop count = snapshot_len - (rows the view still legitimately holds)); extend the past-cap display test to assert row_count stays at MAX after incremental at-cap appends, not just after a rebuild. (crates/werust/src/main.rs tail_plan AppendFrom arm + DebugView::refresh (append-only) vs crates/werust-core/src/debug.rs push_console/push_network pop_front at MAX=300; docs/spikes/debug-view-console-network-tabs-desktop/DECISIONS.md Decision 2 correction paragraph)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):
