//! Color catalogs: DMC and Anchor embroidery floss (cross stitch; DMC
//! numbers are also how diamond-painting drills are sold), Diamond Dotz
//! drills, and Perler/Hama/Artkal fuse beads.
//!
//! `data/dmc.tsv` (code, hex, name) is derived from the MIT-licensed `dmc`
//! R package by Sharla Gelfand (github.com/sharlagelfand/dmc, its
//! `data-raw/floss.R` cleaning script applied to `floss_adrianj.csv`),
//! which in turn sourced it from github.com/adrianj/CrossStitchCreator.
//!
//! `data/anchor.tsv` (code, hex; no names in the source) is extracted from
//! Cstitch's `initializeAnchor()` (github.com/kleintom/Cstitch,
//! `colorLists.cpp`, Copyright 2010-2011 Tom Klein, GPL-3.0-or-later -
//! compatible with this project's AGPL-3.0).
//!
//! `data/{perler,perler_mini,hama,hama_mini,artkal_s,artkal_c,diamond_dotz}.tsv`
//! are converted from the MIT-licensed beadcolors project
//! (github.com/maxcleme/beadcolors, `gen/v1/*.csv`, Copyright 2020
//! maxcleme).
//!
//! Brand strings are single tokens (`Perler-Mini`, not `Perler Mini`)
//! because `.cgp` `THREAD` lines are whitespace-separated.
//!
//! RGB values for physical floss are approximations of a dyed thread
//! under typical lighting, not an official manufacturer specification -
//! treat a match as "the floss to buy," and double-check against a real
//! color card for anything critical.

use crate::color::{delta_e_2000, rgb_to_lab, Lab};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq)]
pub struct Thread {
    pub brand: &'static str,
    pub code: &'static str,
    /// Empty for catalogs whose source has no color names (Anchor).
    pub name: &'static str,
    pub rgb: [u8; 3],
    lab: Lab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Catalog {
    Dmc,
    Anchor,
    DiamondDotz,
    Perler,
    PerlerMini,
    Hama,
    HamaMini,
    ArtkalS,
    ArtkalC,
}

const DMC_TSV: &str = include_str!("../data/dmc.tsv");
const ANCHOR_TSV: &str = include_str!("../data/anchor.tsv");
const DIAMOND_DOTZ_TSV: &str = include_str!("../data/diamond_dotz.tsv");
const PERLER_TSV: &str = include_str!("../data/perler.tsv");
const PERLER_MINI_TSV: &str = include_str!("../data/perler_mini.tsv");
const HAMA_TSV: &str = include_str!("../data/hama.tsv");
const HAMA_MINI_TSV: &str = include_str!("../data/hama_mini.tsv");
const ARTKAL_S_TSV: &str = include_str!("../data/artkal_s.tsv");
const ARTKAL_C_TSV: &str = include_str!("../data/artkal_c.tsv");

fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

fn parse_catalog(brand: &'static str, tsv: &'static str) -> Vec<Thread> {
    tsv.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let code = parts.next()?;
            let rgb = parse_hex(parts.next()?)?;
            let name = parts.next().unwrap_or("");
            Some(Thread {
                brand,
                code,
                name,
                rgb,
                lab: rgb_to_lab(rgb),
            })
        })
        .collect()
}

impl Catalog {
    pub const ALL: [Catalog; 9] = [
        Catalog::Dmc,
        Catalog::Anchor,
        Catalog::DiamondDotz,
        Catalog::Perler,
        Catalog::PerlerMini,
        Catalog::Hama,
        Catalog::HamaMini,
        Catalog::ArtkalS,
        Catalog::ArtkalC,
    ];

