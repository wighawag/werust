# Judgement calls made while researching the Windows platform

Task: `windows-platform-research`. Findings: [`README.md`](README.md). Decision: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md).

These are the calls a reviewer or a human should be able to reverse without re-reading the research. One entry per decision: what was chosen, why, what was rejected, and what it touches.

## 1. The ADR recommends DEFER, not GO, even though WebView2 is feasible

**Chosen:** the ADR's headline recommendation is **defer** the Windows desktop build, with an explicit, dated revisit trigger and a ready-to-dispatch breakdown, and a **go** on ONE small platform-neutral enabler (extracting the desktop chrome presentation rules into the shared core).

**Why:** the task asked "can it be done?" and the answer is a clean yes (section 5 of the README: the seam needs no widening, the bindings are mature, the runtime is present on Windows 10+). But feasibility is not the funding question. The measured cost is 22 to 39 person-days for capability parity (README section 6), the repo's stated live experiment is whether the pure-Rust renderer stack reaches T1 faster than the Zig arm (`CONTEXT.md`), and a third desktop OS advances neither T1 nor the thesis. Recommending "go" on a 5-week build that nothing in the spec ladder is waiting on would be the research telling the human what they can do rather than what it costs.

**Rejected alternatives:** (a) **go now** (honest about feasibility, dishonest about priority; also would have to be started on an UNPROVEN origin mechanism, see decision 2); (b) **no-go** (too strong: nothing found is disqualifying, and a flat no-go would invite re-researching this in six months, which is exactly what an ADR exists to prevent).

**What it touches:** `macos-desktop-build` (the ADR's split recommendation), and any future `windows-*` task. It does NOT touch shipped code.

## 2. The origin question is answered as "documented yes, unproven in practice", not as a yes

**Chosen:** report the tuple-origin capability as VERIFIED-as-documentation and UNVERIFIED-as-behaviour, and make an on-Windows probe gate 0 of any Windows work, with werust's existing `origin_map.rs` named as the fallback mechanism.

**Why:** the primary docs are unambiguous that `HasAuthorityComponent = true` yields a tuple origin, which is a real difference from Android (interception only, no registration). But werust's actual requirement is a same-origin `fetch()` plus `pushState` on that origin, and the WebView2 tracker carries an open (2024-01 to 2025-12) report of `fetch()`/XHR failing on registered custom schemes with the handler never firing, plus a January 2026 stable-channel regression that broke plain link navigation from custom-scheme documents. This repo has already paid once for settling a platform-origin question from documents instead of a device (`mobile-ipfs-scheme-interception-ios-and-android` -> the field bug in `mobile-ronan-eth-buttons-no-navigation`). Repeating that would be the same mistake with a different OS.

**Rejected alternatives:** (a) assert it works because the docs say so (the exact failure mode this repo already recorded a lesson about); (b) assert it does NOT work because wry avoids it (wry's stated reason is Windows-version coverage, and its comment predates parts of the API's maturity; that is evidence of caution, not of breakage).

**What it touches:** whether `origin_map.rs` stays Android-specific or gets promoted to a shared module. Deliberately left OPEN for the probe to decide, so no code moves on a guess.

## 3. No new seam concept was introduced for "the generic desktop shell"

**Chosen:** answer the forward-pointer's generic-desktop-seam question WITHOUT adding a new named seam. The thing that is genuinely shareable is the chrome PRESENTATION (pure functions of `ChromeState`), and its home is the existing shared toolkit-free core next to `ChromeState`; each platform stays an "OS edge" that only PAINTS. The per-platform window/widget layer is NOT shareable and gets no shared abstraction.

**Why (coherence check against `CONTEXT.md`, the ADRs and the code):** the repo's glossary already fixes the meaning of "seam" as a hot-swappable INTERFACE with alternative implementations (`Renderer`, `ScriptEngine`, `Fetcher`). A "desktop shell seam" would be a second interface over the same rendering/chrome layer the `Renderer` seam plus `BrowserShell` already own, i.e. it would re-mean "seam" as "shared helper code" and duplicate an existing concept. It would also sit at the wrong layer: what is duplicated today is not the window, it is the DERIVATION of display facts, currently triplicated (`crates/werust/src/main.rs` `status_line` / `trust_indicator` / `error_banner_*` / `invalid_entry_badge_*`, Kotlin `WerustCore.kt` `statusLine()` / `trustIndicator()` / `errorBanner()`, Swift `WerustCore.swift` twins). Naming the fix "presentation in the core, painting at the edge" reuses vocabulary the parity matrix already speaks ("each chrome applies the SAME rule over the SAME core fact").

**Rejected alternatives:** (a) a `DesktopShell` seam/trait spanning GTK + Win32 + AppKit (no common widget vocabulary without adopting a cross-platform GUI toolkit, which is a large new dependency and would re-mean the toolkit-free core); (b) rendering the browser chrome itself in a second webview (genuinely generic across all five contexts, and worth revisiting, but it replaces every native chrome at once and puts werust's own UI inside the engine werust does not trust by default, which is an ADR-0001-adjacent decision far beyond this task).

**What it touches:** the recommended split of `macos-desktop-build` (its first sub-task becomes this extraction), the eventual de-duplication of the Kotlin/Swift presentation twins, and any future Windows shell.

## 4. The ADR carries no shape test, unlike most changes in this repo

**Chosen:** ship documents only (ADR + this spike), with no test.

**Why:** the deliverable is a decision record; there is no seam, no behaviour and no wiring to pin, and none of `docs/adr/0001`..`0010` carries a test. Adding a doc-shape test for 0011 alone would invent a convention this repo does not have, for a file whose content a test cannot judge. The `verify` gate (`cargo fmt --check && cargo clippy && cargo build && cargo test`) is unaffected by a docs-only change. Where this research DOES prescribe a test, it prescribes it as gate 0 of the future build (the origin probe, README section 4), which is where a red-capable test can actually exist.

**What it touches:** the test-first nudge (`promptGuidance.testFirst`) for this one item, and the shape of the future Windows task's acceptance.

## 5. `wry` was evaluated as a whole-backend option, not just as bindings

**Chosen:** recommend `webview2-com` directly and reject `wry` as werust's backend, while citing wry as the decisive real-world PRECEDENT for the origin mechanism.

**Why:** wry would supply less code (it already drives WebView2, and as of 0.56 it does expose `go_back` / `can_go_back`, so session history is no longer a blocker), but it hides exactly the two things werust's capability rows depend on: it gives no per-resource network observation (the `debug-capture-console-and-network` row's desktop reach) and it FORCES the internal-`localhost`-origin mapping, removing the choice the probe exists to make. It would also add `wry` + a window crate as core dependencies of the trust-carrying path, and werust's `Renderer` trait already IS the abstraction wry would otherwise provide.

**What it touches:** any future Windows or macOS backend task, and the dependency posture of `crates/webview-renderer`.
