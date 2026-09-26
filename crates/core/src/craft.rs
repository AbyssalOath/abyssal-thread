/// Which craft a pattern is for. Chosen up front in the GUI's "New
/// pattern" picker and then carried by the pattern itself (a `.oxs` file
/// or a `CRAFT: cross-stitch` `.cgp` is always cross stitch; any other
/// `.cgp` is crochet), rather than being an app-wide mode - so opening a
/// file from a different craft never needs a separate mode switch first.
///
/// Most of these are "a grid of colored cells" underneath (see
/// `ColorGrid`) and differ mainly in color catalog, physical cell size,
/// and chart output, which is why they're listed together even though
/// only some are implemented yet - `is_available` is the switch the
/// picker uses to show the rest as "coming soon".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Craft {
    Crochet,
    CrossStitch,
    Knitting,
    DiamondPainting,
    FuseBeads,
    PixelMacrame,
    LatchHook,
    Quilt,
    PixelHobby,
    PixelArt,
}

impl Craft {
    pub const ALL: [Craft; 10] = [
        Craft::Crochet,
        Craft::CrossStitch,
        Craft::Knitting,
        Craft::DiamondPainting,
        Craft::FuseBeads,
        Craft::PixelMacrame,
        Craft::LatchHook,
        Craft::Quilt,
        Craft::PixelHobby,
        Craft::PixelArt,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Craft::Crochet => "Crochet",
            Craft::CrossStitch => "Cross stitch",
            Craft::Knitting => "Knitting",
            Craft::DiamondPainting => "Diamond painting",
            Craft::FuseBeads => "Fuse beads",
            Craft::PixelMacrame => "Pixel macrame",
            Craft::LatchHook => "Latch hook",
            Craft::Quilt => "Quilt",
            Craft::PixelHobby => "Pixelhobby",
            Craft::PixelArt => "Pixel art / other",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Craft::Crochet => "Shaped pieces from the pattern language, or colorwork/filet charts.",
            Craft::CrossStitch => {
                "Symbol charts on Aida with DMC floss, hoop sizing, and a floss shopping list."
            }
            Craft::Knitting => {
                "Colorwork charts at your gauge, with row instructions and float checks."
            }
            Craft::DiamondPainting => "Drill charts with DMC or Diamond Dotz codes and bag counts.",
            Craft::FuseBeads => "Perler/Hama/Artkal bead charts split into pegboards.",
            Craft::PixelMacrame => "Knot-grid charts for pixel macrame, any colors.",
            Craft::LatchHook => "Rug canvas charts with knot counts, any colors.",
            Craft::Quilt => "Pixel and half-square-triangle quilts with cutting lists and yardage.",
            Craft::PixelHobby => "Baseplate mosaics with pixelsquare counts.",
            Craft::PixelArt => "Plain pixel grids, exported as PNG.",
        }
    }

    /// Whether the GUI can actually create/edit this craft yet.
    pub fn is_available(self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_lists_every_craft_once_with_crochet_and_cross_stitch_available() {
        for (i, a) in Craft::ALL.iter().enumerate() {
            for b in &Craft::ALL[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert!(Craft::Crochet.is_available());
        assert!(Craft::CrossStitch.is_available());
        assert!(Craft::FuseBeads.is_available());
        assert!(Craft::PixelArt.is_available());
        assert!(Craft::Quilt.is_available());
        assert!(Craft::Knitting.is_available());
    }
}
