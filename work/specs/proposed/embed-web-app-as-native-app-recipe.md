---
title: "werust: embed-a-web-app-as-a-native-app — a RECIPE (not a framework) reusing the shell + build machinery"
slug: embed-web-app-as-native-app-recipe
needsAnswers: true
taskedAfter: []
---

> PROPOSED spec \u2014 records intent for human review before tasking. A DIFFERENT axis from the
> browser specs: those make werust a BROWSER (a user opens arbitrary content); THIS makes
> werust an app-PACKAGING substrate (a developer ships THEIR web app as a native desktop +
> mobile app). Tauri-like in outcome, but explicitly a RECIPE, not a framework. Not tasked.

## Problem Statement

A developer with a web app (an SPA / static site / local web UI) should be able to ship it as
a native DESKTOP and MOBILE app by REUSING werust's existing shell + cross-compile/packaging
machinery \u2014 pointing the shell at THEIR app instead of arbitrary URLs. Tauri does this as a
whole FRAMEWORK (WRY webview abstraction, an IPC bridge, a permissions system, a bundler,
`create-tauri-app`, a plugin ecosystem). werust should deliberately NOT become that. The ask
is the MINIMUM: at most a RECIPE (a documented procedure + a thin config + the reusable
build steps) a dev follows to embed their app, reusing the mechanism werust already has to
produce desktop + Android + iOS artifacts.

## The load-bearing distinction: RECIPE, not FRAMEWORK

This is the whole design tension; get it right or werust accretes a framework it does not want.

- **A FRAMEWORK (what to AVOID)** owns the developer's app: a bespoke IPC/command API surface
  (`invoke`/commands), a permissions/capabilities DSL, a plugin system, a project generator,
  a large maintained API the dev programs AGAINST and is locked into. That is Tauri's value
  AND its weight. werust does not want to own/maintain that surface.
- **A RECIPE (what to BUILD)** is: "here is how to point werust's EXISTING shell at your app,
  and here are the EXISTING build steps to produce the artifacts." At most a thin manifest
  (app name/id/icon, the entry point, window defaults) + docs + a reuse of the desktop shell
  crate and the Android/iOS scaffolds already in the repo. The dev's app stays a plain web
  app; werust is the wrapper, not the API they code against. No new IPC/plugin/permission
  framework. If the app needs to talk to native code, that is EXPLICITLY out of scope for the
  recipe (see Out of Scope) \u2014 the recipe wraps a self-contained web app.

Litmus test for every proposed piece: "does this add API SURFACE the dev programs against, or
does it just REUSE an existing werust mechanism to package their app?" Only the latter passes.

## What werust ALREADY has to reuse (this is why it is a recipe, not a build-out)

- **Desktop shell** (`crates/werust`): a GTK window + the WebKitGTK `Renderer` backend + the
  `werust-core` chrome, today navigating a URL. The recipe points it at the dev's app entry.
- **`werust-core`**: the shared browsing/shell logic already extracted as a linkable crate
  (desktop + Android + iOS all link it) \u2014 the exact "shared core, many OS edges" shape an
  embedded app wants.
- **Mobile app modules** (`crates/werust-android`, `crates/werust-ios`): real Gradle + Xcode
  projects that cross-compile the Rust core and package an APK / Simulator `.app`, with
  build-leg checks. The recipe reuses these as the mobile packaging path.
- **Release machinery** (`.goreleaser.yaml` + `release.yml`): already produces desktop +
  APK + iOS artifacts. The recipe reuses the packaging, parameterised for the dev's app.

So "embed a web app as a native app" is mostly REPACKAGING what exists to point at the dev's
app + serve it, NOT a green-field framework.

## Solution (shape, not final)

1. **Serve the dev's app to the webview.** Two modes (see open Q): (a) BUNDLED static assets
   served from a custom app-scheme (Tauri-style, no localhost HTTP server \u2014 reuse the
   `Renderer` custom-scheme hook werust already has for `ipfs://`), or (b) point at a
   dev/prod URL. Bundled + custom-scheme is the privacy/robustness-preferred default.
2. **A thin app manifest.** Name, bundle id, icon, entry point, window defaults (size/title),
   target platforms. NOT a permissions/plugin DSL \u2014 just packaging metadata. Likely a small
   TOML the recipe reads (reusing werust's config style).
3. **Reuse the shell as the app window.** The dev's app runs in the werust shell window;
   for an EMBEDDED-APP build the browser chrome (URL bar / back-forward) is hidden/optional
   (an app, not a browser), while the same `werust-core` + webview backend drive it.
4. **Reuse the build/packaging paths.** The Android/iOS scaffolds + GoReleaser leg,
   parameterised by the manifest, produce the dev's desktop binary + APK + iOS `.app`.
5. **Documented as a RECIPE.** The primary deliverable is DOCS (a `docs/` how-to +
   `docs/spikes` proof) + the thin manifest support + whatever minimal shell parameterisation
   the reuse needs \u2014 not a new crate the dev depends on as a framework.

## User Stories

1. As a web-app developer, I follow a documented recipe to wrap my existing web app as a
   native desktop app, reusing werust's shell \u2014 without adopting a framework or rewriting my
   app.
2. As a developer, the SAME recipe + build machinery produces Android and iOS apps from my
   web app.
3. As a developer, I ship my app's assets bundled (served via a custom scheme, no localhost
   server) so the app works offline and does not expose an HTTP port.
