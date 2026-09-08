/// A flat rectangular grid of colors, one per stitch - the data model for
/// "colorwork"/graphgan-style patterns (a picture crocheted as a grid of
/// single-crochet stitches, one color per pixel), as distinct from the
/// round/row shaping `StitchGraph` models. Produced by
/// `abyssal-thread-imageimport` from a photo/logo; consumed by
/// `abyssal-thread-export::export_color_chart_svg`.
///
/// This deliberately does NOT try to unify with `StitchGraph`/`StitchKind` -
/// the DSL has no color syntax yet (every stitch is one undyed abbreviation),
/// so a "colorwork pattern" today is its own simple data type rather than a
/// stitch graph with a color field bolted on. Teaching the DSL to express
/// `sc(#fdd83f)`-style per-stitch color, and folding that into `StitchNode`,
/// is the natural next step if you want colorwork patterns to flow through
/// the same graph/layout/tension pipeline as shaped patterns.
#[derive(Debug, Clone)]
pub struct ColorGrid {
    pub width: usize,
    pub height: usize,
    /// Row-major, length == width * height. Row 0 is the first row worked.
    pub cells: Vec<[u8; 3]>,
}

impl ColorGrid {
    pub fn new(width: usize, height: usize, fill: [u8; 3]) -> Self {
        Self { width, height, cells: vec![fill; width * height] }
    }

    pub fn get(&self, x: usize, y: usize) -> [u8; 3] {
        self.cells[y * self.width + x]
    }

    pub fn set(&mut self, x: usize, y: usize, color: [u8; 3]) {
        self.cells[y * self.width + x] = color;
    }

    /// Distinct colors used, in first-seen order - handy for building a
    /// legend (color -> symbol/letter) for a text or SVG export.
    pub fn palette(&self) -> Vec<[u8; 3]> {
        let mut seen = Vec::new();
        for &c in &self.cells {
            if !seen.contains(&c) {
                seen.push(c);
            }
        }
        seen
    }
}