    /// Single-token brand as stored on `Floss::brand`.
    pub fn brand(self) -> &'static str {
        match self {
            Catalog::Dmc => "DMC",
            Catalog::Anchor => "Anchor",
            Catalog::DiamondDotz => "DiamondDotz",
            Catalog::Perler => "Perler",
            Catalog::PerlerMini => "Perler-Mini",
            Catalog::Hama => "Hama",
            Catalog::HamaMini => "Hama-Mini",
            Catalog::ArtkalS => "Artkal-S",
            Catalog::ArtkalC => "Artkal-C",
        }
    }

    /// Human-readable name for pickers.
    pub fn display_name(self) -> &'static str {
        match self {
            Catalog::Dmc => "DMC",
            Catalog::Anchor => "Anchor",
            Catalog::DiamondDotz => "Diamond Dotz",
            Catalog::Perler => "Perler (5 mm)",
            Catalog::PerlerMini => "Perler Mini",
            Catalog::Hama => "Hama Midi (5 mm)",
            Catalog::HamaMini => "Hama Mini",
            Catalog::ArtkalS => "Artkal S (5 mm)",
            Catalog::ArtkalC => "Artkal C (mini)",
        }
    }

    /// The catalog whose brand matches `brand` (case-insensitive).
    pub fn for_brand(brand: &str) -> Option<Catalog> {
        Self::ALL
            .into_iter()
            .find(|c| c.brand().eq_ignore_ascii_case(brand.trim()))
    }

    /// Every thread in the catalog, parsed once on first use.
    pub fn threads(self) -> &'static [Thread] {
        static CATALOGS: [OnceLock<Vec<Thread>>; 9] = [const { OnceLock::new() }; 9];
        let tsv = match self {
            Catalog::Dmc => DMC_TSV,
            Catalog::Anchor => ANCHOR_TSV,
            Catalog::DiamondDotz => DIAMOND_DOTZ_TSV,
            Catalog::Perler => PERLER_TSV,
            Catalog::PerlerMini => PERLER_MINI_TSV,
            Catalog::Hama => HAMA_TSV,
            Catalog::HamaMini => HAMA_MINI_TSV,
            Catalog::ArtkalS => ARTKAL_S_TSV,
            Catalog::ArtkalC => ARTKAL_C_TSV,
        };
        CATALOGS[self as usize].get_or_init(|| parse_catalog(self.brand(), tsv))
    }

    /// The catalog thread that looks closest to `rgb` (CIEDE2000).
    pub fn nearest(self, rgb: [u8; 3]) -> &'static Thread {
        self.nearest_n(rgb, 1)[0]
    }

    /// The `n` closest catalog threads to `rgb`, nearest first.
    pub fn nearest_n(self, rgb: [u8; 3], n: usize) -> Vec<&'static Thread> {
        let target = rgb_to_lab(rgb);
        let mut ranked: Vec<(f32, &'static Thread)> = self
            .threads()
            .iter()
            .map(|t| (delta_e_2000(target, t.lab), t))
            .collect();
        ranked.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        ranked.into_iter().take(n.max(1)).map(|(_, t)| t).collect()
    }

    /// Looks up a thread by code, case-insensitively ("310", "b5200").
    pub fn find(self, code: &str) -> Option<&'static Thread> {
        let code = code.trim();
        self.threads()
            .iter()
            .find(|t| t.code.eq_ignore_ascii_case(code))
    }

    /// Exact color match - how an OXS blend (which only records a color)
    /// is mapped back to a real thread.
    pub fn find_by_rgb(self, rgb: [u8; 3]) -> Option<&'static Thread> {
        self.threads().iter().find(|t| t.rgb == rgb)
    }

    /// Case-insensitive substring search over code and name.
    pub fn search(self, query: &str) -> Vec<&'static Thread> {
        let q = query.trim().to_ascii_lowercase();
        if q.is_empty() {
            return self.threads().iter().collect();
        }
        self.threads()
            .iter()
            .filter(|t| {
                t.code.to_ascii_lowercase().contains(&q) || t.name.to_ascii_lowercase().contains(&q)
            })
            .collect()
    }
}

/// Looks up `"DMC 310"`, `"Anchor 403"`, or a bare code (tried against
/// `default` first).
pub fn find_thread(spec: &str, default: Catalog) -> Option<&'static Thread> {
    let spec = spec.trim();
    if let Some((brand, code)) = spec.split_once(|c: char| c.is_whitespace()) {
        if let Some(cat) = Catalog::for_brand(brand) {
            return cat.find(code);
        }
    }
    default.find(spec)
}

