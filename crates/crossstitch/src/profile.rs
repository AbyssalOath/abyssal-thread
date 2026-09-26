//! Per-craft settings for every "grid of colored cells" craft that shares
//! the `Chart` model: what a cell is called, its physical size, which
//! color catalogs apply, which tools make sense, and how materials are
//! bought. Everything craft-specific the GUI, PDF and materials list need
//! lives here, so adding a grid craft is mostly adding a row to each
//! match below.
//!
//! Sources for the physical numbers:
//! - Pixelhobby: a standard baseplate is 10.2 x 12.7 cm holding 40 x 50
//!   pixels (20 x 25 XL pixels), i.e. a 2.54 mm (0.1 in) pitch; a
//!   pixelsquare holds 140 pixels, an XL pixelsquare 60 (pixelhobby.com,
//!   pixelhobby.uk).
//! - Fuse beads: midi beads (Perler, Hama midi, Artkal S) sit on a 5 mm
//!   pegboard; the common large square pegboard is 29 x 29 pegs.
//! - Diamond painting: drills are laid on a 2.5 mm grid, and are sold by
//!   DMC-style color number.
//! - Knitting: stitches are wider than they are tall, so a chart has a
//!   separate row gauge. The presets use stitch counts inside the Craft
//!   Yarn Council's standard ranges per yarn weight (fingering 27-32,
//!   DK 21-24, worsted 16-20, bulky 12-15 sts per 4 in) with typical row
//!   counts - a starting point, not a substitute for a swatch.
//! - Quilting: cells are finished patch sizes; cutting and yardage math
//!   lives in `quilt.rs`.
//!
//! Pack sizes are common retail sizes, not universal - the UI and
//! materials list say so.

