//! The cross-stitch chart data model.
//!
//! Deliberately a separate type from `abyssal_thread_core::ColorGrid`
//! rather than a `ColorGrid` plus metadata, because a cross-stitch chart
//! differs from a crochet colorwork grid in the ways that matter most:
//!
//! - A square can be *unstitched* (`None`) - the fabric shows through.
//!   Crochet colorwork has no equivalent; every cell is a stitch.
//! - Every color is a specific, purchasable floss (brand + code), each
//!   with its own chart symbol and strand count, not a bare RGB value.
//! - Physical size comes from the fabric count (squares per inch), not a
//!   yarn/hook gauge swatch.
//! - Besides full crosses, a square can hold part stitches (half, 3/4,
//!   quarter), and backstitch lines and French knots sit on the grid
//!   lines between squares.
//!
//! `to_color_grid`/`from_color_grid` bridge to `ColorGrid` so the existing
//! image/text import pipeline (resize + k-means quantize) is reused as-is.
//!
//! Coordinates: squares are `(x, y)`, row 0 at the top. Backstitch and
//! knot positions are on the grid *lines*, in half-square units - `(0, 0)`
//! is the top-left corner of the chart, `(1, 1)` the center of the
//! top-left square, `(2, 0)` the corner between the first two squares -
//! matching OXS, which allows `.5` positions and nothing finer.

use crate::profile::GridCraft;
use crate::threads::{Catalog, Thread};
use abyssal_thread_core::ColorGrid;
use std::collections::BTreeMap;

/// Chart symbols, in the order they're assigned. ASCII only, on purpose:
/// the PDF export draws them with a built-in PDF font (Helvetica), which
/// covers ASCII reliably and little else, and egui's default font covers
/// them too. Ordered roughly most-to-least visually distinct, so small
/// palettes get the easiest-to-tell-apart shapes. No `(`, `)` or `\` -
/// those need escaping in PDF text strings.
pub const SYMBOLS: &[char] = &[
    'X', 'O', '+', '/', '#', '*', '=', '@', '%', '&', '$', '^', '~', '<', '>', '?', '!', 'S', 'T',
    'V', 'Z', 'N', 'H', 'K', 'M', 'W', 'A', 'E', 'U', 'C', 'D', 'L', 'P', 'R', 'Y', 'F', 'G', 'B',
    'J', 'Q', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'd', 'e', 'g', 'h', 'k', 'n', 'q',
    'r', 's', 'u', 'v', 'w', 'x', 'y', 'z', '[', ']', '{', '}', ':', ';', '|',
];

/// A second thread stitched together with a palette entry's own thread in
/// the same needle (a "blend" - e.g. one strand of each).
#[derive(Debug, Clone, PartialEq)]
pub struct BlendThread {
    pub brand: String,
    pub code: String,
    pub name: String,
    pub rgb: [u8; 3],
}

impl BlendThread {
    pub fn from_thread(t: &Thread) -> Self {
        Self {
            brand: t.brand.to_string(),
            code: t.code.to_string(),
            name: t.name.to_string(),
            rgb: t.rgb,
        }
    }

    pub fn label(&self) -> String {
        thread_label(&self.brand, &self.code, &self.name)
    }
}

fn thread_label(brand: &str, code: &str, name: &str) -> String {
    format!("{brand} {code} {name}")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// One palette entry: a specific floss and how it's shown/stitched.
#[derive(Debug, Clone, PartialEq)]
pub struct Floss {
    /// "DMC", "Anchor", ... - or "" for a custom, non-catalog color.
    pub brand: String,
    /// "310", "B5200", ... - or "" for a custom color.
    pub code: String,
    pub name: String,
    pub rgb: [u8; 3],
    pub symbol: char,
    /// Strands used for crosses, part stitches and knots (2 is standard
    /// on 14-count Aida). For a blend, strands *of each* thread.
    pub strands: u8,
    /// Strands used for backstitch (usually 1).
    pub bs_strands: u8,
    pub blend: Option<BlendThread>,
}

impl Floss {
    pub fn from_thread(t: &Thread, symbol: char) -> Self {
        Self {
            brand: t.brand.to_string(),
            code: t.code.to_string(),
            name: t.name.to_string(),
            rgb: t.rgb,
            symbol,
            strands: 2,
            bs_strands: 1,
            blend: None,
        }
    }

    pub fn custom(name: &str, rgb: [u8; 3], symbol: char) -> Self {
        Self {
            brand: String::new(),
            code: String::new(),
            name: name.to_string(),
            rgb,
            symbol,
            strands: 2,
            bs_strands: 1,
            blend: None,
        }
    }

    /// A free (non-catalog) color, named by its nearest common color
    /// name and hex code - what crafts without a color catalog (latch
    /// hook, Pixelhobby, macrame, pixel art) use.
    pub fn free(rgb: [u8; 3], symbol: char) -> Self {
        let name = format!(
            "{} #{:02x}{:02x}{:02x}",
            abyssal_thread_export::nearest_color_name(rgb),
            rgb[0],
            rgb[1],
            rgb[2]
        );
        Self::custom(&name, rgb, symbol)
    }

    pub fn is_custom(&self) -> bool {
        self.brand.is_empty() && self.code.is_empty()
    }

    /// "DMC 310 Black", "Anchor 403", "DMC 310 Black + DMC 3865 Winter
    /// White", or just the name for a custom color.
    pub fn label(&self) -> String {
        let own = if self.is_custom() {
            self.name.clone()
        } else {
            thread_label(&self.brand, &self.code, &self.name)
        };
        match &self.blend {
            Some(b) => format!("{own} + {}", b.label()),
            None => own,
        }
    }

    /// The color to draw this entry in - for a blend, the average of its
    /// two threads (roughly how the mixed strands read from a distance).
    pub fn display_rgb(&self) -> [u8; 3] {
        match &self.blend {
            Some(b) => [0, 1, 2].map(|i| ((self.rgb[i] as u16 + b.rgb[i] as u16) / 2) as u8),
            None => self.rgb,
        }
    }

    /// Same thread(s), ignoring presentation (symbol/strands).
    pub fn same_thread(&self, other: &Floss) -> bool {
        let own_same = if self.is_custom() {
            other.is_custom() && self.rgb == other.rgb
        } else {
            self.brand.eq_ignore_ascii_case(&other.brand)
                && self.code.eq_ignore_ascii_case(&other.code)
        };
        let blend_same = match (&self.blend, &other.blend) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                a.brand.eq_ignore_ascii_case(&b.brand)
                    && a.code.eq_ignore_ascii_case(&b.code)
                    && a.rgb == b.rgb
            }
            _ => false,
        };
        own_same && blend_same
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fabric {
    /// Cells per inch across - 14 for "14-count" Aida, stitches per inch
    /// for knitting, 0.5 for 2in quilt squares. f32 because evenweave
    /// "28 over 2" is really 14, and gauges are fractional.
    pub count: f32,
    /// Rows per inch when cells aren't square (knitting's row gauge);
    /// `None` = same as `count`.
    pub count_y: Option<f32>,
    pub rgb: [u8; 3],
}

impl Default for Fabric {
    fn default() -> Self {
        Self {
            count: 14.0,
            count_y: None,
            rgb: [255, 255, 255],
        }
    }
}

impl Fabric {
    /// Rows per inch.
    pub fn rows_per_inch(&self) -> f32 {
        self.count_y.unwrap_or(self.count)
    }

    /// A cell's height relative to its width (1.0 = square; knit
    /// stitches are ~0.7).
    pub fn cell_aspect(&self) -> f32 {
        (self.count / self.rows_per_inch().max(0.01)).clamp(0.2, 5.0)
    }
}

/// Which way a diagonal runs across a square.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Diagonal {
    /// `/` - bottom-left to top-right.
    Slash,
    /// `\` - top-left to bottom-right.
    Backslash,
}

