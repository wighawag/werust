//! The **browser menu**: werust's primary ⋮ menu, the one every OS edge renders
//! natively.
//!
//! This is the general browser menu other browsers put behind a ⋮ / hamburger
//! button (task `general-browser-menu-with-version-and-debug-entry`, spec
//! `in-app-debug-menu-console-and-network`). It is a USER-FACING, always-available
//! surface (NOT debug-build-gated), and it is deliberately a GENERAL container
//! meant to GROW into the usual browser items (bookmarks, settings, history, …).
//! Its Phase-1 contents are only two entries: werust's VERSION and a DEBUG entry
//! that opens the in-app debug view.
//!
//! # The menu MODEL lives here, the menu SURFACE lives at each edge
//!
//! Exactly like [`ChromeState`](crate::ChromeState) and the
//! [`debug`](crate::debug) capture store: the toolkit-free core owns the ordered
//! list of ITEMS ([`BrowserMenu`]), and each OS edge renders that SAME list in its
//! own native menu widget (desktop a GTK `MenuButton` + popover, Android a
//! `PopupMenu`, iOS a `UIMenu`). Nothing here knows about GTK, UIKit, or the
//! Android view system, so the whole menu model is unit-testable inside the
//! pure-Rust `verify` gate with no display and no SDK.
//!
//! Adding a future item is therefore ONE edit here: a new [`MenuItem`] in
//! [`BrowserMenu::new`] appears in all three menus at once, and each edge only
//! needs a branch for its [`id`](MenuItem::id) if it is an
//! [`Action`](MenuItemKind::Action). That is the "structured to grow" property
//! the task asks for, expressed as code rather than as a comment.
//!
//! # One version, three menus
//!
//! The version shown is [`crate::version`] — the SINGLE source (`WERUST_VERSION`,
//! resolved once at build time by `werust-core`'s `build.rs` from the release
//! tag, else `git describe`, else the Cargo version), read here and handed to the
//! mobile edges over the FFI (`werust_ios_version` / `nativeVersion`, plus the
//! whole menu as [`menu_json`]) so no edge hardcodes a version string of its own
//! and all three menus can never disagree.
//!
//! # The Debug entry is a HOOK
//!
//! The [`MENU_ITEM_DEBUG`] item names the intent ("open the debug view"); the
//! debug VIEW itself is the follow-on tasks `debug-view-console-network-tabs-desktop`
//! / `-mobile`. Each edge routes the item to its own `open_debug_view` /
//! `openDebugView` hook, which today shows a short "coming" placeholder and is
//! the ONE function those tasks replace. The recorded rationale (and the other
//! judgement calls this module bakes in) is in
//! `docs/spikes/general-browser-menu-with-version-and-debug-entry/DECISIONS.md`.

use serde_json::{json, Value};

/// The stable id of the VERSION entry: a non-interactive line reading
/// `werust <version>`.
///
/// Ids are the wire vocabulary the edges branch on (like the trust-posture and
/// load-step wire names), so they are lower-kebab and stable across platforms.
pub const MENU_ITEM_VERSION: &str = "version";

/// The stable id of the DEBUG entry: the item that opens the in-app debug view.
pub const MENU_ITEM_DEBUG: &str = "debug";

/// What an edge should DO with a [`MenuItem`] when it renders it.
///
/// Deliberately a small closed vocabulary rather than a per-item flag soup: an
/// edge maps [`Info`](MenuItemKind::Info) onto its platform's non-interactive /
/// disabled menu entry and [`Action`](MenuItemKind::Action) onto a tappable one
/// that dispatches on the item's [`id`](MenuItem::id). A future submenu/toggle
/// item adds a variant here, so every edge is FORCED to consider it (a `match`
/// goes non-exhaustive) instead of silently rendering it wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItemKind {
    /// A non-interactive line (the version). Shown, never activatable.
    Info,
    /// An activatable entry; activating it dispatches on the item's id.
    Action,
}

impl MenuItemKind {
    /// The stable, lower-case wire name for [`menu_json`], so the mobile edges
    /// decide interactivity from the SAME fact desktop does.
    #[must_use]
    pub fn wire_name(self) -> &'static str {
        match self {
            MenuItemKind::Info => "info",
            MenuItemKind::Action => "action",
        }
    }
}

