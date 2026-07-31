# `window.localStorage` on Android: the fix, the measurements, and the matrix's first web-platform row

`window.localStorage` on Android is now a `Storage` object that round-trips and survives a reload. Before this it was **`null`** — found by the human on real hardware testing `mandalas.eth`, which worked on desktop. Root cause: Android's `WebSettings.domStorageEnabled` defaults to `false` and `BrowserActivity.kt` never set it, so the System WebView returned `null` where the web platform allows only a `Storage` object or a `SecurityError` throw. Task `android-enable-dom-storage-and-guard-web-platform-parity`; diagnosis `work/notes/findings/android-localstorage-is-null-dom-storage-never-enabled-2026-07-31.md`; capability row `web-storage` in `docs/platform-capability-matrix.toml`.

**It was NOT the opaque-origin problem, and the `null` is what proved that.** Android is the one platform where `ipfs://` is origin-MAPPED, so an opaque origin was the obvious suspect — but an opaque origin THROWS `SecurityError`. `crates/werust-android/rust/src/origin_map.rs` is working and was not touched.

## What is in this directory

| File | What it is |
|---|---|
| `README.md` | this: what landed, the decisions, how to verify |
| `MEASUREMENTS.md` | the on-device evidence: what `domStorageEnabled` actually governs, verbatim |
| `WEBSETTINGS-AUDIT.md` | the other browser-wrong `WebView` defaults, with recommendations, **changing none of them** |

## Wiring

- `crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt` — `settings.domStorageEnabled = true` in the `WebView` configuration block, beside `javaScriptEnabled`, carrying the WHY its neighbours carry: the default is built for an app EMBEDDING a view rather than for a browser, and enabling it is safe here because `origin_map.rs` gives each CID its own subdomain, so storage stays partitioned per CONTENT ADDRESS exactly as it is on the four platforms with real `ipfs://<cid>` origins.
- `crates/werust-core/tests/web_storage_edge_wiring_shape.rs` — the guard that **runs on every push**. It parses the Kotlin edge and pins that DOM storage is still enabled (so a refactor of that settings block cannot silently return `localStorage` to `null`), that the origin map still puts each CID in its own HOST LABEL (the safety premise, asserted rather than trusted), that the matrix row exists and is complete, that **every cell of the row names the evidence backing it** (`EVIDENCE (<platform>):`, so a cell cannot be flipped to `implemented` without writing down what measured it) with `android` pinned `implemented`, and that every audited setting is listed in the audit note and set by nobody.
- `crates/werust-android/app/src/androidTest/java/com/github/wighawag/werust/WebStorageTest.kt` — the on-device probe that **does not run in CI**. It measures `localStorage`, `sessionStorage`, IndexedDB and cookies against the real System WebView with the setting off and on.
- `docs/platform-capability-matrix.toml` — the `web-storage` row.

## How to verify

The half that runs on every push, and in the `verify` gate:

```sh
cargo test -p werust-core --test web_storage_edge_wiring_shape
```

The half a human runs by hand on a device or emulator (**there is no CI emulator leg**):

```sh
cd crates/werust-android && ./gradlew :app:connectedDebugAndroidTest
```

The captured evidence is logged under the `WebStorageProbe` logcat tag and quoted verbatim in `MEASUREMENTS.md`. On an API 36 emulator with System WebView 142 all 7 web-storage tests pass, the 3 pre-existing `SpaClientNavOriginTest` tests still pass, and `./gradlew :app:assembleRelease` still builds the release APK.

## Decisions

### 1. Only the two MEASURED edges claim `implemented`; the three that rest on an engine default are `stubbed` (REVERSED after review, 2026-07-31)

**Chosen:** `android` = `implemented` (measured on-device), `desktop` = `implemented` (the field report, named as the weaker evidence), and `macos` / `windows` / `ios` = `stubbed` against `matrix-web-platform-rows-are-measured-on-every-edge`. Every cell carries an `EVIDENCE (<platform>):` line in the row saying what backs it, and the shape guard reds if one is missing.