use crate::threads::Catalog;
use abyssal_thread_core::Craft;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GridCraft {
    CrossStitch,
    Knitting,
    Quilt,
    DiamondPainting,
    FuseBeads,
    LatchHook,
    PixelHobby,
    PixelMacrame,
    PixelArt,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizePreset {
    pub label: &'static str,
    /// Cells per inch across.
    pub per_inch: f32,
    /// Rows per inch, when cells aren't square (knitting); `None` =
    /// same as `per_inch`.
    pub per_inch_y: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardPreset {
    pub label: &'static str,
    pub width: usize,
    pub height: usize,
}

/// How a craft's material is bought when it's counted per cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Packaging {
    pub per_pack: usize,
    /// Singular, e.g. "bag of 200 drills".
    pub unit: &'static str,
    /// Extra to buy on top of the exact count, as a fraction (0.1 = 10%).
    pub spare: f32,
}

const MM_PER_IN: f32 = 25.4;

const CROSS_STITCH_SIZES: &[SizePreset] = &[
    SizePreset {
        label: "11-count Aida",
        per_inch: 11.0,
        per_inch_y: None,
    },
    SizePreset {
        label: "14-count Aida",
        per_inch: 14.0,
        per_inch_y: None,
    },
    SizePreset {
        label: "16-count Aida",
        per_inch: 16.0,
        per_inch_y: None,
    },
    SizePreset {
        label: "18-count Aida",
        per_inch: 18.0,
        per_inch_y: None,
    },
    SizePreset {
        label: "22-count Aida",
        per_inch: 22.0,
        per_inch_y: None,
    },
];
const DIAMOND_SIZES: &[SizePreset] = &[SizePreset {
    label: "2.5 mm drills",
    per_inch: MM_PER_IN / 2.5,
    per_inch_y: None,
}];
const BEAD_SIZES: &[SizePreset] = &[
    SizePreset {
        label: "Midi beads (5 mm)",
        per_inch: MM_PER_IN / 5.0,
        per_inch_y: None,
    },
    SizePreset {
        label: "Mini beads (2.6 mm)",
        per_inch: MM_PER_IN / 2.6,
        per_inch_y: None,
    },
    SizePreset {
        label: "Maxi beads (10 mm)",
        per_inch: MM_PER_IN / 10.0,
        per_inch_y: None,
    },
];
const LATCH_HOOK_SIZES: &[SizePreset] = &[
    SizePreset {
        label: "3.75 holes/in rug canvas",
        per_inch: 3.75,
        per_inch_y: None,
    },
    SizePreset {
        label: "5 holes/in rug canvas",
        per_inch: 5.0,
        per_inch_y: None,
    },
];
const PIXELHOBBY_SIZES: &[SizePreset] = &[
    SizePreset {
        label: "Standard pixels (2.54 mm)",
        per_inch: 10.0,
        per_inch_y: None,
    },
    SizePreset {
        label: "XL pixels (5.08 mm)",
        per_inch: 5.0,
        per_inch_y: None,
    },
];
const MACRAME_SIZES: &[SizePreset] = &[
    SizePreset {
        label: "8 mm knots",
        per_inch: MM_PER_IN / 8.0,
        per_inch_y: None,
    },
    SizePreset {
        label: "1 cm knots",
        per_inch: MM_PER_IN / 10.0,
        per_inch_y: None,
    },
    SizePreset {
        label: "1.5 cm knots",
        per_inch: MM_PER_IN / 15.0,
        per_inch_y: None,
    },
];
const KNITTING_SIZES: &[SizePreset] = &[
    SizePreset {
        label: "Fingering (28 sts x 36 rows / 4 in)",
        per_inch: 7.0,
        per_inch_y: Some(9.0),
    },
    SizePreset {
        label: "DK (22 sts x 30 rows / 4 in)",
        per_inch: 5.5,
        per_inch_y: Some(7.5),
    },
    SizePreset {
        label: "Worsted (18 sts x 24 rows / 4 in)",
        per_inch: 4.5,
        per_inch_y: Some(6.0),
    },
    SizePreset {
        label: "Bulky (14 sts x 19 rows / 4 in)",
        per_inch: 3.5,
        per_inch_y: Some(4.75),
    },
];
const QUILT_SIZES: &[SizePreset] = &[
    SizePreset {
        label: "1 in finished squares",
        per_inch: 1.0,
        per_inch_y: None,
    },
    SizePreset {
        label: "1.5 in finished squares",
        per_inch: 1.0 / 1.5,
        per_inch_y: None,
    },
    SizePreset {
        label: "2 in finished squares",
        per_inch: 0.5,
        per_inch_y: None,
    },
    SizePreset {
        label: "2.5 in finished squares",
        per_inch: 0.4,
        per_inch_y: None,
    },
    SizePreset {
        label: "3 in finished squares",
        per_inch: 1.0 / 3.0,
        per_inch_y: None,
    },
    SizePreset {
        label: "4 in finished squares",
        per_inch: 0.25,
        per_inch_y: None,
    },
    SizePreset {
        label: "5 in finished squares",
        per_inch: 0.2,
        per_inch_y: None,
    },
    SizePreset {
        label: "6 in finished squares",
        per_inch: 1.0 / 6.0,
        per_inch_y: None,
    },
];
const PIXEL_ART_SIZES: &[SizePreset] = &[SizePreset {
    label: "0.1 in pixels",
    per_inch: 10.0,
    per_inch_y: None,
}];

const PEGBOARDS: &[BoardPreset] = &[BoardPreset {
    label: "Large square pegboard (29 x 29)",
    width: 29,
    height: 29,
}];
const QUILT_BLOCKS: &[BoardPreset] = &[
    BoardPreset {
        label: "Blocks of 3 x 3 squares",
        width: 3,
        height: 3,
    },
    BoardPreset {
        label: "Blocks of 4 x 4 squares",
        width: 4,
        height: 4,
    },
    BoardPreset {
        label: "Blocks of 5 x 5 squares",
        width: 5,
        height: 5,
    },
    BoardPreset {
        label: "Blocks of 6 x 6 squares",
        width: 6,
        height: 6,
    },
];
const PIXELHOBBY_PLATES: &[BoardPreset] = &[
    BoardPreset {
        label: "Baseplate, standard pixels (40 x 50)",
        width: 40,
        height: 50,
    },
    BoardPreset {
        label: "Baseplate, XL pixels (20 x 25)",
        width: 20,
        height: 25,
    },
];

impl GridCraft {
    pub const ALL: [GridCraft; 9] = [
        GridCraft::CrossStitch,
        GridCraft::Knitting,
        GridCraft::Quilt,
        GridCraft::DiamondPainting,
        GridCraft::FuseBeads,
        GridCraft::LatchHook,
        GridCraft::PixelHobby,
        GridCraft::PixelMacrame,
        GridCraft::PixelArt,
    ];

    pub fn craft(self) -> Craft {
        match self {
            GridCraft::CrossStitch => Craft::CrossStitch,
            GridCraft::Knitting => Craft::Knitting,
            GridCraft::Quilt => Craft::Quilt,
            GridCraft::DiamondPainting => Craft::DiamondPainting,
            GridCraft::FuseBeads => Craft::FuseBeads,
            GridCraft::LatchHook => Craft::LatchHook,
            GridCraft::PixelHobby => Craft::PixelHobby,
            GridCraft::PixelMacrame => Craft::PixelMacrame,
            GridCraft::PixelArt => Craft::PixelArt,
        }
    }

    pub fn from_craft(craft: Craft) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.craft() == craft)
    }

    pub fn label(self) -> &'static str {
        self.craft().label()
    }

    /// The value of a `.cgp` file's `CRAFT:` line.
    pub fn cgp_name(self) -> &'static str {
        match self {
            GridCraft::CrossStitch => "cross-stitch",
            GridCraft::Knitting => "knitting",
            GridCraft::Quilt => "quilt",
            GridCraft::DiamondPainting => "diamond-painting",
            GridCraft::FuseBeads => "fuse-beads",
            GridCraft::LatchHook => "latch-hook",
            GridCraft::PixelHobby => "pixelhobby",
            GridCraft::PixelMacrame => "pixel-macrame",
            GridCraft::PixelArt => "pixel-art",
        }
    }

    /// Parses a `CRAFT:` value, forgiving case and spaces/dashes.
    pub fn from_cgp_name(v: &str) -> Option<Self> {
        let norm: String = v
            .chars()
            .filter(|c| !matches!(c, ' ' | '-' | '_'))
            .flat_map(char::to_lowercase)
            .collect();
        Self::ALL
            .into_iter()
            .find(|g| g.cgp_name().replace('-', "") == norm)
    }

    /// What one cell is, singular and plural ("drill", "drills").
    pub fn cell_word(self) -> (&'static str, &'static str) {
        match self {
            GridCraft::CrossStitch | GridCraft::Knitting => ("stitch", "stitches"),
            GridCraft::Quilt => ("square", "squares"),
            GridCraft::DiamondPainting => ("drill", "drills"),
            GridCraft::FuseBeads => ("bead", "beads"),
            GridCraft::LatchHook => ("knot", "knots"),
            GridCraft::PixelHobby | GridCraft::PixelArt => ("pixel", "pixels"),
            GridCraft::PixelMacrame => ("knot", "knots"),
        }
    }

    /// What the design is worked on.
    pub fn surface(self) -> &'static str {
        match self {
            GridCraft::CrossStitch => "Aida",
            GridCraft::Knitting => "knitted fabric",
            GridCraft::Quilt => "quilt top",
            GridCraft::DiamondPainting => "adhesive canvas",
            GridCraft::FuseBeads => "pegboards",
            GridCraft::LatchHook => "rug canvas",
            GridCraft::PixelHobby => "baseplates",
            GridCraft::PixelMacrame => "cord grid",
            GridCraft::PixelArt => "image",
        }
    }

    /// What unfilled cells are called, e.g. for the "leave background
    /// empty" option.
    pub fn background_word(self) -> &'static str {
        match self {
            GridCraft::CrossStitch => "unstitched",
            GridCraft::Knitting => "in the main color",
            GridCraft::Quilt => "as background fabric",
            GridCraft::PixelArt => "transparent",
            _ => "empty",
        }
    }

    pub fn sizes(self) -> &'static [SizePreset] {
        match self {
            GridCraft::CrossStitch => CROSS_STITCH_SIZES,
            GridCraft::Knitting => KNITTING_SIZES,
            GridCraft::Quilt => QUILT_SIZES,
            GridCraft::DiamondPainting => DIAMOND_SIZES,
            GridCraft::FuseBeads => BEAD_SIZES,
            GridCraft::LatchHook => LATCH_HOOK_SIZES,
            GridCraft::PixelHobby => PIXELHOBBY_SIZES,
            GridCraft::PixelMacrame => MACRAME_SIZES,
            GridCraft::PixelArt => PIXEL_ART_SIZES,
        }
    }

    fn default_size(self) -> SizePreset {
        let sizes = self.sizes();
        match self {
            GridCraft::CrossStitch => sizes[1], // 14-count
            GridCraft::Knitting => sizes[1],    // DK
            GridCraft::Quilt => sizes[2],       // 2 in squares
            _ => sizes[0],
        }
    }

    pub fn default_per_inch(self) -> f32 {
        self.default_size().per_inch
    }

    /// Rows per inch for crafts with non-square cells.
    pub fn default_per_inch_y(self) -> Option<f32> {
        self.default_size().per_inch_y
    }

    /// The preset matching `per_inch` (and `per_inch_y`), if any.
    pub fn size_label_xy(self, per_inch: f32, per_inch_y: Option<f32>) -> Option<&'static str> {
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        self.sizes()
            .iter()
            .find(|s| {
                close(s.per_inch, per_inch)
                    && close(
                        s.per_inch_y.unwrap_or(s.per_inch),
                        per_inch_y.unwrap_or(per_inch),
                    )
            })
            .map(|s| s.label)
    }

    /// The preset matching square cells of `per_inch`, if any.
    pub fn size_label(self, per_inch: f32) -> Option<&'static str> {
        self.size_label_xy(per_inch, None)
    }

    /// Whether cells can be non-square (a separate row gauge).
    pub fn has_row_gauge(self) -> bool {
        self == GridCraft::Knitting
    }

    /// Purchasable color catalogs, first = default. Empty means colors
    /// are free RGB (named by nearest common color name).
    pub fn catalogs(self) -> &'static [Catalog] {
        match self {
            GridCraft::CrossStitch => &[Catalog::Dmc, Catalog::Anchor],
            GridCraft::Knitting | GridCraft::Quilt => &[],
            GridCraft::DiamondPainting => &[Catalog::Dmc, Catalog::DiamondDotz],
            GridCraft::FuseBeads => &[
                Catalog::Perler,
                Catalog::Hama,
                Catalog::ArtkalS,
                Catalog::PerlerMini,
                Catalog::HamaMini,
                Catalog::ArtkalC,
            ],
            GridCraft::LatchHook
            | GridCraft::PixelHobby
            | GridCraft::PixelMacrame
            | GridCraft::PixelArt => &[],
        }
    }

    pub fn default_catalog(self) -> Option<Catalog> {
        self.catalogs().first().copied()
    }

    /// Half/quarter/three-quarter stitches, backstitch and French knots.
    pub fn has_part_stitches(self) -> bool {
        self == GridCraft::CrossStitch
    }

    /// Squares split diagonally into two triangles: cross stitch's 3/4
    /// stitches, and quilting's half-square triangles.
    pub fn has_triangles(self) -> bool {
        matches!(self, GridCraft::CrossStitch | GridCraft::Quilt)
    }

    /// Charts numbered knitting-style: row 1 at the bottom, stitch 1 at
    /// the right (the order they're worked in).
    pub fn numbers_from_bottom_right(self) -> bool {
        self == GridCraft::Knitting
    }

    /// Display-in-a-hoop sizing.
    pub fn has_hoop(self) -> bool {
        self == GridCraft::CrossStitch
    }

    /// Board presets (pegboards, baseplates) for this craft at cell size
    /// `per_inch`, first = default. Empty = no boards.
    pub fn boards(self, per_inch: f32) -> &'static [BoardPreset] {
        match self {
            GridCraft::FuseBeads => PEGBOARDS,
            GridCraft::Quilt => QUILT_BLOCKS,
            GridCraft::PixelHobby if per_inch < 7.5 => &PIXELHOBBY_PLATES[1..],
            GridCraft::PixelHobby => &PIXELHOBBY_PLATES[..1],
            _ => &[],
        }
    }

    pub fn board_word(self) -> &'static str {
        match self {
            GridCraft::PixelHobby => "baseplate",
            GridCraft::Quilt => "block",
            _ => "pegboard",
        }
    }

    /// Boards on by default for pegboard/baseplate crafts; quilt blocks
    /// are opt-in (a pixel quilt is often sewn straight in rows).
    pub fn default_board(self, per_inch: f32) -> Option<(usize, usize)> {
        if self == GridCraft::Quilt {
            return None;
        }
        self.boards(per_inch).first().map(|b| (b.width, b.height))
    }

    /// Whether imports leave the background empty by default. Diamond
    /// painting and latch hook canvases are normally covered edge to
    /// edge; beads, Pixelhobby, macrame and pixel art are usually a motif
    /// on nothing.
    pub fn default_skip_background(self) -> bool {
        !matches!(
            self,
            GridCraft::DiamondPainting
                | GridCraft::LatchHook
                | GridCraft::Knitting
                | GridCraft::Quilt
        )
    }

    /// How to buy per-cell materials, for crafts where that's a count
    /// (cross stitch uses thread length -> skeins instead).
    pub fn packaging(self, per_inch: f32) -> Option<Packaging> {
        match self {
            GridCraft::DiamondPainting => Some(Packaging {
                per_pack: 200,
                unit: "bag of 200 drills",
                spare: 0.10,
            }),
            GridCraft::FuseBeads => Some(Packaging {
                per_pack: 1000,
                unit: "bag of 1,000 beads",
                spare: 0.05,
            }),
            GridCraft::PixelHobby if per_inch < 7.5 => Some(Packaging {
                per_pack: 60,
                unit: "XL pixelsquare (60 pixels)",
                spare: 0.0,
            }),
            GridCraft::PixelHobby => Some(Packaging {
                per_pack: 140,
                unit: "pixelsquare (140 pixels)",
                spare: 0.0,
            }),
            _ => None,
        }
    }

    /// Starting canvas for a blank chart, in cells.
    pub fn default_chart_size(self) -> (usize, usize) {
        match self {
            // 5in at 14-count - comfortably inside a 7-8in hoop.
            GridCraft::CrossStitch => (70, 70),
            GridCraft::Knitting => (40, 30),
            // 24 x 32 in at 2 in squares - a crib quilt.
            GridCraft::Quilt => (12, 16),
            GridCraft::DiamondPainting => (120, 160),
            GridCraft::FuseBeads => (29, 29),
            GridCraft::LatchHook => (60, 80),
            GridCraft::PixelHobby => (40, 50),
            GridCraft::PixelMacrame => (30, 40),
            GridCraft::PixelArt => (32, 32),
        }
    }

    /// Most colors a picture import may use.
    pub fn max_colors(self) -> usize {
        match self {
            GridCraft::DiamondPainting => 60,
            // Stranded colorwork rarely uses more than a handful of
            // colors; quilts a couple of dozen fabrics.
            GridCraft::Knitting => 8,
            GridCraft::Quilt => 24,
            GridCraft::PixelArt => 64,
            _ => 40,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crafts_round_trip_through_cgp_names_and_core_crafts() {
        for g in GridCraft::ALL {
            assert_eq!(GridCraft::from_cgp_name(g.cgp_name()), Some(g));
            assert_eq!(GridCraft::from_craft(g.craft()), Some(g));
            assert!(!g.sizes().is_empty());
            assert!(g.max_colors() <= crate::chart::SYMBOLS.len());
        }
        assert_eq!(
            GridCraft::from_cgp_name("Cross Stitch"),
            Some(GridCraft::CrossStitch)
        );
        assert_eq!(
            GridCraft::from_cgp_name("crossstitch"),
            Some(GridCraft::CrossStitch)
        );
        assert_eq!(GridCraft::from_craft(Craft::Crochet), None);
        assert_eq!(GridCraft::from_cgp_name("crochet"), None);
    }

    #[test]
    fn pixelhobby_plate_math_matches_the_published_numbers() {
        // 40 x 50 standard pixels at 10/in = 4 x 5 in = 10.2 x 12.7 cm.
        let (w, h) = GridCraft::PixelHobby.default_board(10.0).unwrap();
        assert_eq!((w, h), (40, 50));
        assert!((w as f32 / 10.0 * 2.54 - 10.16).abs() < 0.01);
        assert_eq!(GridCraft::PixelHobby.default_board(5.0), Some((20, 25)));
        assert_eq!(GridCraft::PixelHobby.packaging(10.0).unwrap().per_pack, 140);
        assert_eq!(GridCraft::PixelHobby.packaging(5.0).unwrap().per_pack, 60);
    }

    #[test]
    fn size_label_finds_presets() {
        assert_eq!(
            GridCraft::CrossStitch.size_label(14.0),
            Some("14-count Aida")
        );
        assert_eq!(
            GridCraft::FuseBeads.size_label(5.08),
            Some("Midi beads (5 mm)")
        );
        assert_eq!(GridCraft::CrossStitch.size_label(13.0), None);
        assert_eq!(
            GridCraft::Knitting.size_label_xy(5.5, Some(7.5)),
            Some("DK (22 sts x 30 rows / 4 in)")
        );
        assert_eq!(GridCraft::Knitting.size_label_xy(5.5, None), None);
        assert_eq!(GridCraft::Quilt.default_per_inch(), 0.5);
        assert_eq!(GridCraft::Quilt.default_board(0.5), None);
        assert_eq!(GridCraft::Knitting.default_per_inch_y(), Some(7.5));
    }
}