/// One entry of the browser menu: a stable id, the label to show, and what the
/// edge should do with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    /// The stable, cross-platform id an edge branches on (never the label, which
    /// is display text and may be translated/reworded).
    pub id: String,
    /// The text to show in the native menu.
    pub label: String,
    /// Whether the edge renders this as a non-interactive line or an activatable
    /// entry.
    pub kind: MenuItemKind,
}

impl MenuItem {
    /// An entry with `id`, `label`, and `kind`.
    #[must_use]
    pub fn new(id: impl Into<String>, label: impl Into<String>, kind: MenuItemKind) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind,
        }
    }
}

/// werust's primary browser menu: the ordered list of items every edge renders.
///
/// A GENERAL container, not a debug menu: [`new`](BrowserMenu::new) is where the
/// browser's menu grows, and the two Phase-1 entries are simply its first two
/// items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserMenu {
    items: Vec<MenuItem>,
}

impl BrowserMenu {
    /// The browser menu as shipped: the werust VERSION line, then the DEBUG entry.
    ///
    /// THIS is the growth point. A future bookmarks/settings/history entry is one
    /// more [`MenuItem`] pushed here (with its own `MENU_ITEM_*` id), and it then
    /// appears in the desktop popover, the Android `PopupMenu`, and the iOS
    /// `UIMenu` without touching their layout code — only an edge branch for its
    /// id if it is an [`Action`](MenuItemKind::Action).
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: vec![
                MenuItem::new(
                    MENU_ITEM_VERSION,
                    format!("werust {}", crate::version()),
                    MenuItemKind::Info,
                ),
                MenuItem::new(MENU_ITEM_DEBUG, "Debug", MenuItemKind::Action),
            ],
        }
    }

    /// The items, in the order the edge should render them.
    #[must_use]
    pub fn items(&self) -> &[MenuItem] {
        &self.items
    }

    /// The item with `id`, or [`None`] if the menu has no such entry.
    #[must_use]
    pub fn item(&self, id: &str) -> Option<&MenuItem> {
        self.items.iter().find(|item| item.id == id)
    }
}

impl Default for BrowserMenu {
    fn default() -> Self {
        Self::new()
    }
}

