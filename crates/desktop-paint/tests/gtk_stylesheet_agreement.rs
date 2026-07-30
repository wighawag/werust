//! The third desktop's transcription guard: the shared palette and the GTK
//! stylesheet must not disagree about a colour.
//!
//! WHY THIS EXISTS. The core exports stable class NAMES and has no notion of
//! colour; the stylesheet belongs to the edge (`docs/adr/0011`'s layering). That
//! leaves werust with TWO stylesheets for one vocabulary: the GTK edge's
//! `APP_CSS`, and [`desktop_paint::CLASS_COLORS`] for the native-widget edges
//! (AppKit and Win32 have no stylesheet at all, so their palette has to be code).
//! Until this task there were nearly THREE: the Win32 window would have carried
//! its own copy of the table.
//!
//! Extracting the carrier removed the third copy; this test removes the drift
//! risk between the remaining two. "A content-verified badge is the same green on
//! every desktop" was a promise kept by careful transcription — the same kind of
//! promise that let the Kotlin and Swift chrome twins drift. Now it is checked.
//!
//! It is deliberately ONE-DIRECTIONAL: every class the GTK stylesheet gives a
//! colour must have the SAME colour here. A class GTK leaves at its default
//! (`.debug-console-log` inherits the theme's text colour, which a GTK edge can
//! do and a Win32 edge cannot) is not a disagreement, so it is skipped and
//! counted — and the count is asserted, so the check can never go vacuous.

use std::path::Path;

use desktop_paint::{class_color, Rgb, CLASS_COLORS};

/// The GTK edge's stylesheet, read from its source (there is no other way to get
/// at a `const &str` in another crate's binary).
fn app_css() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../werust/src/main.rs");
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let start = source
        .find("const APP_CSS: &str")
        .expect("the GTK edge must still declare its stylesheet");
    let end = source[start..]
        .find("\n\n")
        .map_or(source.len(), |offset| start + offset);
    source[start..end].to_string()
}

/// The value of `property` inside `.class { ... }`, as a `0xRRGGBB` number.
fn declared(css: &str, class: &str, property: &str) -> Option<u32> {
    let rule_start = css.find(&format!(".{class} {{"))?;
    let rule_end = css[rule_start..].find('}')? + rule_start;
    let rule = &css[rule_start..rule_end];
    // `background-color` contains `color`, so a plain search would match the
    // wrong property; anchor on the property preceded by a space or a brace.
    let at = rule
        .match_indices(property)
        .find(|(index, _)| *index > 0 && matches!(rule.as_bytes()[index - 1], b' ' | b'{' | b';'))
        .map(|(index, _)| index)?;
    let value = &rule[at + property.len()..];
    let hash = value.find('#')?;
    let hex: String = value[hash + 1..]
        .chars()
        .take_while(char::is_ascii_hexdigit)
        .collect();
    (hex.len() == 6).then(|| u32::from_str_radix(&hex, 16).ok())?
}

/// The `0xRRGGBB` a palette colour was written as.
fn hex_of(rgb: Rgb) -> u32 {
    let channel = |v: f64| ((v * 255.0).round() as u32) & 0xff;
    (channel(rgb.red) << 16) | (channel(rgb.green) << 8) | channel(rgb.blue)
}

#[test]
fn the_gtk_stylesheet_and_the_shared_palette_agree() {
    let css = app_css();
    let mut compared = 0;
    let mut skipped = Vec::new();
    for (class, rgb) in CLASS_COLORS {
        // The error banner's palette entry is its FILL (its text is always
        // white); every other class's is its TEXT colour. That is the same split
        // the GTK rules make.
        let property = if class.starts_with("error-banner") {
            "background-color:"
        } else {
            "color:"
        };
        match declared(&css, class, property) {
            Some(gtk) => {
                assert_eq!(
                    gtk,
                    hex_of(*rgb),
                    "`{class}` is #{gtk:06x} in the GTK stylesheet but #{:06x} in the shared \
                     palette: the same werust state would read differently on two desktops",
                    hex_of(*rgb)
                );
                compared += 1;
            }
            None => skipped.push(*class),
        }
    }
    assert!(
        compared >= CLASS_COLORS.len() - 2,
        "too many classes went unchecked ({skipped:?}); this guard is only worth having while \
         nearly every shared colour is really compared against the GTK stylesheet"
    );
    // The one legitimate skip today, named so a NEW one has to be justified: GTK
    // can leave a class at the theme's default text colour, which a Win32 or
    // AppKit edge (which paints its own text) cannot.
    assert_eq!(skipped, vec!["debug-console-log"], "unexpected skips");
}

#[test]
fn the_invalid_entry_and_progress_colours_are_the_gtk_ones_too() {
    // Neither is an exported CLASS (they are the URL bar's own two states), so
    // they are not in `CLASS_COLORS` — but they are transcribed from the same GTK
    // rules and drift exactly as easily.
    let css = app_css();
    assert_eq!(
        declared(&css, "url-invalid", "color:"),
        Some(hex_of(desktop_paint::INVALID_ENTRY_COLOR)),
        "the invalid-entry colour must be the GTK edge's `.url-invalid` red"
    );
    // The progress fill is a GTK selector rather than a class (`entry > progress`).
    let progress = css
        .find("entry > progress")
        .map(|start| &css[start..])
        .and_then(|rule| rule.find('#').map(|hash| &rule[hash + 1..hash + 7]))
        .and_then(|hex| u32::from_str_radix(hex, 16).ok());
    assert_eq!(
        progress,
        Some(hex_of(desktop_paint::LOAD_PROGRESS_COLOR)),
        "the URL bar's progress fill must be the GTK edge's blue"
    );
    // And the shared palette really does answer for a class the core exports, so
    // this file's other assertions are about a live table.
    assert!(class_color("trust-verified").is_some());
}
