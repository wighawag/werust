---
title: "`settings_dir()` has no Windows branch, so the retrieval-backend setting cannot persist on Windows"
date: 2026-07-30
status: open
---

Noticed while building `windows-win32-window-and-chrome` (which needed a per-user Windows path for the WebView2 profile). `werust_core::retrieval::settings_dir()` resolves `$WERUST_SETTINGS_DIR`, then `$XDG_CONFIG_HOME/werust`, then `$HOME/.config/werust`, then `None`. A normal Windows session sets `%USERPROFILE%` and `%LOCALAPPDATA%` but not `HOME`, so on Windows it returns `None` and `RetrievalSettings::save` has nowhere to write: the user's chosen retrieval backend takes effect for the session and is silently forgotten at exit.

Not fixed here (out of this task's scope, and it is the settings concept's own decision where Windows state lives). The window's own durable WebView2 profile deliberately names the same vendor directory a core Windows branch would want — `%LOCALAPPDATA%\werust\...` — so the two converge rather than collide; see `crates/werust-windows/src/profile.rs` and `docs/spikes/windows-win32-window-and-chrome/DECISIONS.md` §8.
