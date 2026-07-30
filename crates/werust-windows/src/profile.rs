//! WHERE the shell keeps its browser PROFILE, and why that is not the engine's
//! default.
//!
//! A pure rule with a pure test, so the one thing this crate decides about
//! per-user state is checked on the Ubuntu `verify` gate rather than discovered
//! on a Windows box months later.
//!
//! # The rule
//!
//! `crates/windows-renderer` defaults its WebView2 user-data folder to
//! `%TEMP%\werust-webview2`, which is right for an ENGINE that only ever runs in
//! a CI smoke and WRONG for a browser: a temp profile loses cookies, storage and
//! cache whenever the OS cleans temp (and on many machines, on reboot). That is a
//! user-visible bug that would be found late and diagnosed slowly, so this crate
//! passes its own DURABLE path instead of inheriting the development default
//! (the acceptance criterion planted on this task at Gate 3 of the engine task).
//!
//! [`user_data_folder_in`] is that rule: `%LOCALAPPDATA%\werust\WebView2`.
//!
//! * `%LOCALAPPDATA%` because a WebView2 profile is MACHINE-LOCAL cache and
//!   state, not roaming user documents \u2014 it is exactly what Microsoft's own
//!   guidance reserves `LocalAppData` for, and putting a multi-hundred-megabyte
//!   browser cache on a roaming profile is a known way to make a domain login
//!   crawl.
//! * `werust\WebView2` (a per-app directory with the engine named INSIDE it)
//!   because werust may one day keep other Windows per-user state \u2014 the
//!   retrieval setting, a profile, a cache \u2014 and a sibling directory is where it
//!   goes. The engine name is a subdirectory rather than a suffix so a future
//!   native-renderer profile sits beside it instead of colliding with it.
//!
//! # This is NOT a third convention
//!
//! werust's existing per-user state (the retrieval-backend setting) resolves
//! through `werust_core::retrieval::settings_dir`, which today knows
//! `$XDG_CONFIG_HOME/werust` and `$HOME/.config/werust` \u2014 the desktop/Linux
//! day-one target \u2014 and has no Windows branch at all, so on Windows it resolves
//! to `None` and the setting simply does not persist. Teaching the CORE about
//! `%LOCALAPPDATA%` is a change to the settings concept and belongs to the task
//! that owns it, not to this window (recorded:
//! `work/notes/observations/settings-dir-has-no-windows-branch-2026-07-30.md`).
//! This rule therefore names the same VENDOR directory (`werust`) that a future
//! core Windows branch would name, so the two converge rather than collide.

use std::ffi::OsString;
use std::path::PathBuf;

/// The environment variable naming the per-user, machine-LOCAL app-data root on
/// Windows.
pub const LOCAL_APP_DATA_ENV: &str = "LOCALAPPDATA";

/// The vendor directory werust keeps its per-user Windows state under.
pub const APP_DIR: &str = "werust";

/// The engine's own subdirectory inside it: this is a WebView2 profile, and a
/// future backend's profile is a sibling rather than a collision.
pub const WEBVIEW2_PROFILE_DIR: &str = "WebView2";

/// The DURABLE WebView2 user-data folder for a given `%LOCALAPPDATA%` reading.
///
/// [`None`] (an unset or empty variable, which on a real Windows session does not
/// happen) falls back to the engine's own temp default rather than to a guess:
/// the browser still runs, and the honest consequence \u2014 a profile that does not
/// survive \u2014 is the same one the engine already documents, instead of werust
/// inventing a location no Windows tool knows about.
#[must_use]
pub fn user_data_folder_in(local_app_data: Option<OsString>) -> Option<PathBuf> {
    let root = local_app_data?;
    if root.is_empty() {
        return None;
    }
    Some(PathBuf::from(root).join(APP_DIR).join(WEBVIEW2_PROFILE_DIR))
}

/// The DURABLE WebView2 user-data folder on THIS machine, or [`None`] when
/// `%LOCALAPPDATA%` is unreadable.
///
/// The shell passes this to
/// `Webview2Renderer::with_user_data_folder`; see the module docs for why it must
/// not inherit `Webview2Renderer::new`'s `%TEMP%` default.
#[must_use]
pub fn user_data_folder() -> Option<PathBuf> {
    user_data_folder_in(std::env::var_os(LOCAL_APP_DATA_ENV))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_is_durable_local_app_data_never_the_engines_temp_default() {
        // THE acceptance criterion this module exists for: a shipping browser
        // must not inherit the engine's development `%TEMP%` profile, because a
        // temp profile loses cookies, storage and cache.
        let folder = user_data_folder_in(Some(OsString::from(r"C:\Users\ada\AppData\Local")))
            .expect("a readable %LOCALAPPDATA% yields a durable folder");
        assert_eq!(
            folder,
            PathBuf::from(r"C:\Users\ada\AppData\Local")
                .join("werust")
                .join("WebView2")
        );
        // It is under LocalAppData (machine-local state), and NOT under temp.
        let shown = folder.to_string_lossy().to_lowercase();
        assert!(shown.contains("appdata"), "{shown}");
        assert!(shown.contains("local"), "{shown}");
        assert!(
            !shown.contains("temp") && !shown.contains("tmp"),
            "a shipping profile must not live in temp: {shown}"
        );
        // And it is a werust-owned directory, not a bare name dropped into
        // LocalAppData's root.
        assert!(shown.contains("werust"), "{shown}");
    }

    #[test]
    fn an_unreadable_local_app_data_falls_back_to_the_engines_default_not_a_guess() {
        // Rather than mint a path no Windows tool knows (`C:\werust`, the exe's
        // directory, the drive root), an unreadable variable declines and the
        // engine's documented default applies. The consequence is stated in the
        // spike README rather than hidden.
        assert_eq!(user_data_folder_in(None), None);
        assert_eq!(user_data_folder_in(Some(OsString::new())), None);
    }

    #[test]
    fn the_folder_is_the_same_one_the_rule_computes_for_this_process() {
        // The env-reading wrapper adds no second rule: it is the pure rule
        // applied to `%LOCALAPPDATA%`.
        assert_eq!(
            user_data_folder(),
            user_data_folder_in(std::env::var_os(LOCAL_APP_DATA_ENV))
        );
    }
}
