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
        bold_italic: include_bytes!("../../../assets/fonts/Open_Sans/static/OpenSans-BoldItalic.ttf"),
    },
    FontFamily {
        name: "Montserrat",
        regular: include_bytes!("../../../assets/fonts/Montserrat/static/Montserrat-Regular.ttf"),
        bold: include_bytes!("../../../assets/fonts/Montserrat/static/Montserrat-Bold.ttf"),
        italic: include_bytes!("../../../assets/fonts/Montserrat/static/Montserrat-Italic.ttf"),
        bold_italic: include_bytes!("../../../assets/fonts/Montserrat/static/Montserrat-BoldItalic.ttf"),
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
        regular: include_bytes!("../../../assets/fonts/Playfair_Display/static/PlayfairDisplay-Regular.ttf"),
        bold: include_bytes!("../../../assets/fonts/Playfair_Display/static/PlayfairDisplay-Bold.ttf"),
        italic: include_bytes!("../../../assets/fonts/Playfair_Display/static/PlayfairDisplay-Italic.ttf"),
        bold_italic: include_bytes!("../../../assets/fonts/Playfair_Display/static/PlayfairDisplay-BoldItalic.ttf"),
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
        .select_best_match(&[FamilyName::Title(family_name.to_string())], &Properties::new())
        .ok()?;
    match handle {
        Handle::Path { path, .. } => Some(path.display().to_string()),
        Handle::Memory { .. } => None,
    }
}
