# Decisions taken while making the Win32 chrome scale itself

Task `windows-chrome-must-scale-with-the-display-dpi`. What landed and what is proven by what: [`README.md`](README.md). These are the judgement calls a reviewer (or a later task) would otherwise have to reverse-engineer from the diff — each with what was chosen, why, what was rejected, and what else it touches.

## 1. The scaling arithmetic is spelled in Rust, not delegated to Win32's `MulDiv`

**Chosen.** `dpi::Dpi::scale` reproduces `MulDiv(value, dpi, 96)` in plain Rust: multiply in `i64`, divide by 96 rounding half AWAY from zero, saturate rather than wrap.

**Why.** The task asked for the arithmetic to be unit-tested on the Ubuntu gate, and a module that called `MulDiv` could only be COMPILED on Windows — the seam would have been `#[cfg(windows)]` and CI's only reachable check would have been "the source mentions MulDiv". The whole value of a seam here is that the one thing this crate computes about geometry is testable where the gate runs, which is the same argument `profile.rs` already won.

**Rejected.** (a) Calling `MulDiv` from a `cfg(windows)` seam: untestable on the gate. (b) Floating-point `value as f32 * dpi as f32 / 96.0`: it introduces a rounding rule DIFFERENT from the one every other Win32 program uses, and Windows' own examples are all `MulDiv`, so a control would sit a pixel off its neighbour at some scales for no reason anyone could later explain.

**What it touches.** Nothing outside this crate. The contract is pinned by tests (`the_rounding_is_mul_divs_and_a_thin_strip_never_vanishes`) precisely so a later "simplify" to truncating division is caught rather than shipped: `MulDiv(3, 144, 96)` is 5, truncation gives 4, and a 3-pixel progress strip losing a pixel at 150% is exactly the class of bug this task exists to end.

## 2. The DPI is read once per window and HELD, not re-read per rectangle

**Chosen.** `Chrome` carries a `dpi: Cell<u32>`, seeded from `GetDpiForWindow` at open and REPLACED from `WM_DPICHANGED`'s `wParam`. `Chrome::metrics()` builds the whole table from it, once per layout pass.

**Why.** Two reasons. A layout pass that re-read the DPI per rectangle could straddle a change and mix two scales in one toolbar. And `WM_DPICHANGED` carries the new DPI in `wParam` precisely so a window does not have to race the platform for it: taking it from the message is what Microsoft's own sample does.

**Rejected.** Calling `window_dpi()` inside `relayout()`. It reads well, but it makes the layout depend on WHEN it runs, and it is untestable: the guard can assert "exactly one `GetDpiForWindow` call site" only because there is one.

**What it touches.** The window smoke reads the same value back (`BrowserWindow::dpi()`), so a run prints the display scale it measured against.

## 3. ONE chrome font per process, sized to the BROWSER window's display

**Chosen.** The debug view wears the browser window's `HFONT`. Its RECTANGLES come from its own monitor's DPI (`relayout_debug_window_of` takes its own `GetDpiForWindow` reading and it answers `WM_DPICHANGED` itself), but its TEXT is whatever size the browser window's font currently is.

**Why.** The font is owned by the `Controller`, which owns both windows; a per-window font means a second `HFONT` lifecycle (create, push, release) for a window that is opened and closed repeatedly, and the leak it would invite is the exact bug criterion 3 of this task exists to prevent. The visible cost is narrow: a debug view alone on a differently scaled monitor has correctly sized rectangles and text one size behind, until the browser window is dragged too.

**Rejected.** A font per window (correct, more lifecycle), and a font per DPI in a small cache (correct, more state than two windows justify).

**What it touches.** The debug view (`debugview.rs`, `window.rs`) and any future second window. It is recorded here and in manual step 5 of the README rather than left for someone to discover, and reversing it is local: give `DebugWindow` its own font field and release it on `WM_CLOSE`.

## 4. The initial size is applied AFTER creation, and it scales the OUTER window