impl Diagonal {
    pub fn as_char(self) -> char {
        match self {
            Diagonal::Slash => '/',
            Diagonal::Backslash => '\\',
        }
    }

    pub fn from_char(c: char) -> Option<Self> {
        match c {
            '/' => Some(Diagonal::Slash),
            '\\' => Some(Diagonal::Backslash),
            _ => None,
        }
    }
}

/// A square's corner, also used as the index into `Partial::Quarters`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Corner {
    TopLeft = 0,
    TopRight = 1,
    BottomLeft = 2,
    BottomRight = 3,
}

impl Corner {
    pub const ALL: [Corner; 4] = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomRight,
    ];

    /// The corner nearest a point at fractional position `(fx, fy)`
    /// within a square (each 0..1).
    pub fn nearest(fx: f32, fy: f32) -> Self {
        match (fx >= 0.5, fy >= 0.5) {
            (false, false) => Corner::TopLeft,
            (true, false) => Corner::TopRight,
            (false, true) => Corner::BottomLeft,
            (true, true) => Corner::BottomRight,
        }
    }

    pub fn is_right(self) -> bool {
        matches!(self, Corner::TopRight | Corner::BottomRight)
    }

    pub fn is_bottom(self) -> bool {
        matches!(self, Corner::BottomLeft | Corner::BottomRight)
    }
}

/// Part stitches in one square (a square holds either a full cross, or
/// one of these, or nothing).
#[derive(Debug, Clone, PartialEq)]
pub enum Partial {
    /// A single diagonal stitch (a "half stitch" / tent stitch).
    Half { diagonal: Diagonal, floss: u16 },
    /// The square split along `diagonal` into two triangles, each either
    /// a three-quarter stitch in some floss or empty. For `Backslash`,
    /// `first` is the bottom-left triangle and `second` the top-right; for
    /// `Slash`, `first` is the top-left and `second` the bottom-right -
    /// OXS's "color on the left"/"color on the right" for part stitches
    /// with direction 1 and 2.
    Split {
        diagonal: Diagonal,
        first: Option<u16>,
        second: Option<u16>,
    },
    /// Quarter stitches, indexed by `Corner`.
    Quarters([Option<u16>; 4]),
}

impl Partial {
    /// The split diagonal and triangle side that contains `corner`: a
    /// three-quarter stitch "toward" a corner fills that corner's
    /// triangle.
    pub fn split_side(corner: Corner) -> (Diagonal, bool) {
        match corner {
            Corner::TopLeft => (Diagonal::Slash, true),
            Corner::BottomRight => (Diagonal::Slash, false),
            Corner::BottomLeft => (Diagonal::Backslash, true),
            Corner::TopRight => (Diagonal::Backslash, false),
        }
    }

    pub fn flosses(&self) -> Vec<u16> {
        match self {
            Partial::Half { floss, .. } => vec![*floss],
            Partial::Split { first, second, .. } => {
                first.iter().chain(second.iter()).copied().collect()
            }
            Partial::Quarters(q) => q.iter().flatten().copied().collect(),
        }
    }

    /// Applies `f` to every floss reference; returns `None` if nothing is
    /// left afterwards.
    pub fn map_floss(&self, f: impl Fn(u16) -> Option<u16>) -> Option<Partial> {
        let p = match self {
            Partial::Half { diagonal, floss } => Partial::Half {
                diagonal: *diagonal,
                floss: f(*floss)?,
            },
            Partial::Split {
                diagonal,
                first,
                second,
            } => Partial::Split {
                diagonal: *diagonal,
                first: first.and_then(&f),
                second: second.and_then(&f),
            },
            Partial::Quarters(q) => Partial::Quarters(q.map(|v| v.and_then(&f))),
        };
        (!p.is_empty()).then_some(p)
    }

    pub fn is_empty(&self) -> bool {
        self.flosses().is_empty()
    }
}

/// A straight backstitch line between two grid points (half-square units,
/// see the module doc).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Backstitch {
    pub from: (u32, u32),
    pub to: (u32, u32),
    pub floss: u16,
}

impl Backstitch {
    /// Visible length in squares.
    pub fn length(&self) -> f32 {
        let dx = self.from.0 as f32 - self.to.0 as f32;
        let dy = self.from.1 as f32 - self.to.1 as f32;
        (dx * dx + dy * dy).sqrt() / 2.0
    }
}

/// A French knot at a grid point (half-square units).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Knot {
    pub at: (u32, u32),
    pub floss: u16,
}

/// Common Aida counts, for the GUI's fabric dropdown.
pub const COMMON_COUNTS: &[f32] = &[11.0, 14.0, 16.0, 18.0, 22.0];

/// Common embroidery hoop diameters, inches.
pub const COMMON_HOOPS_IN: &[f32] = &[4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 12.0];

/// Minimum unstitched fabric to leave between a design and the inside of
/// a hoop it's displayed in, inches.
pub const HOOP_MARGIN_IN: f32 = 0.5;

/// How much of each stitch kind one palette entry uses.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Usage {
    pub full: usize,
    pub half: usize,
    pub three_quarter: usize,
    pub quarter: usize,
    /// Total visible backstitch length, in squares.
    pub backstitch_squares: f32,
    pub knots: usize,
}

impl Usage {
    /// Crosses and part stitches (not backstitch/knots).
    pub fn stitches(&self) -> usize {
        self.full + self.half + self.three_quarter + self.quarter
    }

    pub fn is_empty(&self) -> bool {
        self.stitches() == 0 && self.knots == 0 && self.backstitch_squares == 0.0
    }
}

/// One thing to buy, aggregated across every palette entry that uses it
/// (for cross stitch, a blend contributes to both of its threads).
#[derive(Debug, Clone, PartialEq)]
pub struct ShoppingItem {
    pub label: String,
    pub rgb: [u8; 3],
    /// Cells of this color in the design (for cross stitch: crosses and
    /// part stitches).
    pub used: usize,
    /// Packs to buy (skeins, bags, pixelsquares), or 0 when the craft has
    /// no standard pack (just buy enough for `used`).
    pub buy: usize,
    /// Singular pack name, e.g. "skein", "bag of 200 drills"; empty when
    /// `buy` is 0.
    pub unit: &'static str,
    /// What to buy when it isn't a pack count, e.g. "1 3/8 yd" or
    /// "34% of the yarn"; takes precedence over `buy`/`unit`.
    pub note: String,
}

