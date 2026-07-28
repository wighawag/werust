---
title: "The retrieval-backend user setting cannot take effect on Android (no settings dir, and the switch needs a restart)"
date: 2026-07-28
kind: observation
status: open
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
---

Spotted while running the on-device end-to-end for `mobile-ronan-eth-buttons-no-navigation` (API-36 emulator, debug APK).

`werust://settings` renders on Android and happily applies a Custom gateway selection ("Custom gateway or local node URL (active)"), but the choice can NEVER reach the load path:

1. `RetrievalSettings` persistence resolves `settings_dir()` from `WERUST_SETTINGS_DIR` / `XDG_CONFIG_HOME` / `HOME` (`crates/werust-core/src/retrieval.rs`); none is set in an Android app process (checked `/proc/<pid>/environ`), and neither mobile edge sets `WERUST_SETTINGS_DIR` from its sandbox path. The code comments even anticipate the edge doing so ("A mobile edge that wants a platform-specific location can set SETTINGS_DIR_ENV from its app sandbox path before creating the session"). So persistence falls back to in-memory. Nothing is written (verified: no `retrieval.json` anywhere under the app's data dir).
2. The retriever is built ONCE at session `new()` (recorded limitation in `docs/spikes/retrieval-backend-user-setting/DECISIONS.md`, decision 4): the choice takes effect on the NEXT launch. But the next launch re-reads the (nonexistent) file and re-defaults.

Net effect on Android: the Custom backend (the private/local-node choice) is unusable, silently: the page says "active" while the load path keeps using `dweb.link`. iOS likely shares the same gap (same `settings_dir()` resolution, and its edge also never sets the lever). Suggested fix direction for a task: each mobile edge sets `WERUST_SETTINGS_DIR` to its app sandbox config dir before session creation (and, separately, consider a live retriever swap so the choice does not need a restart).