pub fn dmc() -> &'static [Thread] {
    Catalog::Dmc.threads()
}

pub fn nearest_dmc(rgb: [u8; 3]) -> &'static Thread {
    Catalog::Dmc.nearest(rgb)
}

pub fn find_dmc(code: &str) -> Option<&'static Thread> {
    Catalog::Dmc.find(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogs_parse_every_line_with_unique_codes() {
        for (cat, tsv) in [
            (Catalog::Dmc, DMC_TSV),
            (Catalog::Anchor, ANCHOR_TSV),
            (Catalog::DiamondDotz, DIAMOND_DOTZ_TSV),
            (Catalog::Perler, PERLER_TSV),
            (Catalog::PerlerMini, PERLER_MINI_TSV),
            (Catalog::Hama, HAMA_TSV),
            (Catalog::HamaMini, HAMA_MINI_TSV),
            (Catalog::ArtkalS, ARTKAL_S_TSV),
            (Catalog::ArtkalC, ARTKAL_C_TSV),
        ] {
            let threads = cat.threads();
            assert_eq!(threads.len(), tsv.lines().count(), "{cat:?}");
            assert!(threads.len() > 30, "{cat:?}");
            assert!(!cat.brand().contains(char::is_whitespace));
            assert_eq!(Catalog::for_brand(cat.brand()), Some(cat));
            for (i, a) in threads.iter().enumerate() {
                assert!(
                    !threads[i + 1..].iter().any(|b| b.code == a.code),
                    "dup {}",
                    a.code
                );
            }
        }
    }

    #[test]
    fn well_known_codes_are_present_with_expected_colors() {
        assert_eq!(find_dmc("310").unwrap().rgb, [0, 0, 0]);
        assert_eq!(find_dmc("b5200").unwrap().rgb, [255, 255, 255]);
        assert!(find_dmc("ECRU").is_some());
        assert!(find_dmc("not-a-code").is_none());
        assert_eq!(Catalog::Anchor.find("1").unwrap().rgb, [255, 255, 255]);
        assert_eq!(Catalog::Anchor.find("403").unwrap().brand, "Anchor");
        assert_eq!(Catalog::Hama.find("H01").unwrap().name, "White");
        assert_eq!(
            find_thread("Perler 80-15179", Catalog::Dmc).unwrap().name,
            "Evergreen"
        );
        assert_eq!(
            find_thread("Artkal-S S01", Catalog::Dmc).unwrap().brand,
            "Artkal-S"
        );
    }

    #[test]
    fn nearest_match_of_an_exact_catalog_color_is_that_thread() {
        for cat in Catalog::ALL {
            for t in cat.threads().iter().step_by(37) {
                assert_eq!(cat.nearest(t.rgb).rgb, t.rgb, "{cat:?} {}", t.code);
            }
        }
    }

    #[test]
    fn nearest_n_is_sorted_and_starts_with_nearest() {
        let picks = Catalog::Dmc.nearest_n([200, 30, 40], 5);
        assert_eq!(picks.len(), 5);
        assert_eq!(picks[0].code, nearest_dmc([200, 30, 40]).code);
    }

    #[test]
    fn search_and_find_thread() {
        assert!(Catalog::Dmc.search("310").iter().any(|t| t.code == "310"));
        assert!(Catalog::Dmc.search("salmon").len() > 3);
        assert_eq!(
            find_thread("Anchor 403", Catalog::Dmc).unwrap().brand,
            "Anchor"
        );
        assert_eq!(find_thread("310", Catalog::Dmc).unwrap().brand, "DMC");
        assert_eq!(
            find_thread("dmc 310", Catalog::Anchor).unwrap().brand,
            "DMC"
        );
        assert!(Catalog::Dmc.find_by_rgb([0, 0, 0]).is_some());
    }
}