impl ShoppingItem {
    /// "3 skeins", "1 bag of 200 drills", "4 pixelsquares (140 pixels)".
    pub fn buy_text(&self) -> String {
        if !self.note.is_empty() {
            return self.note.clone();
        }
        if self.unit.is_empty() {
            return String::new();
        }
        if self.buy == 1 {
            return format!("1 {}", self.unit);
        }
        // Pluralize the first word: "bag of 200" -> "bags of 200".
        match self.unit.split_once(' ') {
            Some((first, rest)) => format!("{} {first}s {rest}", self.buy),
            None => format!("{} {}s", self.buy, self.unit),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub name: Option<String>,
    /// Which grid craft this chart is for - drives sizes, catalogs,
    /// tools and materials (see `profile`).
    pub craft: GridCraft,
    /// Pegboard/baseplate (or quilt block) size in cells, for crafts
    /// built from boards/blocks.
    pub board: Option<(usize, usize)>,
    /// Knitting: worked in the round (every row read right to left)
    /// rather than flat (wrong-side rows read left to right).
    pub worked_in_round: bool,
    pub width: usize,
    pub height: usize,
    pub fabric: Fabric,
    pub palette: Vec<Floss>,
    /// Full crosses. Row-major, `width * height`. `None` = no full cross
    /// (unstitched, or see `partials`); `Some(i)` indexes `palette`.
    pub cells: Vec<Option<u16>>,
    /// Part stitches, keyed `(y, x)` so iteration runs in row order.
    /// Never present for a square that has a full cross.
    pub partials: BTreeMap<(usize, usize), Partial>,
    pub backstitches: Vec<Backstitch>,
    pub knots: Vec<Knot>,
}

impl Chart {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            name: None,
            craft: GridCraft::CrossStitch,
            board: None,
            worked_in_round: false,
            width,
            height,
            fabric: Fabric::default(),
            palette: Vec::new(),
            cells: vec![None; width * height],
            partials: BTreeMap::new(),
            backstitches: Vec::new(),
            knots: Vec::new(),
        }
    }

    /// A blank chart for `craft` at its default size, cell size and board.
    pub fn new_for(craft: GridCraft) -> Self {
        let (w, h) = craft.default_chart_size();
        let mut c = Self::new(w, h);
        c.craft = craft;
        c.fabric.count = craft.default_per_inch();
        c.fabric.count_y = craft.default_per_inch_y();
        c.board = craft.default_board(c.fabric.count);
        c
    }

    /// Boards needed across and down, if this chart uses boards.
    pub fn boards_needed(&self) -> Option<(usize, usize)> {
        let (bw, bh) = self.board?;
        Some((
            self.width.div_ceil(bw.max(1)),
            self.height.div_ceil(bh.max(1)),
        ))
    }

    pub fn get(&self, x: usize, y: usize) -> Option<u16> {
        self.cells[y * self.width + x]
    }

    /// Sets the full cross at `(x, y)`. Stitching a full cross replaces
    /// any part stitches there.
    pub fn set(&mut self, x: usize, y: usize, v: Option<u16>) {
        self.cells[y * self.width + x] = v;
        if v.is_some() {
            self.partials.remove(&(y, x));
        }
    }

    pub fn partial(&self, x: usize, y: usize) -> Option<&Partial> {
        self.partials.get(&(y, x))
    }

    /// Puts part stitches in `(x, y)`, replacing any full cross there. An
    /// empty `Partial` just clears the square's part stitches.
    pub fn set_partial(&mut self, x: usize, y: usize, p: Partial) {
        self.cells[y * self.width + x] = None;
        if p.is_empty() {
            self.partials.remove(&(y, x));
        } else {
            self.partials.insert((y, x), p);
        }
    }

    /// Removes everything stitched in square `(x, y)`.
    pub fn clear_square(&mut self, x: usize, y: usize) {
        self.cells[y * self.width + x] = None;
        self.partials.remove(&(y, x));
    }

    /// Whether anything (full or part stitch) is in square `(x, y)`.
    pub fn is_stitched(&self, x: usize, y: usize) -> bool {
        self.get(x, y).is_some() || self.partials.contains_key(&(y, x))
    }

    /// The first symbol not already used by this palette.
    pub fn next_symbol(&self) -> char {
        SYMBOLS
            .iter()
            .copied()
            .find(|s| !self.palette.iter().any(|f| f.symbol == *s))
            .unwrap_or(SYMBOLS[self.palette.len() % SYMBOLS.len()])
    }

    /// Adds `floss` (reassigning its symbol if already taken) and returns
    /// its palette index - or the existing index if that thread is
    /// already in the palette.
    pub fn add_floss(&mut self, mut floss: Floss) -> u16 {
        if let Some(i) = self.palette.iter().position(|f| f.same_thread(&floss)) {
            return i as u16;
        }
        if floss.symbol.is_whitespace() || self.palette.iter().any(|f| f.symbol == floss.symbol) {
            floss.symbol = self.next_symbol();
        }
        self.palette.push(floss);
        (self.palette.len() - 1) as u16
    }

    /// Removes palette entry `index`; everything stitched in it is
    /// removed and later indices shift down.
    pub fn remove_floss(&mut self, index: usize) {
        if index >= self.palette.len() {
            return;
        }
        self.palette.remove(index);
        let removed = index as u16;
        self.remap_floss(move |i: u16| match i {
            i if i == removed => None,
            i if i > removed => Some(i - 1),
            i => Some(i),
        });
    }

    /// Rewrites every floss reference through `remap` (`None` = remove
    /// that stitch). The palette itself is the caller's job.
    fn remap_floss(&mut self, remap: impl Fn(u16) -> Option<u16> + Copy) {
        for c in self.cells.iter_mut() {
            *c = c.and_then(remap);
        }
        self.partials = std::mem::take(&mut self.partials)
            .into_iter()
            .filter_map(|(k, p)| Some((k, p.map_floss(remap)?)))
            .collect();
        self.backstitches = std::mem::take(&mut self.backstitches)
            .into_iter()
            .filter_map(|b| {
                Some(Backstitch {
                    floss: remap(b.floss)?,
                    ..b
                })
            })
            .collect();
        self.knots = std::mem::take(&mut self.knots)
            .into_iter()
            .filter_map(|k| {
                Some(Knot {
                    floss: remap(k.floss)?,
                    ..k
                })
            })
            .collect();
    }

    /// Replaces every palette color with its nearest match in `catalog`
    /// (or, with `None`, turns each into a free color of its current
    /// RGB), keeping symbols; entries that land on the same thread are
    /// merged. Blends are dropped, since they're specific to embroidery
    /// floss. For switching a chart to another craft or brand.
    pub fn rematch_colors(&mut self, catalog: Option<Catalog>) {
        let old = std::mem::take(&mut self.palette);
        let mut mapping = Vec::with_capacity(old.len());
        for f in &old {
            let mut new = match catalog {
                Some(cat) => Floss::from_thread(cat.nearest(f.display_rgb()), f.symbol),
                None => Floss::free(f.display_rgb(), f.symbol),
            };
            new.strands = f.strands;
            new.bs_strands = f.bs_strands;
            mapping.push(self.add_floss(new));
        }
        self.remap_floss(|i| mapping.get(i as usize).copied());
    }

    /// Drops palette entries nothing is stitched in.
    pub fn remove_unused_floss(&mut self) {
        let usage = self.usage();
        for i in (0..self.palette.len()).rev() {
            if usage[i].is_empty() {
                self.remove_floss(i);
            }
        }
    }