**Chosen.** `CreateWindowExW` still takes the 96-DPI design size (1024x768); the window is then resized to `metrics.default_width/height` with `SetWindowPos` as soon as it exists.

**Why.** `GetDpiForWindow` needs an `HWND`, and the HWND is what tells us WHICH monitor the window landed on — the per-monitor answer the manifest's `PerMonitorV2` is about. Sizing before creation would mean asking a monitor handle instead (`MonitorFromPoint` + `GetDpiForMonitor`), i.e. guessing the monitor the window has not been placed on yet, with `CW_USEDEFAULT` placement making the guess unreliable.

**Rejected.** (a) `GetDpiForMonitor` on the nearest monitor before creation: a second DPI reader, for a guess. (b) `AdjustWindowRectExForDpi` to scale the CLIENT area instead of the outer window: strictly more correct, but it CHANGES the shipped window's apparent size at 100% (the client area would grow by the frame), and this task is about restoring the intended size, not re-picking it. The pre-existing semantics — 1024x768 is the outer window — are preserved exactly.

**What it touches.** The one-frame window flash a resize implies is invisible: it happens before `ShowWindow`. Any later task that wants a true 1024x768 CLIENT area should change the design constant and the semantics together.

## 5. An unreadable DPI falls back to the 96 baseline rather than refusing

**Chosen.** `Dpi::new(0)` is `Dpi::BASELINE`. `GetDpiForWindow` returns 0 for an invalid window.

**Why.** The alternative behaviours are worse in every direction: scaling by 0 collapses every rectangle to nothing (a window with no visible chrome), and there is no useful error to raise — the chrome cannot decline to lay itself out. Falling back to the design metrics reproduces exactly the behaviour werust had before this task, which is a known, survivable state.

**What it touches.** Nothing user-visible on a real session, where the reading always succeeds. It is a defined edge rather than an accident, and it is unit-tested (`an_unreadable_dpi_falls_back_to_the_baseline_rather_than_collapsing`).

## 6. The guard SCANS for raw pixels instead of listing the ones it remembers

**Chosen.** `windows_window_shape.rs` strips `metrics.scale(…)` calls out of both layout functions and then fails on any remaining integer literal other than `0` (an origin) and `2` (the two-margin multiplier).

**Why.** The task's own warning: there are dozens of call sites, and a missed one is a subtly misaligned control rather than an obvious failure. A guard that lists the constants it knows about would have gone green on the very next hard-coded `+ 6` someone adds. This is the same reasoning that put the palette, the labels and the chrome rules under scanning guards in this file already.

**What it touches.** Anyone adding a metric to a Win32 layout in this crate must route it through `Metrics` (a named field for a design metric, `Metrics::scale` for an incidental gap) or the Ubuntu gate goes red with the literal it found.

## 7. The word "seam", used here in the task's loose sense, not `CONTEXT.md`'s strict one

**The tension.** `CONTEXT.md` reserves **seam** for a hot-swappable BACKEND INTERFACE with alternative implementations (`Renderer`, `ScriptEngine`, `Fetcher`), and contrasts it explicitly with a **painter**, which "has no alternatives to swap, it has one derivation to render". `crates/werust-windows/src/dpi.rs` has no alternative implementation and never will: it is one conversion, in one place. By the glossary's definition it is not a seam.

**Chosen anyway, and knowingly.** The task, its acceptance criteria and the finding all say "through ONE `scale()`/`Dpi` seam", and a build that silently renamed the thing its acceptance criterion names would be harder to review than this paragraph. So the word is kept, always QUALIFIED as "the DPI seam" (never a bare "seam"), and it is used in the everyday engineering sense of "the one place two representations meet" — here 96-DPI design units and this display's pixels.

**What it touches.** The glossary. Two cheap resolutions exist and both are a later editorial change, not code: add the loose sense to `CONTEXT.md` beside the strict one, or rename this module's vocabulary to something the glossary already owns (a `Dpi` SCALE, and `Metrics` as the chrome's measurements at it — the type names already read that way, so only prose would move). Recorded rather than decided here because it is a naming decision with owners outside this task.
