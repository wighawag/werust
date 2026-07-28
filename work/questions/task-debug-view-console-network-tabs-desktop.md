<!-- dorfl-sidecar: item=task:debug-view-console-network-tabs-desktop type=task slug=debug-view-console-network-tabs-desktop allAnswered=false -->

## Q1

**'task:debug-view-console-network-tabs-desktop' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The debug view's incremental refresh silently freezes once a ring buffer reaches its 300-entry cap. DebugCapture::push_console/push_network evict via pop_front at capacity, so len stays at MAX (300) and neither refresh branch fires (len < rendered resets, len > rendered appends; len == rendered does nothing). From that point new entries are captured but never rendered, and the view's oldest rows no longer exist in the store: the Console/Network tabs go silently stale exactly in the long-session case the ring buffer exists for. 300 network requests is one busy session. This breaks acceptance criterion 'the view updates as new entries are captured'. Decision 2 in DECISIONS.md describes the incremental design but misses the at-capacity eviction case, and no test covers it. Fix: detect eviction (compare the snapshot head, or a store-side sequence number) and drop the same rows from the top of the list, or rebuild when the head differs. (crates/werust/src/main.rs DebugView::refresh (console.len() < rendered_console / > rendered_console branches) vs crates/werust-core/src/debug.rs push_console/push_network (if len >= MAX { pop_front(); } push_back(); len constant at cap), MAX_CONSOLE_ENTRIES/MAX_NETWORK_ENTRIES = 300)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):