    /// Per-palette-entry usage of every stitch kind.
    pub fn usage(&self) -> Vec<Usage> {
        let mut u = vec![Usage::default(); self.palette.len()];
        let mut bump = |i: u16, f: &dyn Fn(&mut Usage)| {
            if let Some(x) = u.get_mut(i as usize) {
                f(x);
            }
        };
        for i in self.cells.iter().flatten() {
            bump(*i, &|x| x.full += 1);
        }
        for p in self.partials.values() {
            match p {
                Partial::Half { floss, .. } => bump(*floss, &|x| x.half += 1),
                Partial::Split { first, second, .. } => {
                    for i in first.iter().chain(second.iter()) {
                        bump(*i, &|x| x.three_quarter += 1);
                    }
                }
                Partial::Quarters(q) => {
                    for i in q.iter().flatten() {
                        bump(*i, &|x| x.quarter += 1);
                    }
                }
            }
        }
        for b in &self.backstitches {
            let len = b.length();
            bump(b.floss, &|x| x.backstitch_squares += len);
        }
        for k in &self.knots {
            bump(k.floss, &|x| x.knots += 1);
        }
        u
    }

    /// Full-cross count per palette entry.
    pub fn stitch_counts(&self) -> Vec<usize> {
        self.usage().iter().map(|u| u.full).collect()
    }

    /// Full crosses.
    pub fn total_stitches(&self) -> usize {
        self.cells.iter().filter(|c| c.is_some()).count()
    }

    /// Estimated thread, in mm of single strand, for palette entry `i`
    /// (for a blend: of *each* of its two threads).
    pub fn thread_mm(&self, i: usize) -> f32 {
        let (Some(f), Some(u)) = (self.palette.get(i), self.usage().get(i).cloned()) else {
            return 0.0;
        };
        thread_mm_for(&u, f, self.fabric.count)
    }

    /// What to buy. Cross stitch: skeins per physical thread, merged
    /// across palette entries (a thread used on its own and in a blend is
    /// one item). Other crafts: one item per palette entry, with packs
    /// from the craft's `Packaging` (plus its spare allowance) when it
    /// has one.
    pub fn shopping_list(&self) -> Vec<ShoppingItem> {
        match self.craft {
            GridCraft::Quilt => return crate::quilt::plan(self).shopping_list(),
            GridCraft::Knitting => return self.yarn_shares(),
            _ => {}
        }
        let usage = self.usage();
        if self.craft != GridCraft::CrossStitch {
            let packaging = self.craft.packaging(self.fabric.count);
            return self
                .palette
                .iter()
                .zip(&usage)
                .filter(|(_, u)| u.full > 0)
                .map(|(f, u)| {
                    let (buy, unit) = match packaging {
                        Some(p) => (
                            ((u.full as f32 * (1.0 + p.spare)) / p.per_pack as f32).ceil() as usize,
                            p.unit,
                        ),
                        None => (0, ""),
                    };
                    ShoppingItem {
                        label: f.label(),
                        rgb: f.rgb,
                        used: u.full,
                        buy,
                        unit,
                        note: String::new(),
                    }
                })
                .collect();
        }
        let mut items: Vec<(String, [u8; 3], f32, usize)> = Vec::new();
        let mut add = |label: String, rgb: [u8; 3], mm: f32, used: usize| match items
            .iter_mut()
            .find(|(l, _, _, _)| *l == label)
        {
            Some(item) => {
                item.2 += mm;
                item.3 += used;
            }
            None => items.push((label, rgb, mm, used)),
        };
        for (f, u) in self.palette.iter().zip(&usage) {
            let mm = thread_mm_for(u, f, self.fabric.count);
            if mm <= 0.0 {
                continue;
            }
            let own = if f.is_custom() {
                f.name.clone()
            } else {
                thread_label(&f.brand, &f.code, &f.name)
            };
            add(own, f.rgb, mm, u.stitches());
            if let Some(b) = &f.blend {
                add(b.label(), b.rgb, mm, u.stitches());
            }
        }
        items
            .into_iter()
            .map(|(label, rgb, mm, used)| ShoppingItem {
                label,
                rgb,
                used,
                buy: (mm / SKEIN_STRAND_MM).ceil().max(1.0) as usize,
                unit: "skein",
                note: String::new(),
            })
            .collect()
    }

    /// Knitting: every stitch is some yarn - empty cells are the main
    /// (background) color - so the useful "shopping" figure is each
    /// yarn's share of the stitches, to split a pattern's total yardage
    /// (or a swatch-based estimate) by.
    fn yarn_shares(&self) -> Vec<ShoppingItem> {
        let total = (self.width * self.height).max(1);
        let usage = self.usage();
        let background = self.cells.iter().filter(|c| c.is_none()).count();
        let share = |n: usize| {
            let pct = n as f32 * 100.0 / total as f32;
            if pct > 0.0 && pct < 1.0 {
                format!("{n} sts (<1% of the yarn)")
            } else {
                format!("{n} sts ({pct:.0}% of the yarn)")
            }
        };
        let mut items = Vec::new();
        if background > 0 {
            items.push(ShoppingItem {
                label: "Main color (background)".to_string(),
                rgb: self.fabric.rgb,
                used: background,
                buy: 0,
                unit: "",
                note: share(background),
            });
        }
        items.extend(
            self.palette
                .iter()
                .zip(&usage)
                .filter(|(_, u)| u.full > 0)
                .map(|(f, u)| ShoppingItem {
                    label: f.label(),
                    rgb: f.rgb,
                    used: u.full,
                    buy: 0,
                    unit: "",
                    note: share(u.full),
                }),
        );
        items
    }

    /// Renders full crosses to a plain color grid; everything else
    /// (unstitched squares, and squares holding only part stitches) shows
    /// as fabric.
    pub fn to_color_grid(&self) -> ColorGrid {
        ColorGrid {
            width: self.width,
            height: self.height,
            cells: self
                .cells
                .iter()
                .map(|c| match c {
                    Some(i) => self
                        .palette
                        .get(*i as usize)
                        .map_or(self.fabric.rgb, |f| f.display_rgb()),
                    None => self.fabric.rgb,
                })
                .collect(),
        }
    }

    /// Converts an (already quantized) color grid to a chart, matching
    /// every distinct color to its nearest floss in `catalog` (or, with
    /// `None`, keeping each color as a free color). Two source
    /// colors that land on the same floss merge into one palette entry,
    /// so the result can have fewer colors than the source.
    ///
    /// With `skip_background`, the most common color along the grid's
    /// border is treated as background and left unstitched - the usual
    /// choice for a motif on plain Aida, and far less stitching.
    pub fn from_color_grid(
        grid: &ColorGrid,
        fabric: Fabric,
        skip_background: bool,
        catalog: Option<Catalog>,
    ) -> Self {
        let mut chart = Chart::new(grid.width, grid.height);
        chart.fabric = fabric;
        let background = if skip_background {
            border_mode(grid)
        } else {
            None
        };
        // Distinct source colors -> palette index, matched once each
        // rather than per cell (the grid has a handful of distinct colors
        // but possibly tens of thousands of cells).
        let mut mapping: Vec<([u8; 3], Option<u16>)> = Vec::new();
        for (i, &c) in grid.cells.iter().enumerate() {
            let idx = match mapping.iter().find(|(k, _)| *k == c) {
                Some((_, v)) => *v,
                None => {
                    let v = if Some(c) == background {
                        None
                    } else {
                        let symbol = chart.next_symbol();
                        let floss = match catalog {
                            Some(cat) => Floss::from_thread(cat.nearest(c), symbol),
                            None => Floss::free(c, symbol),
                        };
                        Some(chart.add_floss(floss))
                    };
                    mapping.push((c, v));
                    v
                }
            };
            chart.cells[i] = idx;
        }
        chart
    }

