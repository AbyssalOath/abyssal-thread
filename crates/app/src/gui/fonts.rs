//! Bundled font families, embedded directly into the compiled binary via
//! `include_bytes!` - so picking a font is a dropdown, not a file hunt,
//! and distribution stays a single .exe with no separate fonts/ folder
//! that could get lost or left behind.
//!
//! Fonts come from https://fonts.google.com - every family there is
//! licensed under the SIL Open Font License 1.1 or Apache 2.0, both of
//! which explicitly permit bundling into and redistributing with software
//! (including commercially) with no attribution requirement.
//!
//! To add a family: on the family's Google Fonts page, use "Get font" ->
//! "Download all", pull the four Regular/Bold/Italic/BoldItalic .ttf files
//! out of the "static" folder in the zip (variable-font families ship
//! fixed-weight TTFs there), drop them into crates/app/assets/fonts/ named
//! to match an entry below, then add a `FontFamily` entry.

pub struct FontFamily {
    pub name: &'static str,
    pub regular: &'static [u8],
    pub bold: &'static [u8],
    pub italic: &'static [u8],
    pub bold_italic: &'static [u8],
}

impl FontFamily {
    pub fn bytes_for(&self, bold: bool, italic: bool) -> &'static [u8] {
        match (bold, italic) {
            (true, true) => self.bold_italic,
            (true, false) => self.bold,
            (false, true) => self.italic,
            (false, false) => self.regular,
        }
    }
}

pub static FONT_FAMILIES: &[FontFamily] = &[
    FontFamily {
        name: "Open Sans",
        regular: include_bytes!("../../../assets/fonts/Open_Sans/static/OpenSans-Regular.ttf"),
        bold: include_bytes!("../../../assets/fonts/Open_Sans/static/OpenSans-Bold.ttf"),
        italic: include_bytes!("../../../assets/fonts/Open_Sans/static/OpenSans-Italic.ttf"),
        bold_italic: include_bytes!(
            "../../../assets/fonts/Open_Sans/static/OpenSans-BoldItalic.ttf"
        ),
    },
    FontFamily {
        name: "Montserrat",
        regular: include_bytes!("../../../assets/fonts/Montserrat/static/Montserrat-Regular.ttf"),
        bold: include_bytes!("../../../assets/fonts/Montserrat/static/Montserrat-Bold.ttf"),
        italic: include_bytes!("../../../assets/fonts/Montserrat/static/Montserrat-Italic.ttf"),
        bold_italic: include_bytes!(
            "../../../assets/fonts/Montserrat/static/Montserrat-BoldItalic.ttf"
        ),
    },
    FontFamily {
        name: "PT Sans",
        regular: include_bytes!("../../../assets/fonts/PT_Sans/PTSans-Regular.ttf"),
        bold: include_bytes!("../../../assets/fonts/PT_Sans/PTSans-Bold.ttf"),
        italic: include_bytes!("../../../assets/fonts/PT_Sans/PTSans-Italic.ttf"),
        bold_italic: include_bytes!("../../../assets/fonts/PT_Sans/PTSans-BoldItalic.ttf"),
    },
    FontFamily {
        name: "Playfair Display",
        regular: include_bytes!(
            "../../../assets/fonts/Playfair_Display/static/PlayfairDisplay-Regular.ttf"
        ),
        bold: include_bytes!(
            "../../../assets/fonts/Playfair_Display/static/PlayfairDisplay-Bold.ttf"
        ),
        italic: include_bytes!(
            "../../../assets/fonts/Playfair_Display/static/PlayfairDisplay-Italic.ttf"
        ),
        bold_italic: include_bytes!(
            "../../../assets/fonts/Playfair_Display/static/PlayfairDisplay-BoldItalic.ttf"
        ),
    },
    FontFamily {
        name: "Roboto",
        regular: include_bytes!("../../../assets/fonts/Roboto/static/Roboto-Regular.ttf"),
        bold: include_bytes!("../../../assets/fonts/Roboto/static/Roboto-Bold.ttf"),
        italic: include_bytes!("../../../assets/fonts/Roboto/static/Roboto-Italic.ttf"),
        bold_italic: include_bytes!("../../../assets/fonts/Roboto/static/Roboto-BoldItalic.ttf"),
    },
];

/// Every family name font-kit can see installed on this system. Empty (not
/// an error) if enumeration fails for any reason - the system-font
/// dropdown just has nothing to show in that case, rather than the app
/// failing to start.
pub fn enumerate_system_font_families() -> Vec<String> {
    use font_kit::source::SystemSource;
    let mut names = SystemSource::new().all_families().unwrap_or_default();
    names.sort();
    names.dedup();
    names
}