4. As a developer, I provide a thin manifest (name/id/icon/entry/window) \u2014 not a
   permissions/plugin config \u2014 and get native artifacts.
5. As the werust maintainer, adding this does NOT saddle werust with a framework API surface
   (IPC/plugins/permissions) to maintain; it reuses existing mechanisms.

## Relationship to the other specs

- Reuses the `Renderer` custom-scheme hook (same one `ipfs://` uses) to serve bundled assets.
- The subsystem/privacy/ENS/Freenet specs are BROWSER features; an embedded app is a
  different product mode. An embedded-app build likely SHIPS WITHOUT the browser chrome and
  MAY ship without the decentralised subsystems (a plain-app build), OR a dev could opt into
  them \u2014 an open question about how much of the browser an embedded app inherits.
- The `werust-core` extraction + mobile scaffolds (done for the browser's own mobile apps)
  are the exact substrate this reuses \u2014 a nice payoff of that earlier architecture.

## Phased delivery (proposed, for review)

- **Phase 0 \u2014 spike + recipe proof:** manually wrap ONE sample web app as a werust desktop
  app (point the shell at bundled assets via the custom scheme, hide the chrome), documenting
  every step. Output: a findings/recipe doc + the minimal shell parameterisation it needed.
  Proves "recipe, not framework" is sufficient.
- **Phase 1 \u2014 desktop recipe:** the thin manifest (name/id/icon/entry/window) + custom-scheme
  bundled-asset serving + chrome-off app mode + a documented desktop build. A dev can ship a
  desktop app from their web app.
- **Phase 2 \u2014 mobile recipe:** parameterise the Android/iOS scaffolds by the manifest so the
  same web app produces an APK + iOS `.app`; documented.
- **Phase 3 \u2014 polish:** icon/splash/window ergonomics, a `create`-style helper ONLY IF it
  stays a scaffolding convenience (not a framework runtime), release-pipeline parameterisation
  per app.

## Out of Scope (for this spec)

- A Tauri-style **framework**: no bespoke IPC/`invoke`/command API, no permissions/capabilities
  DSL, no plugin system, no large maintained runtime API the dev codes against. If the app
  needs deep native integration, werust's recipe is NOT the tool \u2014 it wraps a SELF-CONTAINED
  web app. (This is the deliberate boundary; revisit only via a new spec + ADR.)
- Native-API bridges (filesystem/notifications/etc. exposed to JS) \u2014 that IS framework
  surface; explicitly excluded from the recipe.
- The browser identity itself (arbitrary-URL browsing, decentralised subsystems) \u2014 an embedded
  app is a different mode reusing the same substrate.

## OPEN QUESTIONS (needsAnswers: true)

1. **Recipe vs. thin-crate boundary.** Where exactly is the line? Purely docs + a manifest the
   existing shell reads, or a small `werust-app` helper crate/CLI that does the packaging? The
   moment it grows an API the dev programs AGAINST, it has become a framework \u2014 how do we keep
   it a recipe while still being usable? (Recommend: docs + manifest + a thin scaffolding CLI
   that emits a project reusing the existing crates, with NO runtime API surface.)
2. **Asset serving mode.** Bundled-static-via-custom-scheme (offline, no port \u2014 recommended
   default) vs. point-at-URL vs. both? How are assets bundled into the desktop binary / APK /
   `.app`?
3. **How much browser does an embedded app inherit?** Chrome hidden by default (it is an app)
   \u2014 confirm. Do the decentralised subsystems (ipfs/ENS/Freenet) ship in an embedded-app build
   at all, or is that a plain-app build with them compiled out? (Likely: off by default,
   opt-in.)
4. **Manifest scope.** Exactly which fields (name/id/icon/entry/window/platforms) \u2014 and hold
   the line that it is PACKAGING metadata, never a permissions/plugin config.
5. **No-native-bridge stance.** Confirm the recipe deliberately does NOT provide a JS<->native
   bridge (that is the framework line). A dev needing native code is out of scope \u2014 acceptable?
6. **Maintenance model.** Is this maintained as first-class (a supported way to build apps on
   werust) or as an example/recipe that may lag? (Affects how much shell parameterisation is
   worth building vs. documenting.)
7. **Mobile signing/store.** Recipe covers unsigned/dev artifacts (as the browser's own mobile
   build does); production signing/store submission is the dev's job \u2014 confirm.

## Why this fits werust

werust ALREADY built the hard parts for its OWN mobile apps: a shared linkable core
(`werust-core`), real Android/iOS scaffolds that cross-compile + package it, a webview shell,
and a release pipeline. "Let a developer point that at THEIR web app" is a high-leverage reuse
of that investment \u2014 and doing it as a RECIPE rather than a framework keeps werust lean (no
IPC/plugin/permission API to own) while giving devs a Tauri-like outcome on werust's stack. It
also dogfoods the shell/build machinery, surfacing rough edges that improve the browser too.