    /// Resizes by cropping/padding, anchored top-left (new area
    /// unstitched) - for hand-drawn charts with no source image to
    /// re-render from. Part stitches, backstitches and knots outside the
    /// new bounds are dropped.
    pub fn crop_or_pad(&mut self, width: usize, height: usize) {
        let (width, height) = (width.max(1), height.max(1));
        let mut cells = vec![None; width * height];
        for y in 0..height.min(self.height) {
            for x in 0..width.min(self.width) {
                cells[y * width + x] = self.get(x, y);
            }
        }
        self.width = width;
        self.height = height;
        self.cells = cells;
        self.partials.retain(|&(y, x), _| x < width && y < height);
        let inside = |(x, y): (u32, u32)| x as usize <= 2 * width && y as usize <= 2 * height;
        self.backstitches.retain(|b| inside(b.from) && inside(b.to));
        self.knots.retain(|k| inside(k.at));
    }

    /// Flood-fills the 4-connected region of full crosses sharing
    /// `(x, y)`'s value (squares with part stitches act as walls).
    pub fn flood_fill(&mut self, x: usize, y: usize, value: Option<u16>) {
        let target = self.get(x, y);
        if target == value || self.partials.contains_key(&(y, x)) {
            return;
        }
        let mut stack = vec![(x, y)];
        while let Some((x, y)) = stack.pop() {
            if self.get(x, y) != target || self.partials.contains_key(&(y, x)) {
                continue;
            }
            self.set(x, y, value);
            if x + 1 < self.width {
                stack.push((x + 1, y));
            }
            if x > 0 {
                stack.push((x - 1, y));
            }
            if y + 1 < self.height {
                stack.push((x, y + 1));
            }
            if y > 0 {
                stack.push((x, y - 1));
            }
        }
    }

    /// Removes "confetti": every 4-connected patch of same-value squares
    /// (a floss, or bare fabric) smaller than `min_size` squares is
    /// recolored to whatever surrounds it most (another floss or
    /// unstitched). Photo imports produce lots of these - lone stitches
    /// of an in-between color along edges - and each one means starting
    /// and ending a thread for a single cross. Decisions are all made
    /// against the original chart, so the result doesn't depend on scan
    /// order. Squares with part stitches are left alone and don't count
    /// as neighbors. Returns how many squares changed.
    pub fn remove_confetti(&mut self, min_size: usize) -> usize {
        if min_size <= 1 || self.cells.is_empty() {
            return 0;
        }
        let (w, h) = (self.width, self.height);
        let has_partial = |i: usize| self.partials.contains_key(&(i / w, i % w));
        let mut comp = vec![usize::MAX; w * h];
        let mut changes: Vec<(usize, Option<u16>)> = Vec::new();
        for start in 0..w * h {
            if comp[start] != usize::MAX || has_partial(start) {
                continue;
            }
            let value = self.cells[start];
            comp[start] = start;
            let mut members = vec![start];
            let mut next = 0;
            while next < members.len() {
                let c = members[next];
                next += 1;
                let (x, y) = (c % w, c / w);
                for (nx, ny) in neighbors(x, y, w, h, false) {
                    let n = ny * w + nx;
                    if comp[n] == usize::MAX && self.cells[n] == value && !has_partial(n) {
                        comp[n] = start;
                        members.push(n);
                    }
                }
            }
            if members.len() >= min_size {
                continue;
            }
            let mut counts: Vec<(Option<u16>, usize)> = Vec::new();
            for &c in &members {
                for (nx, ny) in neighbors(c % w, c / w, w, h, true) {
                    let n = ny * w + nx;
                    if comp[n] == start || has_partial(n) {
                        continue;
                    }
                    match counts.iter_mut().find(|(v, _)| *v == self.cells[n]) {
                        Some((_, k)) => *k += 1,
                        None => counts.push((self.cells[n], 1)),
                    }
                }
            }
            let mut best: Option<(Option<u16>, usize)> = None;
            for (v, k) in counts {
                if best.is_none_or(|(_, bk)| k > bk) {
                    best = Some((v, k));
                }
            }
            if let Some((v, _)) = best {
                changes.extend(members.into_iter().map(|c| (c, v)));
            }
        }
        for &(c, v) in &changes {
            self.cells[c] = v;
        }
        changes.len()
    }

    /// Finished design size in inches (whole chart, including unstitched
    /// border squares).
    pub fn finished_size_in(&self) -> (f32, f32) {
        (
            self.width as f32 / self.fabric.count.max(0.01),
            self.height as f32 / self.fabric.rows_per_inch().max(0.01),
        )
    }

    /// The number a chart shows for row `y` - top-down for most crafts,
    /// bottom-up for knitting (row 1 is the first row worked).
    pub fn row_number(&self, y: usize) -> usize {
        if self.craft.numbers_from_bottom_right() {
            self.height - y
        } else {
            y + 1
        }
    }

    /// Whether the grid line before column `c` (0..=width) is a heavy
    /// counting line: every 10th, counted in the chart's own numbering
    /// direction (from the right for knitting), plus both edges.
    pub fn is_heavy_col_line(&self, c: usize) -> bool {
        let n = if self.craft.numbers_from_bottom_right() {
            self.width - c
        } else {
            c
        };
        n.is_multiple_of(10) || c == 0 || c == self.width
    }

    /// `is_heavy_col_line` for the line above row `r` (0..=height),
    /// counted from the bottom for knitting.
    pub fn is_heavy_row_line(&self, r: usize) -> bool {
        let n = if self.craft.numbers_from_bottom_right() {
            self.height - r
        } else {
            r
        };
        n.is_multiple_of(10) || r == 0 || r == self.height
    }

    /// The number a chart shows for column `x` - left to right for most
    /// crafts, right to left for knitting (stitch 1 is the first worked).
    pub fn col_number(&self, x: usize) -> usize {
        if self.craft.numbers_from_bottom_right() {
            self.width - x
        } else {
            x + 1
        }
    }