/// Resolves a system family name to a loadable font file path, if font-kit
/// can find one on disk for it (some system fonts are memory-only/embedded
/// and have no path - `None` in that case).
pub fn resolve_system_font_path(family_name: &str) -> Option<String> {
    use font_kit::family_name::FamilyName;
    use font_kit::handle::Handle;
    use font_kit::properties::Properties;
    use font_kit::source::SystemSource;

    let handle = SystemSource::new()
        .select_best_match(
            &[FamilyName::Title(family_name.to_string())],
            &Properties::new(),
        )
        .ok()?;
    match handle {
        Handle::Path { path, .. } => Some(path.display().to_string()),
        Handle::Memory { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Bundled FontFamily data: fully deterministic, since these bytes
    // are baked into the binary at compile time via include_bytes! - no
    // reason for these to depend on what's installed on the machine
    // running the tests, unlike the font-kit-backed tests further down.

    #[test]
    fn every_bundled_family_has_all_four_nonempty_variants() {
        for family in FONT_FAMILIES {
            assert!(
                !family.regular.is_empty(),
                "{}: regular is empty",
                family.name
            );
            assert!(!family.bold.is_empty(), "{}: bold is empty", family.name);
            assert!(
                !family.italic.is_empty(),
                "{}: italic is empty",
                family.name
            );
            assert!(
                !family.bold_italic.is_empty(),
                "{}: bold_italic is empty",
                family.name
            );
        }
    }

    #[test]
    fn bytes_for_selects_the_right_variant() {
        // Only need one family for this - it's testing the (bool, bool)
        // match arms, not per-family content.
        let family = &FONT_FAMILIES[0];
        assert_eq!(
            family.bytes_for(false, false).as_ptr(),
            family.regular.as_ptr()
        );
        assert_eq!(family.bytes_for(true, false).as_ptr(), family.bold.as_ptr());
        assert_eq!(
            family.bytes_for(false, true).as_ptr(),
            family.italic.as_ptr()
        );
        assert_eq!(
            family.bytes_for(true, true).as_ptr(),
            family.bold_italic.as_ptr()
        );
    }

    #[test]
    fn bundled_font_bytes_start_with_a_valid_ttf_or_otf_header() {
        // 0x00010000 = TrueType, "OTTO" = OpenType/CFF, "true"/"typ1" cover
        // the rarer Mac-flavored variants - catches an accidentally
        // corrupted/truncated/wrong file at the include_bytes! path long
        // before it'd surface as a confusing "couldn't parse bundled font"
        // status message at runtime.
        const VALID_HEADERS: [[u8; 4]; 4] =
            [[0x00, 0x01, 0x00, 0x00], *b"OTTO", *b"true", *b"typ1"];
        for family in FONT_FAMILIES {
            for (variant_name, bytes) in [
                ("regular", family.regular),
                ("bold", family.bold),
                ("italic", family.italic),
                ("bold_italic", family.bold_italic),
            ] {
                let header = &bytes[..4.min(bytes.len())];
                assert!(
                    VALID_HEADERS.iter().any(|h| h == header),
                    "{} {variant_name}: doesn't start with a recognized font file header",
                    family.name,
                );
            }
        }
    }

    #[test]
    fn family_names_are_unique() {
        let mut names: Vec<&str> = FONT_FAMILIES.iter().map(|f| f.name).collect();
        let original_len = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            original_len,
            "duplicate family name in FONT_FAMILIES"
        );
    }

    // --- font-kit-backed system enumeration: deliberately tolerant of
    // "this CI runner has zero/unusual fonts installed" rather than
    // asserting specific counts or names, which would make these tests
    // flaky across the different OS runners release.yml actually builds on.

    #[test]
    fn enumerate_system_font_families_does_not_panic_and_is_sorted_deduped() {
        let names = enumerate_system_font_families();
        let mut sorted_deduped = names.clone();
        sorted_deduped.sort();
        sorted_deduped.dedup();
        assert_eq!(
            names, sorted_deduped,
            "enumerate_system_font_families should already be sorted+deduped"
        );
    }

    #[test]
    fn resolve_system_font_path_returns_none_for_a_family_that_cannot_exist() {
        // The one part of this that's safe to assert unconditionally on
        // any machine: a family name specific enough that no real system
        // font could ever collide with it should never resolve.
        assert_eq!(
            resolve_system_font_path("Definitely Not A Real Font Family 12345 XYZ"),
            None
        );
    }

    #[test]
    fn resolving_an_enumerated_family_does_not_panic() {
        // Best-effort positive-path exercise: if this system happens to
        // have any fonts at all, try resolving the first one and just
        // confirm it doesn't panic - Some(path) and None (a memory-only
        // font) are both legitimate outcomes, so nothing else is asserted.
        if let Some(first) = enumerate_system_font_families().into_iter().next() {
            let _ = resolve_system_font_path(&first);
        }
    }
}