**This reverses what this task first landed.** The first version marked all five `implemented`, arguing that WKWebView exposes no DOM-storage toggle, WebView2 inherits Chromium's browser defaults, no edge disables anything, and therefore what was missing was EVIDENCE rather than capability. Review blocked it, and it was right to: **this repo has already MEASURED that engine defaults do not carry to the origins werust serves.** On a REGISTERED `ipfs://` origin with `HasAuthorityComponent` + `TreatAsSecure` — a real, secure tuple origin where `fetch` and `pushState` both work — Blink still rejects `navigator.serviceWorker.register('/sw.js')` with `InvalidStateError` (`docs/spikes/windows-ipfs-origin-probe-on-ci/probe-report-2026-07-30.json`, WebView2 150.0.4078.65; `work/notes/observations/service-worker-registration-differs-by-ipfs-serving-origin-2026-07-30.md`). Engine capabilities on CUSTOM-SCHEME origins are SCHEME-GATED, and `ipfs://` is exactly the origin those three edges serve. So "the engine enables it and nobody disabled it" is not an argument about `localStorage` on an `ipfs://` origin at all. A machine-readable `implemented` on three unmeasured edges would have defused the one row added to stop this class of over-claim — and ADR-0005 records the same over-claim being corrected once already (the guard's original seed listed `implemented`-everywhere where the code said otherwise).

**Why `desktop` stays `implemented` while those three do not** (the line has to be drawn somewhere, so it is drawn on the record rather than on a hunch): desktop has an OBSERVATION on the origin it really serves — the human ran `mandalas.eth`, a site that uses `localStorage`, and it worked there while it was `null` on Android. That is behaviour, not an inference from a default. It is still weaker than Android's read-back (a site working is not the property being read), and the cell says so, names it the weaker of the two, and points at the follow-on that would upgrade it. Stubbing desktop too was the alternative and is defensible; it was rejected because it would discard the one piece of real-world evidence this task actually has and would make `stubbed` mean "nobody wrote a probe", which is broader than the signal it should carry.

**Alternatives considered:** (a) all five `implemented` — what was blocked, above; (b) a fourth cell state such as `unmeasured` — rejected as a unilateral change to ADR-0005's vocabulary that a storage fix has no business making, and it would need the guard, the ADR and every reader updated (a reasonable thing for a human to consider if this recurs); (c) stub desktop as well — defensible, rejected for the reason above.

**What it touches:** the parity guard's meaning of `stubbed` (see Decision 6), the follow-on task `matrix-web-platform-rows-are-measured-on-every-edge` (now the linked task for three cells, so it must resolve — it does, in `work/tasks/backlog/`), and any future edge that wants to flip a cell: the shape guard now demands an `EVIDENCE (<platform>):` line in the same change.

### 2. Further web-platform rows are AUTHORED as staged tasks, not filled in speculatively

**Chosen:** `work/tasks/backlog/` gains four follow-on tasks — `matrix-web-platform-row-indexeddb`, `matrix-web-platform-row-cookies`, `matrix-web-platform-row-service-workers`, `matrix-web-platform-rows-are-measured-on-every-edge` — instead of four more matrix rows filled from this task's Android-only measurements.

**Why:** a row filled with a prediction where a measurement belongs is a failure this repo has already paid for (both origin probes carry a recorded-verdict guard for exactly it). I measured IndexedDB and cookies on ANDROID only; writing five-platform rows off one platform's evidence would put four guesses into the file whose job is to be trustworthy. Tasks land in `work/tasks/backlog/` (staging) precisely so a human admits them, which is the right gate for "should the matrix grow this way?".

**What it touches:** the shape of the matrix, and a human's backlog. If the answer is "no, not those rows", dropping four staged tasks costs nothing; four wrong rows would have cost more.

### 3. The on-device probe PINS the pre-fix platform behaviour, it does not merely log it

**Chosen:** the probe asserts that with the shipped defaults `localStorage` is `null`, that `sessionStorage` is a working `Storage` object anyway, and that IndexedDB round-trips regardless — as assertions, not log lines.

**Why:** those are the facts the whole diagnosis rests on, and two of them contradict the widely-repeated claim that `domStorageEnabled` gates "DOM storage" as a whole. If a future WebView changes any of them, the honest outcome is a RED test on the next hand-run that makes someone re-read this directory — not a stale document nobody re-checks. It follows the sibling `SpaClientNavOriginTest`, which asserts the pre-fix opaque-origin behaviour the same way.