    /// Every point (in squares) that must fit when framing the design:
    /// stitched squares' corners, backstitch ends, knots.
    fn extent_points(&self) -> Vec<(f32, f32)> {
        let mut pts = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.is_stitched(x, y) {
                    let (fx, fy) = (x as f32, y as f32);
                    pts.extend([
                        (fx, fy),
                        (fx + 1.0, fy),
                        (fx, fy + 1.0),
                        (fx + 1.0, fy + 1.0),
                    ]);
                }
            }
        }
        let half = |(x, y): (u32, u32)| (x as f32 / 2.0, y as f32 / 2.0);
        for b in &self.backstitches {
            pts.push(half(b.from));
            pts.push(half(b.to));
        }
        pts.extend(self.knots.iter().map(|k| half(k.at)));
        pts
    }

    /// Whether everything stitched fits inside a round hoop of
    /// `diameter_in`, with `HOOP_MARGIN_IN` to spare, when centered in
    /// it. Checks actual points against the circle rather than just the
    /// bounding box, so a round-ish motif is allowed to be wider than a
    /// square one would be.
    pub fn fits_in_hoop(&self, diameter_in: f32) -> bool {
        let pts = self.extent_points();
        if pts.is_empty() {
            return true;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in &pts {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let (sx, sy) = (
            self.fabric.count.max(0.01),
            self.fabric.rows_per_inch().max(0.01),
        );
        let radius = (diameter_in / 2.0 - HOOP_MARGIN_IN).max(0.0);
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        pts.iter().all(|&(x, y)| {
            ((x - cx) / sx).powi(2) + ((y - cy) / sy).powi(2) <= radius * radius + 1e-6
        })
    }

    /// Smallest common hoop the design fits, if any.
    pub fn smallest_hoop(&self) -> Option<f32> {
        COMMON_HOOPS_IN
            .iter()
            .copied()
            .find(|&d| self.fits_in_hoop(d))
    }

    /// Fabric to cut: the design plus 3 inches on every side (room to
    /// hoop and to frame/finish), the usual rule of thumb.
    pub fn fabric_cut_size_in(&self) -> (f32, f32) {
        let (w, h) = self.finished_size_in();
        (w + 6.0, h + 6.0)
    }
}

fn neighbors(x: usize, y: usize, w: usize, h: usize, diagonals: bool) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(8);
    for dy in -1i64..=1 {
        for dx in -1i64..=1 {
            if (dx == 0 && dy == 0) || (!diagonals && dx != 0 && dy != 0) {
                continue;
            }
            let (nx, ny) = (x as i64 + dx, y as i64 + dy);
            if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                out.push((nx as usize, ny as usize));
            }
        }
    }
    out
}

/// Most common color along the grid's outer edge.
fn border_mode(grid: &ColorGrid) -> Option<[u8; 3]> {
    if grid.width == 0 || grid.height == 0 {
        return None;
    }
    let mut counts: Vec<([u8; 3], usize)> = Vec::new();
    let mut bump = |c: [u8; 3]| match counts.iter_mut().find(|(k, _)| *k == c) {
        Some((_, n)) => *n += 1,
        None => counts.push((c, 1)),
    };
    for x in 0..grid.width {
        bump(grid.get(x, 0));
        if grid.height > 1 {
            bump(grid.get(x, grid.height - 1));
        }
    }
    for y in 1..grid.height.saturating_sub(1) {
        bump(grid.get(0, y));
        if grid.width > 1 {
            bump(grid.get(grid.width - 1, y));
        }
    }
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(c, _)| c)
}

/// One skein is 8 m of six-strand floss = 48 m of single strand.
const SKEIN_STRAND_MM: f32 = 8000.0 * 6.0;
/// Starting/ending tails and waste allowance on top of the thread that
/// actually lies in the fabric.
const WASTE: f32 = 1.5;
/// Single-strand thread per stitch, in square-widths: what shows on the
/// front plus roughly the travel on the back.
const FULL_SQUARES: f32 = 2.0 * std::f32::consts::SQRT_2 + 2.0;
const HALF_SQUARES: f32 = std::f32::consts::SQRT_2 + 1.0;
const THREE_QUARTER_SQUARES: f32 = 1.5 * std::f32::consts::SQRT_2 + 1.5;
const QUARTER_SQUARES: f32 = std::f32::consts::FRAC_1_SQRT_2 + 0.5;
/// Backstitch uses about twice its visible length (front and back).
const BACKSTITCH_FACTOR: f32 = 2.0;
/// Wraps plus the pass through the fabric, per strand.
const KNOT_MM: f32 = 20.0;

fn thread_mm_for(u: &Usage, f: &Floss, count: f32) -> f32 {
    let square_mm = 25.4 / count.max(1.0);
    let crosses = u.full as f32 * FULL_SQUARES
        + u.half as f32 * HALF_SQUARES
        + u.three_quarter as f32 * THREE_QUARTER_SQUARES
        + u.quarter as f32 * QUARTER_SQUARES;
    let strands = f.strands.max(1) as f32;
    let bs_strands = f.bs_strands.max(1) as f32;
    WASTE
        * (crosses * square_mm * strands
            + u.backstitch_squares * BACKSTITCH_FACTOR * square_mm * bs_strands
            + u.knots as f32 * KNOT_MM * strands)
}

/// Estimated skeins (8 m of six-strand floss) for `stitches` full crosses
/// at `count` squares per inch with `strands` strands.
///
/// One cross is two diagonals plus roughly two squares of travel on the
/// back, about 4.83 square-widths of thread per strand; a 1.5x allowance
/// covers starting/ending tails and waste. That works out to roughly 1,800
/// crosses per skein at 14-count/2 strands, in line with the usual
/// published figure. An estimate - round up when shopping.
pub fn estimate_skeins(stitches: usize, count: f32, strands: u8) -> f32 {
    let u = Usage {
        full: stitches,
        ..Usage::default()
    };
    let f = Floss {
        strands,
        ..Floss::custom("", [0, 0, 0], 'X')
    };
    thread_mm_for(&u, &f, count) / SKEIN_STRAND_MM
}

