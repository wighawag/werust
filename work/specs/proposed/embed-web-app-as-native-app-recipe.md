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

## Second principle: build the COMPONENTS so the recipe stays simple

"Recipe, not framework" does NOT mean "leave the complexity in the recipe." The opposite: the
recipe is simple BECAUSE the werust components are BUILT to be embeddable. Design-for-
embedding is a requirement ON THE COMPONENTS, not documentation bolted on afterward. Concretely:

- The desktop shell must expose a clean "run as an embedded app pointed at THESE bundled
  assets, chrome off" entry — not force the dev to fork `main.rs`. If wrapping an app needs a
  hand-edit of a shell internal, that is a COMPONENT bug: fix the component to take the
  manifest, so the recipe is just "provide a manifest + assets."
- The mobile scaffolds must be parameterisable by the manifest (app id/name/icon/entry), not
  hand-edited per app.
- The custom-scheme asset serving must be a reusable, drop-in mechanism.

The test: adding a feature that makes the RECIPE shorter/simpler by pushing the work into a
well-shaped COMPONENT is GOOD and in scope; adding developer-facing API surface is NOT. Keep
the dev's steps minimal by making the substrate do the work, cleanly.

## Scope: MINIMAL — a static page embedded in an app, like Tauri's core

This is deliberately the SMALL version: serve a self-contained STATIC web app (HTML/CSS/JS
assets) embedded in the native app and render it in the shell window — that is it, the Tauri
"webview shows your bundled frontend" core. It ships NONE of werust's browser machinery: NO
ipfs/ENS/Freenet subsystems, NO decentralised schemes, NO trust indicator, NO privacy
routing, NO arbitrary-URL browsing. An embedded-app build is a PLAIN static-page app; the
decentralised/browser features are compiled OUT (or simply never wired) for this mode. If a
dev ever wants those, that is a different, later conversation — not this recipe.

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
5. **A scaffold CLI that emits a project and SYNCS to latest.** The dev's entry point is a
   scaffolding CLI (e.g. `werust-app new <name>` / `create-werust-app`) that GENERATES a thin
   project wiring their static assets + manifest to the reusable werust shell + build scaffolds
   (like `create-tauri-app`, but it emits a project that REUSES werust components — it is NOT a
   runtime API). Crucially it also SYNCS: a `werust-app update`/`sync` verb pulls the LATEST
   werust shell/build-scaffold version into an existing project (bump the pinned werust
   version; refresh the regenerable scaffold files) so a dev is never stranded on an old
   werust. This is what keeps the recipe simple over TIME: the scaffold is generated +
   updatable, not copy-pasted once and left to rot. Scaffolding + version-sync ONLY — no
   runtime API the app codes against (that would be a framework).
6. **Documented as a RECIPE.** Alongside the CLI, DOCS (a `docs/` how-to + a `docs/spikes`
   proof) + the thin manifest support + whatever minimal shell/scaffold parameterisation
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
6. As a developer, I scaffold my app project with a CLI (`werust-app new` /
   `create-werust-app`), and later run its `update`/`sync` verb to pull the latest werust
   shell/build version into my project, so I stay current without re-scaffolding by hand.
7. As a developer, my embedded app is a PLAIN static-page app: no ipfs/ENS/Freenet, no browser
   chrome, no arbitrary browsing — just my bundled frontend in a native window, like Tauri.

## Relationship to the other specs

- Reuses the `Renderer` custom-scheme hook (same one `ipfs://` uses) to serve bundled assets.
- The subsystem/privacy/ENS/Freenet specs are BROWSER features; an embedded app is a
  different, MINIMAL product mode that ships WITHOUT them: no browser chrome, no decentralised
  subsystems, no arbitrary browsing (DECIDED — minimal static-page embed, per the human). They
  are compiled out / never wired for an embedded-app build \u2014 an open question about how much of the browser an embedded app inherits.
