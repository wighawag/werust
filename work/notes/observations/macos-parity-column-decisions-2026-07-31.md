# Decisions taken while filling the `macos` parity column (2026-07-31)

Task `macos-parity-column-and-stub-tasks`. Three judgement calls the task did not settle, recorded here so a reviewer or the sibling `windows-parity-column-and-stub-tasks` can ratify or reverse them rather than reverse-engineer them. Each one is visible in `docs/platform-capability-matrix.toml`'s per-row prose too; this note is the one place they sit together.

## 1. A partly-wired capability gets `stubbed`, not `implemented` with a caveat

`debug-capture-console-and-network` is real on macOS for CONSOLE (measured on a `macos-14` runner) but its NETWORK reach is the injected `fetch`/`XHR` shim only: no main-document row, no row for the requests werust's own scheme handler serves. The row's description explicitly requires the main-document entry to mirror the load's posture, so the cell is `stubbed` pointing at the new task `macos-debug-network-capture-main-document-and-scheme-handled`.

Alternative considered: `implemented` with the shortfall written into the comment, which is what the iOS cell does for its own subresource limit. Rejected because iOS DOES have the main-document and scheme-handler capture points (`werust_ios_capture_network`); macOS has neither, so an `implemented` cell would claim a capability element the row names and macOS does not have — the exact green-looking claim ADR-0005 exists to prevent. What it touches: the sibling Windows column faces the same call on the same shared code (`desktop_paint::install_debug_capture` serves both new desktop edges), and its own task body already anticipates a `stubbed` cell there.

## 2. `implemented` where the platform does it BY DEFAULT and werust's job is an enforced absence

`follow-os-color-scheme` is marked `implemented` on macOS although no one has watched a live Light/Dark switch: AppKit propagates the effective appearance into the chrome and the web process, and werust's entire obligation (ADR-0009: follow, never force) is to set no `NSAppearance` — an absence the source-shape guard `crates/werust-macos/tests/macos_window_shape.rs` reds the gate over. This is the same basis the iOS cell already stands on.

Alternative considered: `stubbed` pending a manual check on a Mac. Rejected because `stubbed` means a known wiring GAP with work to do, and using it for "wired but unwitnessed" would blur it into a verification tracker; the honest place for the unwitnessed half is the row's prose plus the manual steps in `docs/spikes/macos-appkit-window-and-chrome/README.md`. What it touches: every row whose macOS evidence is compilation plus shared-core tests rather than a driven runtime step (`spa-url-tracking`, `blank-window-open-navigates-in-place`, `ipfs-redirects-3xx-navigation`, `retrieval-backend`, `scheme-less-entry-routing`) says so in its own comment, so the distinction is visible per row and not only here.

## 3. Column placement and a strengthened guard

`macos` is placed BESIDE `desktop` in `platforms` (and its cell line directly under each row's `desktop` line) rather than appended last, so the desktop-family edges read together and the pending `windows` column has an obvious slot next to it; the sibling task rebases either way. The guard's `the_real_matrix_is_well_formed_and_covers_every_platform` now also expects `macos`, so a later change cannot drop the column and take its two tracked gaps with it. That strengthens the guard; nothing about it was weakened. The `desktop`-means-Linux naming wrinkle this exposes is recorded separately in `desktop-platform-key-now-means-linux-only-2026-07-31.md`.