/// The menu as the JSON document the mobile edges build their native menu from:
/// `{"version":"…","items":[{"id":…,"label":…,"kind":"info"|"action"}]}`.
///
/// A DEDICATED accessor beside the chrome JSON and the debug JSON (the same shape
/// decision the [`debug`](crate::debug) module records): the chrome is re-encoded
/// on every chrome refresh, while the menu is read once when the menu is built.
/// `version` is carried alongside the items so an edge that wants only the bare
/// version string (an about box, a crash report) does not have to parse a label.
#[must_use]
pub fn menu_json(menu: &BrowserMenu) -> String {
    let items: Vec<Value> = menu
        .items()
        .iter()
        .map(|item| {
            json!({
                "id": item.id,
                "label": item.label,
                "kind": item.kind.wire_name(),
            })
        })
        .collect();
    json!({
        "version": crate::version(),
        "items": items,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_shows_the_werust_version_from_the_one_shared_source() {
        // Acceptance: the menu shows the werust VERSION, sourced from ONE place
        // (`crate::version`, the build-time-resolved `WERUST_VERSION`) so the
        // desktop, Android and iOS menus can never disagree — no edge builds its
        // own version string.
        let menu = BrowserMenu::new();
        let version_item = menu
            .item(MENU_ITEM_VERSION)
            .expect("the menu has a version entry");
        assert_eq!(version_item.label, format!("werust {}", crate::version()));
        assert!(
            version_item.label.contains(crate::version()),
            "the label carries the real resolved version: {}",
            version_item.label
        );
        // The version is a LINE, not something to activate.
        assert_eq!(version_item.kind, MenuItemKind::Info);
    }

    #[test]
    fn the_menu_has_a_debug_entry_that_is_activatable() {
        // Acceptance: the menu has a Debug entry that opens the debug view. The
        // core owns the ENTRY (its stable id + label + that it is activatable);
        // each edge routes the id to its own open-debug-view hook.
        let menu = BrowserMenu::new();
        let debug = menu
            .item(MENU_ITEM_DEBUG)
            .expect("the menu has a debug entry");
        assert_eq!(debug.label, "Debug");
        assert_eq!(
            debug.kind,
            MenuItemKind::Action,
            "the Debug entry must be activatable — it opens the debug view"
        );
    }

    #[test]
    fn the_menu_is_a_general_container_not_a_debug_only_menu() {
        // Acceptance: the menu is a GENERAL browser menu structured to GROW, not a
        // debug-only menu. Expressed as: it is an ORDERED LIST of items with
        // unique ids (so a future bookmarks/settings entry is one push, and every
        // edge renders it by iterating), the version comes FIRST, and nothing in
        // the model is debug-specific — the Debug entry is just one item.
        let menu = BrowserMenu::new();
        assert_eq!(
            menu.items()[0].id,
            MENU_ITEM_VERSION,
            "the version line heads the menu"
        );

        let mut ids: Vec<&str> = menu.items().iter().map(|i| i.id.as_str()).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "menu item ids are unique: {ids:?}");

        // The growth property, exercised rather than asserted about: an edge
        // renders whatever items the core lists, so a NEW item is visible to the
        // same iteration with no new model concept.
        let grown = BrowserMenu {
            items: menu
                .items()
                .iter()
                .cloned()
                .chain(std::iter::once(MenuItem::new(
                    "settings",
                    "Settings",
                    MenuItemKind::Action,
                )))
                .collect(),
        };
        assert_eq!(grown.items().len(), count + 1);
        assert_eq!(
            grown.item("settings").map(|i| i.label.as_str()),
            Some("Settings")
        );
        // …and the shipped entries are untouched by the growth.
        assert!(grown.item(MENU_ITEM_VERSION).is_some());
        assert!(grown.item(MENU_ITEM_DEBUG).is_some());
    }

    #[test]
    fn an_unknown_item_id_is_absent_rather_than_guessed() {
        assert!(BrowserMenu::new().item("bookmarks").is_none());
    }

    #[test]
    fn menu_json_carries_the_version_and_every_item_for_the_mobile_edges() {
        // The mobile edges build their NATIVE menu from this ONE document, so it
        // must carry the version and every item's id/label/kind — the same
        // "one shared fact, per-platform rendering" shape the chrome JSON and the
        // debug JSON use.
        let json = menu_json(&BrowserMenu::new());
        let parsed: Value = serde_json::from_str(&json).expect("valid JSON");

        assert_eq!(parsed["version"], crate::version());

        let items = parsed["items"].as_array().expect("an items array");
        assert_eq!(items.len(), BrowserMenu::new().items().len());
        assert_eq!(items[0]["id"], MENU_ITEM_VERSION);
        assert_eq!(items[0]["label"], format!("werust {}", crate::version()));
        assert_eq!(
            items[0]["kind"], "info",
            "the version line is non-interactive on every platform"
        );

        let debug = items
            .iter()
            .find(|i| i["id"] == MENU_ITEM_DEBUG)
            .expect("the debug entry is on the wire");
        assert_eq!(debug["label"], "Debug");
        assert_eq!(
            debug["kind"], "action",
            "the Debug entry is activatable on every platform"
        );
    }

    #[test]
    fn the_version_is_resolved_at_build_time_and_is_never_empty_or_a_placeholder() {
        // The ONE version source. If this were empty the menus would silently show
        // "werust " on all three platforms — and if it were the un-injected
        // `0.0.0` placeholder they would all confidently show a LIE, which is the
        // exact defect `build.rs` exists to prevent (nothing used to inject a
        // version into the Rust build, so a tagged v0.2.6 release shipped menus
        // reading "werust 0.0.0").
        let version = crate::version();
        assert!(!version.is_empty());
        assert_ne!(
            version, "0.0.0",
            "the version must be resolved (injected / git-described / the real Cargo version), \
             never the 0.0.0 placeholder"
        );
        // It is the build-time-resolved value, not a second `env!` of the Cargo
        // metadata: a dev checkout resolves `git describe`, which carries the
        // commit distance the bare Cargo version cannot.
        assert_eq!(version, env!("WERUST_VERSION"));
    }
}
