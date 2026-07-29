# Loading banner (phase + cancel) — manual verification steps

Task: `loading-banner-with-phase-and-cancel`. The chrome JSON already carries
`loading` + `loadStep`; this task adds the visible BANNER on each platform that
the existing chrome-refresh pump drives (no new timer / poll / tight loop — the
Android ANR guard is not regressed).

The chrome-state → phase-name mapping is asserted by a display-free Rust unit
test on desktop (`loading_banner_names_the_phase_while_a_load_is_in_flight_and_hides_when_settled`
in `crates/werust/src/main.rs`). The mobile mappings live in Kotlin/Swift
(`WerustCore.Chrome.loadingBannerText`), which this repo has no JVM/native
unit-test harness for, so the visible banner on each platform is verified by the
manual steps below.

## Shared expectation (all platforms)

While a load is in flight a NON-BLOCKING banner appears directly under the
toolbar and above the page view, naming the current pipeline phase and offering
a **Cancel**. The phase names are the existing `LoadStep` vocabulary verbatim
(capitalised + ellipsised for the banner):

| `LoadStep`        | banner text          |
| ----------------- | -------------------- |
| `ResolvingName`   | `Resolving name…`    |
| `FetchingRecord`  | `Fetching record…`   |
| `FetchingContent` | `Fetching content…`  |
| `Rendering`       | `Rendering…`         |
| `Idle` (loading)  | `Loading…`           |

The banner:

- appears as soon as `chrome.is_loading()` is true;
- updates as the phase advances (resolve → fetch → render);
- disappears on `Finished` / `Failed` / `Idle`;
- its **Cancel** calls the SAME `core.stop()` the toolbar Stop button uses (no
  new mechanic);
- is driven by the existing chrome-refresh pump (no new timer / poll).

## Desktop (WebKitGTK, `crates/werust`)

1. `cargo run -p werust -- ipfs://<a slow cid>` (or navigate to a `ronan.eth`
   page that exercises ENS resolution).
2. While the load is in flight: a blue banner appears under the toolbar reading
   the current phase (e.g. `Resolving name…` → `Fetching content…` →
   `Rendering…`) with a **Cancel** button at the right.
3. As the phase advances the label updates in place (driven by the 50 ms GTK
   pump — no new timer).
4. On finish the banner disappears; the trust indicator + footer status settle.
5. Start another load and click **Cancel** in the banner: the load aborts (the
   same as clicking the toolbar ✕ Stop), the banner disappears, the chrome
   returns to idle.
6. Navigate to a URL that fails (e.g. an unsupported `ipfs://` Swarm hash): the
   loading banner hides and the red error banner takes the slot (the two are
   mutually exclusive).

## Android (`crates/werust-android`)

1. Build + install the debug APK on a device/emulator:
   `./gradlew :app:installDebug` (from `crates/werust-android`).
2. Open the app and navigate to a `.eth` page (e.g. `ronan.eth`) that triggers a
   multi-phase retrieval.
3. While loading: a blue banner under the toolbar names the phase with a
   **Cancel** button at the right. It updates as the phase advances, driven by
   the existing `refreshChrome()` cadence (event-driven, off the UI thread for
   core work — the ANR guard is not regressed).
4. On finish/failure the banner hides; a failure shows the red error banner in
   the same slot.
5. Tap **Cancel** mid-load: `core.stop()` runs (inline, non-blocking) and the
   banner hides.

## iOS (`crates/werust-ios`)

1. `BUILD_ONLY=1 crates/werust-ios/build-and-run.sh` then launch on the iOS
   Simulator, or `crates/werust-ios/build-and-run.sh` to build + launch.
2. Navigate to a `.eth` page that triggers a multi-phase retrieval.
3. While loading: a blue banner under the toolbar names the phase with a
   **Cancel** button at the right, updating as the phase advances on the
   existing `refreshChrome()` cadence (no new timer).
4. On finish/failure the banner hides; a failure shows the red error banner in
   the same slot.
5. Tap **Cancel** mid-load: `onStop` runs (`core.stop()`) and the banner hides.