/// Tapestry needle size typically used for a given fabric count.
pub fn recommended_needle(count: f32) -> u8 {
    match count {
        c if c < 12.5 => 22,
        c if c < 17.0 => 24,
        c if c < 20.0 => 26,
        _ => 28,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threads::find_dmc;

    fn floss(code: &str, symbol: char) -> Floss {
        Floss::from_thread(find_dmc(code).unwrap(), symbol)
    }

    #[test]
    fn symbols_are_unique() {
        for (i, a) in SYMBOLS.iter().enumerate() {
            assert!(!SYMBOLS[i + 1..].contains(a), "duplicate symbol {a}");
            assert!(a.is_ascii_graphic());
        }
    }

    #[test]
    fn add_floss_dedupes_threads_and_symbols() {
        let mut c = Chart::new(2, 2);
        assert_eq!(c.add_floss(floss("310", 'X')), 0);
        assert_eq!(c.add_floss(floss("666", 'X')), 1);
        assert_ne!(
            c.palette[1].symbol, 'X',
            "symbol collision should be reassigned"
        );
        assert_eq!(
            c.add_floss(floss("310", 'Q')),
            0,
            "same thread reuses entry"
        );
        let mut blended = floss("310", 'Q');
        blended.blend = Some(BlendThread::from_thread(find_dmc("3865").unwrap()));
        assert_eq!(c.add_floss(blended), 2, "a blend is a different entry");
        assert_eq!(c.palette.len(), 3);
    }

    #[test]
    fn remove_floss_unstitches_and_reindexes_everything() {
        let mut c = Chart::new(3, 2);
        c.add_floss(floss("310", 'X'));
        c.add_floss(floss("666", 'O'));
        c.add_floss(floss("700", '+'));
        c.cells[..3].copy_from_slice(&[Some(0), Some(1), Some(2)]);
        c.set_partial(
            0,
            1,
            Partial::Half {
                diagonal: Diagonal::Slash,
                floss: 1,
            },
        );
        c.set_partial(
            1,
            1,
            Partial::Split {
                diagonal: Diagonal::Backslash,
                first: Some(1),
                second: Some(2),
            },
        );
        c.backstitches.push(Backstitch {
            from: (0, 0),
            to: (2, 2),
            floss: 1,
        });
        c.backstitches.push(Backstitch {
            from: (0, 0),
            to: (2, 0),
            floss: 2,
        });
        c.knots.push(Knot {
            at: (1, 1),
            floss: 2,
        });
        c.remove_floss(1);
        assert_eq!(&c.cells[..3], &[Some(0), None, Some(1)]);
        assert_eq!(c.palette[1].code, "700");
        assert!(c.partial(0, 1).is_none(), "half in removed floss is gone");
        assert_eq!(
            c.partial(1, 1),
            Some(&Partial::Split {
                diagonal: Diagonal::Backslash,
                first: None,
                second: Some(1)
            })
        );
        assert_eq!(c.backstitches.len(), 1);
        assert_eq!(c.backstitches[0].floss, 1);
        assert_eq!(c.knots[0].floss, 1);
    }

    #[test]
    fn full_crosses_and_part_stitches_exclude_each_other() {
        let mut c = Chart::new(1, 1);
        c.add_floss(floss("310", 'X'));
        c.set(0, 0, Some(0));
        c.set_partial(
            0,
            0,
            Partial::Half {
                diagonal: Diagonal::Slash,
                floss: 0,
            },
        );
        assert_eq!(c.get(0, 0), None);
        c.set(0, 0, Some(0));
        assert!(c.partial(0, 0).is_none());
    }

    #[test]
    fn from_color_grid_snaps_to_catalog_and_can_skip_background() {
        let mut g = ColorGrid::new(4, 4, [255, 255, 255]);
        g.set(1, 1, [0, 0, 0]);
        g.set(2, 2, [2, 1, 3]); // near-black: merges with black
        let kept = Chart::from_color_grid(&g, Fabric::default(), false, Some(Catalog::Dmc));
        assert_eq!(kept.total_stitches(), 16);
        assert_eq!(kept.palette.len(), 2, "white + black, near-black merged");

        let skipped = Chart::from_color_grid(&g, Fabric::default(), true, Some(Catalog::Dmc));
        assert_eq!(skipped.total_stitches(), 2);
        assert_eq!(skipped.palette[0].code, "310");
        assert_eq!(skipped.get(0, 0), None);

        let anchor = Chart::from_color_grid(&g, Fabric::default(), true, Some(Catalog::Anchor));
        assert_eq!(anchor.palette[0].brand, "Anchor");

        let free = Chart::from_color_grid(&g, Fabric::default(), true, None);
        assert!(free.palette[0].is_custom());
        assert_eq!(free.palette[0].rgb, [0, 0, 0]);
        assert_eq!(free.palette[0].name, "Black #000000");
        assert_eq!(free.palette.len(), 2, "free colors don't merge near-black");
    }

    #[test]
    fn to_color_grid_shows_fabric_for_unstitched_and_blends_average() {
        let mut c = Chart::new(2, 1);
        c.fabric.rgb = [1, 2, 3];
        let mut f = floss("310", 'X');
        f.blend = Some(BlendThread::from_thread(find_dmc("B5200").unwrap()));
        c.add_floss(f);
        c.set(1, 0, Some(0));
        let g = c.to_color_grid();
        assert_eq!(g.get(0, 0), [1, 2, 3]);
        assert_eq!(g.get(1, 0), [127, 127, 127]);
    }

    #[test]
    fn confetti_is_absorbed_by_its_surroundings() {
        // 5x5 green with a lone black speck, a 2-square red speck, and a
        // single-square unstitched hole.
        let mut c = Chart::new(5, 5);
        c.add_floss(floss("702", 'O'));
        c.add_floss(floss("310", 'X'));
        c.add_floss(floss("666", '+'));
        c.cells = vec![Some(0); 25];
        c.set(1, 1, Some(1));
        c.set(3, 3, Some(2));
        c.set(3, 2, Some(2));
        c.set(1, 3, None);
        let mut singles = c.clone();
        assert_eq!(singles.remove_confetti(2), 2, "speck + hole");
        assert_eq!(singles.get(1, 1), Some(0));
        assert_eq!(singles.get(1, 3), Some(0));
        assert_eq!(singles.get(3, 3), Some(2), "2-square patch survives min 2");
        assert_eq!(c.remove_confetti(3), 4);
        assert!(c.cells.iter().all(|v| *v == Some(0)));
        assert_eq!(c.remove_confetti(1), 0, "1 = off");
        c.remove_unused_floss();
        assert_eq!(c.palette.len(), 1);
    }

    #[test]
    fn confetti_leaves_large_regions_and_part_stitches_alone() {
        let mut c = Chart::new(4, 1);
        c.add_floss(floss("702", 'O'));
        c.add_floss(floss("310", 'X'));
        c.cells = vec![Some(0), Some(0), Some(1), Some(1)];
        c.set_partial(
            0,
            0,
            Partial::Half {
                diagonal: Diagonal::Slash,
                floss: 1,
            },
        );
        // Square 1 is a lone green, but its only stitched neighbor that
        // counts is black (square 0 has a part stitch).
        assert_eq!(c.remove_confetti(2), 1);
        assert_eq!(c.get(1, 0), Some(1));
        assert!(c.partial(0, 0).is_some());
    }

    #[test]
    fn hoop_fit_uses_the_circle_not_the_bounding_box() {
        // 98x98 squares at 14ct = 7in square. Its diagonal (~9.9in) is far
        // too big for an 8in hoop, but a round motif of the same width
        // should fit a hoop 1in (2 x margin) wider than it.
        let n = 98;
        let mut square = Chart::new(n, n);
        square.add_floss(floss("310", 'X'));
        square.cells = vec![Some(0); n * n];
        assert!(!square.fits_in_hoop(8.0));
        assert!(square.fits_in_hoop(11.0));

        let mut disc = Chart::new(n, n);
        disc.add_floss(floss("310", 'X'));
        let r = n as f32 / 2.0;
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = (x as f32 + 0.5 - r, y as f32 + 0.5 - r);
                if dx * dx + dy * dy <= (r - 1.0) * (r - 1.0) {
                    disc.set(x, y, Some(0));
                }
            }
        }
        assert!(disc.fits_in_hoop(8.0));
        assert!(!disc.fits_in_hoop(7.0));
        assert_eq!(disc.smallest_hoop(), Some(8.0));
    }

    #[test]
    fn backstitch_and_knots_count_toward_hoop_fit() {
        let mut c = Chart::new(200, 1);
        c.add_floss(floss("310", 'X'));
        assert!(c.fits_in_hoop(4.0));
        c.backstitches.push(Backstitch {
            from: (0, 0),
            to: (400, 0),
            floss: 0,
        });
        assert!(
            !c.fits_in_hoop(12.0),
            "a 14in backstitch line can't fit a 12in hoop"
        );
        c.backstitches.clear();
        c.knots.push(Knot {
            at: (0, 0),
            floss: 0,
        });
        c.knots.push(Knot {
            at: (140, 0),
            floss: 0,
        });
        assert!(!c.fits_in_hoop(5.0));
        assert!(c.fits_in_hoop(7.0));
    }

    #[test]
    fn skein_estimate_is_about_1800_crosses_at_14_count_two_strands() {
        let per_skein = 1.0 / estimate_skeins(1, 14.0, 2);
        assert!((1600.0..2000.0).contains(&per_skein), "got {per_skein}");
        assert!(estimate_skeins(1000, 11.0, 2) > estimate_skeins(1000, 18.0, 2));
    }

    #[test]
    fn usage_and_shopping_list_cover_every_stitch_kind_and_blends() {
        let mut c = Chart::new(3, 1);
        let mut blend = floss("310", 'X');
        blend.blend = Some(BlendThread::from_thread(find_dmc("666").unwrap()));
        c.add_floss(blend);
        c.add_floss(floss("666", 'O'));
        c.set(0, 0, Some(0));
        c.set_partial(
            1,
            0,
            Partial::Half {
                diagonal: Diagonal::Slash,
                floss: 1,
            },
        );
        c.set_partial(2, 0, Partial::Quarters([Some(1), None, None, Some(0)]));
        c.backstitches.push(Backstitch {
            from: (0, 0),
            to: (6, 0),
            floss: 1,
        });
        c.knots.push(Knot {
            at: (1, 1),
            floss: 1,
        });
        let u = c.usage();
        assert_eq!(
            u[0],
            Usage {
                full: 1,
                quarter: 1,
                ..Usage::default()
            }
        );
        assert_eq!((u[1].half, u[1].quarter, u[1].knots), (1, 1, 1));
        assert!((u[1].backstitch_squares - 3.0).abs() < 1e-6);
        let list = c.shopping_list();
        assert_eq!(list.len(), 2, "DMC 666 alone and in the blend is one item");
        assert!(list.iter().all(|i| i.buy == 1 && i.unit == "skein"));
        assert!(
            list.iter().any(|i| i.label == "DMC 666 Red - Bright"),
            "{list:?}"
        );
    }

    #[test]
    fn other_crafts_buy_by_the_pack_with_spares() {
        let mut c = Chart::new_for(GridCraft::DiamondPainting);
        c.add_floss(floss("310", 'X'));
        c.add_floss(floss("666", 'O'));
        c.cells = vec![None; c.width * c.height];
        for i in 0..400 {
            c.cells[i] = Some(0); // 400 drills + 10% = 440 -> 3 bags of 200
        }
        c.cells[500] = Some(1);
        let list = c.shopping_list();
        assert_eq!((list[0].used, list[0].buy), (400, 3));
        assert_eq!(list[0].buy_text(), "3 bags of 200 drills");
        assert_eq!(list[1].buy_text(), "1 bag of 200 drills");

        let mut art = Chart::new_for(GridCraft::PixelArt);
        art.add_floss(Floss::free([10, 20, 30], 'X'));
        art.set(0, 0, Some(0));
        let list = art.shopping_list();
        assert_eq!(
            (list[0].used, list[0].buy, list[0].buy_text()),
            (1, 0, String::new())
        );
    }

    #[test]
    fn rematch_colors_merges_entries_that_land_on_the_same_thread() {
        let mut c = Chart::new(3, 1);
        c.add_floss(Floss::custom("black", [0, 0, 0], 'X'));
        c.add_floss(Floss::custom("near black", [3, 2, 1], 'O'));
        c.add_floss(Floss::custom("red", [200, 20, 30], '+'));
        c.cells = vec![Some(0), Some(1), Some(2)];
        c.knots.push(Knot {
            at: (1, 1),
            floss: 2,
        });
        c.rematch_colors(Some(Catalog::Hama));
        assert_eq!(c.palette.len(), 2);
        assert!(c.palette.iter().all(|f| f.brand == "Hama"));
        assert_eq!(c.cells[0], c.cells[1]);
        assert_ne!(c.cells[0], c.cells[2]);
        assert_eq!(c.knots[0].floss, c.cells[2].unwrap());
        assert_eq!(c.palette[0].symbol, 'X', "symbols kept");
        c.rematch_colors(None);
        assert!(c.palette.iter().all(|f| f.is_custom()));
    }

    #[test]
    fn knitting_gauge_numbering_and_yarn_shares() {
        let mut c = Chart::new_for(GridCraft::Knitting);
        c.crop_or_pad(22, 30);
        // DK: 22 sts x 30 rows per 4 in -> exactly 4 x 4 in.
        let (w, h) = c.finished_size_in();
        assert!(
            (w - 4.0).abs() < 1e-4 && (h - 4.0).abs() < 1e-4,
            "{w} x {h}"
        );
        assert!((c.fabric.cell_aspect() - 22.0 / 30.0).abs() < 1e-4);
        assert_eq!(
            (c.row_number(29), c.row_number(0)),
            (1, 30),
            "row 1 at the bottom"
        );
        assert_eq!(
            (c.col_number(21), c.col_number(0)),
            (1, 22),
            "stitch 1 at the right"
        );
        // Heavy lines after stitch 10 / row 10 counted from bottom-right.
        assert!(c.is_heavy_col_line(12) && !c.is_heavy_col_line(10));
        assert!(c.is_heavy_row_line(20) && c.is_heavy_row_line(10) && !c.is_heavy_row_line(15));
        assert!(c.is_heavy_col_line(0) && c.is_heavy_col_line(22));
        c.add_floss(Floss::free([200, 0, 0], 'X'));
        for x in 0..22 {
            c.set(x, 0, Some(0));
        }
        let list = c.shopping_list();
        assert_eq!(list[0].label, "Main color (background)");
        assert_eq!(list[0].buy_text(), "638 sts (97% of the yarn)");
        assert_eq!(list[1].buy_text(), "22 sts (3% of the yarn)");
    }

    #[test]
    fn quilt_sizes_are_not_clamped_to_one_cell_per_inch() {
        let c = Chart::new_for(GridCraft::Quilt);
        assert_eq!(c.finished_size_in(), (24.0, 32.0));
        assert_eq!((c.row_number(0), c.col_number(0)), (1, 1));
    }

    #[test]
    fn boards_needed_rounds_up() {
        let mut c = Chart::new_for(GridCraft::FuseBeads);
        assert_eq!(c.boards_needed(), Some((1, 1)));
        c.crop_or_pad(30, 58);
        assert_eq!(c.boards_needed(), Some((2, 2)));
        assert_eq!(Chart::new(3, 3).boards_needed(), None);
    }

    #[test]
    fn crop_or_pad_keeps_overlap_and_drops_out_of_bounds_extras() {
        let mut c = Chart::new(2, 2);
        c.add_floss(floss("310", 'X'));
        c.set(1, 1, Some(0));
        c.set_partial(
            0,
            1,
            Partial::Half {
                diagonal: Diagonal::Slash,
                floss: 0,
            },
        );
        c.backstitches.push(Backstitch {
            from: (0, 0),
            to: (4, 4),
            floss: 0,
        });
        c.knots.push(Knot {
            at: (1, 1),
            floss: 0,
        });
        c.crop_or_pad(3, 1);
        assert_eq!((c.width, c.height), (3, 1));
        assert_eq!(c.cells, vec![None, None, None]);
        assert!(c.partials.is_empty());
        assert!(c.backstitches.is_empty());
        assert_eq!(c.knots.len(), 1);
    }

    #[test]
    fn split_side_matches_the_oxs_left_right_convention() {
        // Backslash: first = bottom-left, second = top-right.
        assert_eq!(
            Partial::split_side(Corner::BottomLeft),
            (Diagonal::Backslash, true)
        );
        assert_eq!(
            Partial::split_side(Corner::TopRight),
            (Diagonal::Backslash, false)
        );
        // Slash: first = top-left, second = bottom-right.
        assert_eq!(
            Partial::split_side(Corner::TopLeft),
            (Diagonal::Slash, true)
        );
        assert_eq!(
            Partial::split_side(Corner::BottomRight),
            (Diagonal::Slash, false)
        );
        assert_eq!(Corner::nearest(0.9, 0.1), Corner::TopRight);
    }
}
