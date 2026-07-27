<!-- dorfl-sidecar: item=task:general-browser-menu-with-version-and-debug-entry type=task slug=general-browser-menu-with-version-and-debug-entry allAnswered=false -->

## Q1

**'task:general-browser-menu-with-version-and-debug-entry' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The menu's version line will read 'werust 0.0.0' in every shipped build, so the feature the task exists for (show the werust VERSION) does not actually show it. The workspace Cargo version is 0.0.0 and nothing bumps it at release: git show v0.2.4:Cargo.toml and v0.2.6:Cargo.toml both carry version = 0.0.0, and the GoReleaser rust builder derives the RELEASE name from the tag without rewriting Cargo.toml or injecting a version. So on v0.2.6 all three menus (and the startup banner) say 0.0.0. Either plumb the real version (release-time bump or a build-time version from the tag) or record this explicitly and file the follow-up task; note DECISIONS.md Decision 2 says the Gradle versionName is a hand-maintained 0.0.0 that does NOT track the workspace version, which implies the workspace version is meaningful, and it is not. (Cargo.toml:16 version = 0.0.0; git show v0.2.6:Cargo.toml line 16; .goreleaser.yaml builds with plain cargo build, no version injection; crates/werust-core/src/lib.rs version() = env!(CARGO_PKG_VERSION))
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):

## Q2

**'task:general-browser-menu-with-version-and-debug-entry' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The menu's version line will read 'werust 0.0.0' in every shipped build, so the feature the task exists for (show the werust VERSION) does not actually show it. The workspace Cargo version is 0.0.0 and nothing bumps it at release: git show v0.2.4:Cargo.toml and v0.2.6:Cargo.toml both carry version = 0.0.0, and the GoReleaser rust builder derives the RELEASE name from the tag without rewriting Cargo.toml or injecting a version. So on v0.2.6 all three menus (and the startup banner) say 0.0.0. Either plumb the real version (release-time bump or a build-time version from the tag) or record this explicitly and file the follow-up task; note DECISIONS.md Decision 2 says the Gradle versionName is a hand-maintained 0.0.0 that does NOT track the workspace version, which implies the workspace version is meaningful, and it is not. (Cargo.toml:16 version = 0.0.0; git show v0.2.6:Cargo.toml line 16; .goreleaser.yaml builds with plain cargo build, no version injection; crates/werust-core/src/lib.rs version() = env!(CARGO_PKG_VERSION))
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q2 fields: id=q2 kind=stuck -->

**Your answer** (write below this line):

## Q3

**'task:general-browser-menu-with-version-and-debug-entry' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The menu's version line will read 'werust 0.0.0' in every shipped build, so the feature the task exists for (show the werust VERSION) does not actually show it. The workspace Cargo version is 0.0.0 and nothing bumps it at release: git show v0.2.4:Cargo.toml and v0.2.6:Cargo.toml both carry version = 0.0.0, and the GoReleaser rust builder derives the RELEASE name from the tag without rewriting Cargo.toml or injecting a version. So on v0.2.6 all three menus (and the startup banner) say 0.0.0. Either plumb the real version (release-time bump or a build-time version from the tag) or record this explicitly and file the follow-up task; note DECISIONS.md Decision 2 says the Gradle versionName is a hand-maintained 0.0.0 that does NOT track the workspace version, which implies the workspace version is meaningful, and it is not. (Cargo.toml:16 version = 0.0.0; git show v0.2.6:Cargo.toml line 16; .goreleaser.yaml builds with plain cargo build, no version injection; crates/werust-core/src/lib.rs version() = env!(CARGO_PKG_VERSION))
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q3 fields: id=q3 kind=stuck -->

**Your answer** (write below this line):

## Q4

**'task:general-browser-menu-with-version-and-debug-entry' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The menu's version line will read 'werust 0.0.0' in every shipped build, so the feature the task exists for (show the werust VERSION) does not actually show it. The workspace Cargo version is 0.0.0 and nothing bumps it at release: git show v0.2.4:Cargo.toml and v0.2.6:Cargo.toml both carry version = 0.0.0, and the GoReleaser rust builder derives the RELEASE name from the tag without rewriting Cargo.toml or injecting a version. So on v0.2.6 all three menus (and the startup banner) say 0.0.0. Either plumb the real version (release-time bump or a build-time version from the tag) or record this explicitly and file the follow-up task; note DECISIONS.md Decision 2 says the Gradle versionName is a hand-maintained 0.0.0 that does NOT track the workspace version, which implies the workspace version is meaningful, and it is not. (Cargo.toml:16 version = 0.0.0; git show v0.2.6:Cargo.toml line 16; .goreleaser.yaml builds with plain cargo build, no version injection; crates/werust-core/src/lib.rs version() = env!(CARGO_PKG_VERSION))
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q4 fields: id=q4 kind=stuck -->

**Your answer** (write below this line):

## Q5

**'task:general-browser-menu-with-version-and-debug-entry' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The menu's version line will read 'werust 0.0.0' in every shipped build, so the feature the task exists for (show the werust VERSION) does not actually show it. The workspace Cargo version is 0.0.0 and nothing bumps it at release: git show v0.2.4:Cargo.toml and v0.2.6:Cargo.toml both carry version = 0.0.0, and the GoReleaser rust builder derives the RELEASE name from the tag without rewriting Cargo.toml or injecting a version. So on v0.2.6 all three menus (and the startup banner) say 0.0.0. Either plumb the real version (release-time bump or a build-time version from the tag) or record this explicitly and file the follow-up task; note DECISIONS.md Decision 2 says the Gradle versionName is a hand-maintained 0.0.0 that does NOT track the workspace version, which implies the workspace version is meaningful, and it is not. (Cargo.toml:16 version = 0.0.0; git show v0.2.6:Cargo.toml line 16; .goreleaser.yaml builds with plain cargo build, no version injection; crates/werust-core/src/lib.rs version() = env!(CARGO_PKG_VERSION))
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q5 fields: id=q5 kind=stuck -->

**Your answer** (write below this line):