**What it touches:** whoever next runs `connectedDebugAndroidTest` on a much newer WebView. That test failing means the PLATFORM changed, not that werust broke, and the failure messages say so.

### 4. The gate asserts the audited settings are set by NOBODY, which couples a future UX change to this audit note

**Chosen:** `web_storage_edge_wiring_shape.rs` asserts that each of the seven audited `WebSettings` appears in `WEBSETTINGS-AUDIT.md` **and** that `BrowserActivity.kt` sets none of them.

**Why:** the task's deliverable was the LIST, unchanged, and this makes "unchanged" checkable rather than a claim in prose. It also means the audit note cannot rot silently: the day a human enables pinch-zoom, the gate reds until they update the note in the same change, which is exactly when the note is cheapest to update and most valuable to a reader.

**A reviewer could reasonably be surprised by this, so it is recorded rather than buried:** a UX task that enables `builtInZoomControls` now has to touch a test and a doc in this directory. That is deliberate friction, and it is small; the alternative is an audit note that describes a state of the world that stopped being true months ago. The assertion's failure message says what to do.

**Alternatives considered:** assert only that the audit lists the settings (weaker — the "changed nothing" half becomes unverified), or assert nothing and rely on review (weakest, and the class of thing review misses months later).

### 5. Coherence: "web-platform row" is a new KIND of row, not a new concept beside `capability`

The matrix's vocabulary is unchanged: `web-storage` is a `[[capability]]` with the same three cell states, validated by the same guard, with no new field or status. What is new is the KIND of thing a row can describe — a capability of the WEB PLATFORM rather than a werust feature — and that distinction is stated inside the row's own description, where a reader meets it, rather than minted as a separate mechanism. Nothing in `CONTEXT.md`'s glossary is re-meaned.

### 6. Coherence: `stubbed` here means NOT ESTABLISHED, which stretches ADR-0005's "known gap" reading

**Chosen:** the three unmeasured cells use `stubbed` + a task, the state ADR-0005 defines for "a known gap ... the matrix face of a no-op'd seam method".

**Why this is recorded rather than assumed:** on `macos` / `windows` / `ios` there is no no-op'd seam and no known defect — nothing suggests web storage fails there; nothing has shown it works. That is a THIRD thing, and the matrix's vocabulary has two: a claim, or a tracked gap. Given only those, a tracked gap is the honest one, because it is the state that cannot mislead a release reader and it forces a linked task that resolves it. Each cell's prose says explicitly that `stubbed` means NOT ESTABLISHED rather than "known broken", so the stretch is visible where it is read, not buried here.

**What it touches:** ADR-0005's vocabulary as READ (not as written — no file of it changed), and anyone scanning the matrix for real gaps, who will now find three cells that may turn out to be fine. If this recurs across several rows, minting a real `unmeasured` state (with the guard and the ADR updated together) is the clean fix; one row does not justify it.

### 7. A per-cell `EVIDENCE (<platform>):` line is now a CHECKED convention of this row

**Chosen:** the row's comment block gives one `EVIDENCE (<platform>):` paragraph per platform, and `web_storage_edge_wiring_shape.rs` reds if any of the five is missing.

**Why:** the failure this task was requeued for was not a wrong cell, it was a cell whose backing was never written down, so nobody could see it was an inference. A marker makes "what backs this?" answerable by looking, and the guard makes flipping a cell without answering it impossible. The convention is scoped to the `web-storage` row and its test; it invents no matrix FIELD (a comment, not schema), so ADR-0005's format and the parity guard are untouched.

**What it touches:** `matrix-web-platform-rows-are-measured-on-every-edge` and the three sibling row tasks, which now inherit the marker as the worked example — and any change that flips a `web-storage` cell, which must update the matching evidence line in the same commit. Whether the other 24 rows should adopt it is a human's call and is deliberately not done here.

## What this fix does NOT do

**Storage still does not survive a site UPDATE, on any platform.** Storage is keyed by origin and werust's origins are content-addressed, so a new CID is a new origin and a dapp's saved state becomes unreachable on every publish. That is inherent to content addressing rather than a bug this fix leaves behind, and the interesting alternative (key by the stable mutable NAME, with the TOFU pin gating a repoint) is a design question a human owns. It is recorded in the finding note and deliberately untouched here.
