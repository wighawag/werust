---
title: "Every edge gets a real settings location, through one non-env seam, and a relocation migrates instead of resetting"
slug: settings-location-on-every-edge
taskedAfter: [android-instrumented-ci-leg]
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/ready/` tasks.

## Problem Statement

`settings_dir()` resolves `WERUST_SETTINGS_DIR` -> `XDG_CONFIG_HOME` -> `$HOME/.config/werust` -> `None`. That leaves werust's persisted state broken or absent on three of five edges, and both files it governs (`retrieval.json`, `pins.json`) share the fault:

- **Android** — no usable fallback, so nothing persists. A trust decision is lost on restart (reported in v0.4.0 field use) and the retrieval-backend choice cannot take effect at all, already recorded in `retrieval-backend-setting-cannot-take-effect-on-mobile-2026-07-28.md`, which verified the cause on device by reading `/proc/<pid>/environ`.
- **Windows** — a normal session sets `%USERPROFILE%` and `%LOCALAPPDATA%` but **not** `HOME`, so `settings_dir()` returns `None`. Recorded in `settings-dir-has-no-windows-branch-2026-07-30.md`. Windows has no persisted settings at all.
- **iOS** — `HOME` IS set, to the app sandbox container, so it probably DOES persist, in a non-idiomatic place with different backup semantics. This must be VERIFIED per edge rather than inherited from the Android diagnosis.

This is a prerequisite, not a detail. `withhold-changed-content-until-trusted` makes a trust record decide what loads; a record that does not persist means the protection silently does not exist on that edge, and a record whose location moves without migrating is a one-shot trust reset.

**The obvious mechanism is unsafe on the edge that needs it most.** The existing production lever is the `WERUST_SETTINGS_DIR` env var. Kotlin and Swift cannot set a process env var, so satisfying it on Android would require `std::env::set_var` from the JNI entry point — and this repo's own code calls that "a data race / UB" (`crates/werust-core/src/retrieval.rs`, the settings tests' comment), on the edge documented as calling into the core from WebView worker threads concurrently with the UI thread. The env route is therefore rejected for production.

## Solution

### One explicit, non-env seam that moves every persisted file together

An explicit settings-location value covering **all** persisted state: `retrieval.json`, `pins.json`, and the block store `blessed-version-content-retention` will add. Supplied without process-global mutation, so it is safe on a multi-threaded edge and usable from a test without touching env.

**It cannot be a shell-level builder, and that is the load-bearing design point.** The obvious shape — a production sibling of the test-only `with_pins_dir` — is structurally incapable of doing the job on the three desktop edges: each of them calls `install_ipfs` on the BACKEND, which resolves the active retrieval endpoint once, BEFORE the shell object exists at all. A builder applied after the shell is constructed could never influence the endpoint that was already chosen. (Mobile differs — there the install happens inside session construction — which is why a shell-level framing reads fine for mobile and is wrong for desktop.)

So the location must be resolvable at the point the backend is built, ahead of the shell: an explicit value threaded into the retrieval-endpoint resolution rather than a post-construction setter. `with_pins_dir` is superseded for production either way, since it moves only `pins.json` and would fork the "one directory, one mechanism" property the code documents while leaving the retrieval half broken. Whether it survives as a narrow test helper or is folded into the new value is settled by decision 2, not left open.

Each edge supplies its platform-idiomatic location:

- **Android** — the app-private files directory, threaded through the FFI. Note the native constructor currently takes NO arguments and is called from a field initialiser, so this changes the JNI signature, the Kotlin class's construction, and every construction site including the instrumented tests.
- **iOS** — the app support directory in the container, likewise.
- **Windows** — `%LOCALAPPDATA%\werust`, as a new branch in the resolution.
- **Desktop/macOS** — unchanged (`XDG_CONFIG_HOME` / `HOME`), so nothing regresses.

`WERUST_SETTINGS_DIR` remains as an explicit operator override, which is what it is good for.

### Ordering is load-bearing

`install_ipfs` resolves the active retrieval endpoint ONCE, at session construction. A location supplied after the session is built would fix the trust store while silently leaving the retrieval choice on the default. The location must therefore be supplied BEFORE session construction, and that ordering is a tested property, not a comment.

### A relocation migrates

Reading a store at a new location that does not exist yet is indistinguishable from a fresh install, and an empty trust store means every previously trusted name reads as untrusted. Under the auto-recording in `withhold-changed-content-until-trusted` that would silently re-record every name at whatever it points to now — a browser-wide, one-shot acceptance window on upgrade day. So the move reads the legacy location when the new one is absent and carries it forward, idempotently, inside the SAME load path every reader uses.

## User Stories

1. As a phone user, I want my trust decisions and my retrieval-backend choice to survive restarts and app updates, so that werust does not silently forget what I chose.
2. As a Windows user, I want werust to have a settings location at all, so that the trust protection and the retrieval setting are not silently inoperative on my platform.
3. As an iOS user, I want my settings in the platform-idiomatic container location with correct backup semantics, rather than wherever `HOME` happened to point.
4. As a user upgrading to the release that moves where settings live, I want my existing trust decisions and retrieval choice carried across, so that the upgrade is not a silent reset.
5. As a werust developer, I want the location supplied through an explicit seam rather than a process env var, so that the mechanism is not a data race on the multi-threaded edge.
6. As a werust developer, I want the retrieval choice and the trust store to move together, so that fixing one does not silently leave the other broken.
7. As a werust developer, I want a test to prove the location is supplied BEFORE session construction, so that a correctly-set location cannot still leave the retrieval endpoint on its default.
8. As a developer running the test suite, I want every session-constructing test to be able to isolate its settings location without touching env, so that tests never read or write my real store and never race each other.

## Implementation Decisions

1. **An explicit settings-location seam, not the env var, for production.** Env mutation from JNI is a documented data race on Android. The seam takes the directory at construction and governs every persisted file.

2. **`with_pins_dir` is FOLDED INTO the new location value and removed as a separate concept.** Leaving it as a parallel test-only path means core tests isolate one file through one mechanism while production and the retention store use another, which is how the two drift. Every test that isolates storage does so through the same value production uses. This matters because two downstream specs write isolation rules that say "through the settings-location seam", and they need one seam to mean one thing.

3. **Windows gets a real branch: `%LOCALAPPDATA%\werust`.** That is the platform-correct home for app state, and it closes the gap that leaves Windows with no persisted settings.

4. **The location is resolvable BEFORE the retrieval endpoint is, and that is tested.** The endpoint is resolved once, at backend build time on desktop and inside session construction on mobile. The test asserts the endpoint a constructed session reports reflects a location supplied ahead of it — which fails against a post-construction builder, and is precisely why decision 1 rejects that shape.

5. **Migration lives inside the single load path both readers and writers use, and is idempotent.** Putting it only at construction would leave the re-read that the auto-record performs looking at an empty new location — reintroducing the reset the migration exists to prevent.

6. **A migration that cannot WRITE is surfaced, not swallowed.** The store's save returns a bool that every caller currently discards. A non-durable migration silently re-opens the reset window on the next launch, so the failure must be reported at least once rather than inferred.

7. **iOS's current behaviour is diagnosed, not assumed.** The task confirms where the settings directory resolves on a real iOS build before changing it, so the migration source is known rather than guessed.

8. **The capability matrix's retrieval-backend row is corrected here.** That row currently records the setting as implemented on all five edges, while this spec's own evidence shows it cannot take effect on Android and has nowhere to persist on Windows. Fixing the mechanism without fixing the row would leave the matrix asserting a capability the work exists because werust does not have. Whether persisted-settings-location becomes its own row or the existing row's honest-limit note is amended is the task's call, but silence is not.

## Testing Decisions

- **Resolution per platform** is unit-testable on the Linux gate ONLY IF the resolver is a pure function of its inputs rather than reading process env directly, which it currently is not. Extracting that pure resolver is the first piece of work, otherwise a Windows-only branch is invisible to the only acceptance bar this repo has and its acceptance criterion would be untestable.
- **Ordering (decision 4)** is tested by asserting the retrieval endpoint resolved by a session reflects a location supplied before construction.
- **Migration (decision 5)** is tested through the SAME load path the auto-record uses — writing a legacy store, then asserting the re-read path sees it, not just that construction saw it. Testing only construction would pass in the broken arrangement.
- **Android persistence** names `.github/workflows/android-instrumented.yml`. The criterion is NOT "the app restarted": an instrumented test cannot restart its own process, and recreating the Activity proves nothing because the in-memory store survives it either way — the test would pass in the broken arrangement. The checkable form is: the store file exists at the app-private path after a write, AND a freshly constructed native session given that directory reads the record back. Note this is the first instrumented case to construct a core session, so it is new harness work rather than adding a case to an established pattern.
- **iOS** is build-verified on `mobile-ios.yml` plus a Swift source-shape guard (the established pattern for reading Swift from a pure-Rust test) plus the diagnosis in decision 7. Note the story's "correct backup semantics" is NOT assertable by anything in this repo — nothing drives the iOS app — so that clause is documented as a design intent, not written as an acceptance criterion.
- **Isolation:** tests supply a scratch location through the seam with no env mutation, and assert the developer's real store is untouched (the shared-write rule).

## Out of Scope

- **The withhold behaviour itself** — `withhold-changed-content-until-trusted`, which is `taskedAfter` this spec because auto-recording into a store that does not persist is pointless.
- **The block store's contents** — `blessed-version-content-retention`. This spec only establishes the directory it will live in.
- **Making `mobile-ios.yml` drive the app.** Build verification plus the diagnosis is the bar here.
- **A settings UI.** The location is not user-facing.