- The `werust-core` extraction + mobile scaffolds (done for the browser's own mobile apps)
  are the exact substrate this reuses \u2014 a nice payoff of that earlier architecture.

## Phased delivery (proposed, for review)

- **Phase 0 \u2014 spike + recipe proof:** manually wrap ONE sample web app as a werust desktop
  app (bundled assets via the custom scheme, chrome off). The OUTPUT is not just docs — it is
  the COMPONENT changes that make it clean: a shell entry taking "embedded-app mode + these
  assets + no chrome" WITHOUT forking `main.rs`. Proves the recipe stays simple BECAUSE the
  substrate does the work.
- **Phase 1 \u2014 desktop recipe:** the thin manifest (name/id/icon/entry/window) + custom-scheme
  bundled-asset serving + chrome-off app mode + the `werust-app new` CLI that emits a desktop
  project reusing the shell. A dev scaffolds + ships a desktop app from their static web app.
- **Phase 2 \u2014 mobile recipe:** parameterise the Android/iOS scaffolds by the manifest so the
  same static app produces an APK + iOS `.app` (via the same CLI).
- **Phase 3 \u2014 the SYNC verb + polish:** `werust-app update`/`sync` pulls the latest werust
  shell/build version into an existing project (version bump + regenerable-scaffold refresh);
  icon/splash/window ergonomics; per-app release-pipeline parameterisation. Scaffolding + sync
  only, never a runtime API.

## Out of Scope (for this spec)

- A Tauri-style **framework**: no bespoke IPC/`invoke`/command API, no permissions/capabilities
  DSL, no plugin system, no large maintained runtime API the dev codes against. If the app
  needs deep native integration, werust's recipe is NOT the tool \u2014 it wraps a SELF-CONTAINED
  web app. (This is the deliberate boundary; revisit only via a new spec + ADR.)
- Native-API bridges (filesystem/notifications/etc. exposed to JS) \u2014 that IS framework
  surface; explicitly excluded from the recipe.
- The browser identity itself (arbitrary-URL browsing, decentralised subsystems) \u2014 an embedded
  app is a different mode reusing the same substrate.

## DECISIONS CONFIRMED BY THE HUMAN (2026-07-22)

- **Recipe made simple by embeddable COMPONENTS** (design-for-embedding is a requirement on
  the shell/scaffolds, not docs bolted on).
- **A scaffold CLI (`werust-app new` / `create-werust-app`) that SYNCS to latest** (an
  `update`/`sync` verb): scaffolding + version-sync only, no runtime API.
- **MINIMAL scope: a static page embedded in an app, like Tauri's core.** NO ipfs/ENS/Freenet,
  no browser chrome, no arbitrary browsing; the browser/decentralised features are compiled
  out / never wired for an embedded-app build.
- **No JS<->native bridge**: the recipe wraps a self-contained static web app; deep native
  integration is out of scope (the framework line).

## OPEN QUESTIONS (needsAnswers: true)

1. **Scaffold-CLI sync MECHANISM.** How does `update`/`sync` work: a pinned werust version in the scaffolded project that the verb bumps + re-emits the regenerable scaffold files (overwriting scaffold-owned files, leaving the dev's assets/manifest untouched)? How is "scaffold-owned vs dev-owned" delineated so a sync never clobbers the dev's code? (The crux of keeping the recipe simple over time; get the regeneration boundary right.)
2. **Recipe boundary (residual).** Decided: docs + manifest + a thin scaffolding/sync CLI reusing the existing crates, NO runtime API. Residual: does the emitted project VENDOR the scaffold files or DEPEND on a published werust template/crate version?
3. **Asset serving mode.** Bundled-static-via-custom-scheme (offline, no port; recommended default) vs. point-at-URL vs. both? How are the static assets bundled into the desktop binary / APK / `.app`?
4. **Compiling the browser features OUT (decided WHETHER, open HOW).** Chrome-off + no ipfs/ENS/Freenet is DECIDED; the open question is the MECHANISM: Cargo features on `werust-core`/the shell, or a separate thin app-shell binary linking only the minimal render+window path, so a static-embed build carries none of that code.
5. **Manifest scope.** Exactly which fields (name/id/icon/entry/window/platforms), holding the line that it is PACKAGING metadata, never a permissions/plugin config.
6. **Maintenance model.** First-class (a supported way to build apps on werust) or an example/recipe that may lag? (The design-for-embedding principle already pushes toward first-class.)
7. **Mobile signing/store.** Recipe covers unsigned/dev artifacts (as the browser's own mobile build does); production signing/store submission is the dev's job; confirm.

## Why this fits werust

werust ALREADY built the hard parts for its OWN mobile apps: a shared linkable core
(`werust-core`), real Android/iOS scaffolds that cross-compile + package it, a webview shell,
and a release pipeline. "Let a developer point that at THEIR web app" is a high-leverage reuse
of that investment \u2014 and doing it as a RECIPE rather than a framework keeps werust lean (no
IPC/plugin/permission API to own) while giving devs a Tauri-like outcome on werust's stack. It
also dogfoods the shell/build machinery, surfacing rough edges that improve the browser too.
