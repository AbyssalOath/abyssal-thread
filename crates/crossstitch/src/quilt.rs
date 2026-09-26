//! Quilt planning for a chart used as a pixel / half-square-triangle
//! quilt: every cell is one finished square patch, either a single fabric
//! or a half-square triangle (HST, `Partial::Split` - the same diagonal
//! split cross stitch uses for 3/4 stitches). Empty cells are the
//! background fabric.
//!
//! The math follows standard quilting practice:
//! - 1/4 in seam allowances, so a square is cut at finished + 1/2 in.
//! - HSTs made two at a time: one square of each fabric, cut at finished
//!   + 7/8 in, sewn 1/4 in either side of the diagonal and cut apart.
//! - Pieces are cut from strips across the fabric's width. Quilting
//!   cotton is sold ~42-44 in wide; `USABLE_WIDTH_IN` assumes 40 in after
//!   selvages and shrinkage, to stay on the safe side.
//! - Yardage gets a 10% allowance for straightening and mistakes, then is
//!   rounded up to the next 1/8 yard, the smallest cut most shops sell.
//! - Backing and batting extend 4 in past the quilt top on every side;
//!   binding is 2 1/2 in strips, the perimeter plus 12 in for joins and
//!   corners.

use crate::chart::{Chart, Diagonal, Partial, ShoppingItem};

pub const USABLE_WIDTH_IN: f32 = 40.0;
pub const SEAM_ALLOWANCE_IN: f32 = 0.25;
const HST_EXTRA_IN: f32 = 0.875;
const YARDAGE_ALLOWANCE: f32 = 1.10;
const BACKING_OVERHANG_IN: f32 = 4.0;
const BINDING_STRIP_IN: f32 = 2.5;
const BINDING_EXTRA_IN: f32 = 12.0;
const BACKGROUND_RGB_LABEL: &str = "Background fabric";

/// A fabric in the quilt: `None` = the background (empty cells), else a
/// palette index.
pub type FabricKey = Option<u16>;

#[derive(Debug, Clone, PartialEq)]
pub struct FabricNeed {
    pub key: FabricKey,
    pub code: String,
    pub label: String,
    pub rgb: [u8; 3],
    /// Plain squares, cut at `QuiltPlan::square_cut_in`.
    pub squares: usize,
    /// Squares for HSTs, cut at `QuiltPlan::hst_cut_in`.
    pub hst_squares: usize,
    /// Rounded up to 1/8 yd.
    pub yards: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuiltPlan {
    /// Finished quilt top, inches.
    pub finished_in: (f32, f32),
    pub square_in: f32,
    pub square_cut_in: f32,
    pub hst_cut_in: f32,
    pub fabrics: Vec<FabricNeed>,
    /// HST units needed per unordered fabric pair.
    pub hst_pairs: Vec<((FabricKey, FabricKey), usize)>,
    pub backing_panels: usize,
    pub backing_panel_len_in: f32,
    pub backing_yards: f32,
    pub binding_strips: usize,
    pub binding_yards: f32,
    pub batting_in: (f32, f32),
    /// Cells holding part stitches other than HSTs (only possible after
    /// switching a cross-stitch chart to quilt) - treated as background.
    pub ignored_cells: usize,
}

/// Rounds up to the next 1/8.
fn ceil_eighth(v: f32) -> f32 {
    (v * 8.0 - 1e-4).ceil().max(0.0) / 8.0
}

/// "2 7/8", "5/8", "3" - inches or yards to the nearest 1/8.
pub fn fmt_eighths(v: f32) -> String {
    let eighths = (v * 8.0).round() as i64;
    let (whole, rem) = (eighths / 8, eighths % 8);
    let frac = match rem {
        0 => "",
        1 => "1/8",
        2 => "1/4",
        3 => "3/8",
        4 => "1/2",
        5 => "5/8",
        6 => "3/4",
        _ => "7/8",
    };
    match (whole, frac) {
        (0, "") => "0".to_string(),
        (0, f) => f.to_string(),
        (w, "") => w.to_string(),
        (w, f) => format!("{w} {f}"),
    }
}

/// Inches of fabric length needed to cut `n` squares of side `cut`, from
/// strips `cut` wide across the usable width.
fn strip_length_in(n: usize, cut: f32) -> f32 {
    if n == 0 {
        return 0.0;
    }
    let per_strip = ((USABLE_WIDTH_IN / cut).floor() as usize).max(1);
    n.div_ceil(per_strip) as f32 * cut
}

fn yards_for(length_in: f32) -> f32 {
    if length_in <= 0.0 {
        return 0.0;
    }
    ceil_eighth(length_in * YARDAGE_ALLOWANCE / 36.0).max(0.125)
}

/// Short code for a fabric in cutting lists and assembly rows: `BG` for
/// the background, else the palette entry's chart symbol.
pub fn fabric_code(chart: &Chart, key: FabricKey) -> String {
    match key.and_then(|i| chart.palette.get(i as usize)) {
        Some(f) => f.symbol.to_string(),
        None => "BG".to_string(),
    }
}

pub fn plan(chart: &Chart) -> QuiltPlan {
    let square_in = 1.0 / chart.fabric.count.max(0.01);
    let square_cut_in = square_in + 2.0 * SEAM_ALLOWANCE_IN;
    let hst_cut_in = square_in + HST_EXTRA_IN;

    let mut squares: Vec<(FabricKey, usize)> = Vec::new();
    let bump = |list: &mut Vec<(FabricKey, usize)>, k: FabricKey, n: usize| match list
        .iter_mut()
        .find(|(key, _)| *key == k)
    {
        Some((_, c)) => *c += n,
        None => list.push((k, n)),
    };
    let mut pairs: Vec<((FabricKey, FabricKey), usize)> = Vec::new();
    let mut ignored = 0;
    for y in 0..chart.height {
        for x in 0..chart.width {
            match chart.partial(x, y) {
                Some(Partial::Split { first, second, .. }) if first != second => {
                    let key = if first <= second {
                        (*first, *second)
                    } else {
                        (*second, *first)
                    };
                    match pairs.iter_mut().find(|(k, _)| *k == key) {
                        Some((_, n)) => *n += 1,
                        None => pairs.push((key, 1)),
                    }
                }
                Some(Partial::Split { first, .. }) => bump(&mut squares, *first, 1),
                Some(_) => {
                    ignored += 1;
                    bump(&mut squares, None, 1);
                }
                None => bump(&mut squares, chart.get(x, y), 1),
            }
        }
    }
    let mut hst_squares: Vec<(FabricKey, usize)> = Vec::new();
    for &((a, b), n) in &pairs {
        // Two at a time: each pair of cut squares makes two units.
        let cut = n.div_ceil(2);
        bump(&mut hst_squares, a, cut);
        bump(&mut hst_squares, b, cut);
    }

    // Background first, then palette order.
    let mut keys: Vec<FabricKey> = squares
        .iter()
        .chain(&hst_squares)
        .map(|(k, _)| *k)
        .collect();
    keys.sort_by_key(|k| k.map_or(-1, |i| i as i64));
    keys.dedup();
    let count_of = |list: &[(FabricKey, usize)], k: FabricKey| {
        list.iter()
            .find(|(key, _)| *key == k)
            .map_or(0, |(_, n)| *n)
    };
    let fabrics = keys
        .into_iter()
        .map(|key| {
            let (label, rgb) = match key.and_then(|i| chart.palette.get(i as usize)) {
                Some(f) => (f.label(), f.display_rgb()),
                None => (BACKGROUND_RGB_LABEL.to_string(), chart.fabric.rgb),
            };
            let (sq, hst) = (count_of(&squares, key), count_of(&hst_squares, key));
            let length = strip_length_in(sq, square_cut_in) + strip_length_in(hst, hst_cut_in);
            FabricNeed {
                key,
                code: fabric_code(chart, key),
                label,
                rgb,
                squares: sq,
                hst_squares: hst,
                yards: yards_for(length),
            }
        })
        .collect();

    let (w, h) = chart.finished_size_in();
    let (bw, bh) = (w + 2.0 * BACKING_OVERHANG_IN, h + 2.0 * BACKING_OVERHANG_IN);
    // Seam backing panels side by side along whichever direction needs
    // less fabric.
    let across = ((bw / USABLE_WIDTH_IN).ceil() as usize).max(1);
    let down = ((bh / USABLE_WIDTH_IN).ceil() as usize).max(1);
    let (backing_panels, backing_panel_len_in) = if across as f32 * bh <= down as f32 * bw {
        (across, bh)
    } else {
        (down, bw)
    };
    let perimeter = 2.0 * (w + h) + BINDING_EXTRA_IN;
    let binding_strips = ((perimeter / USABLE_WIDTH_IN).ceil() as usize).max(1);

    QuiltPlan {
        finished_in: (w, h),
        square_in,
        square_cut_in,
        hst_cut_in,
        fabrics,
        hst_pairs: pairs,
        backing_panels,
        backing_panel_len_in,
        backing_yards: ceil_eighth(backing_panels as f32 * backing_panel_len_in / 36.0),
        binding_strips,
        binding_yards: ceil_eighth(binding_strips as f32 * BINDING_STRIP_IN / 36.0),
        batting_in: (bw, bh),
        ignored_cells: ignored,
    }
}

impl QuiltPlan {
    pub fn shopping_list(&self) -> Vec<ShoppingItem> {
        let mut items: Vec<ShoppingItem> = self
            .fabrics
            .iter()
            .map(|f| ShoppingItem {
                label: format!("{} ({})", f.label, f.code),
                rgb: f.rgb,
                used: f.squares + f.hst_squares,
                buy: 0,
                unit: "",
                note: format!("{} yd", fmt_eighths(f.yards)),
            })
            .collect();
        let gray = [150, 150, 150];
        items.push(ShoppingItem {
            label: "Backing".to_string(),
            rgb: gray,
            used: 0,
            buy: 0,
            unit: "",
            note: format!(
                "{} yd ({} panel{} of {} in)",
                fmt_eighths(self.backing_yards),
                self.backing_panels,
                if self.backing_panels == 1 { "" } else { "s" },
                fmt_eighths(self.backing_panel_len_in)
            ),
        });
        items.push(ShoppingItem {
            label: "Binding".to_string(),
            rgb: gray,
            used: 0,
            buy: 0,
            unit: "",
            note: format!(
                "{} yd ({} strips of {} in)",
                fmt_eighths(self.binding_yards),
                self.binding_strips,
                fmt_eighths(BINDING_STRIP_IN)
            ),
        });
        items.push(ShoppingItem {
            label: "Batting".to_string(),
            rgb: [240, 240, 230],
            used: 0,
            buy: 0,
            unit: "",
            note: format!(
                "{} x {} in",
                fmt_eighths(self.batting_in.0),
                fmt_eighths(self.batting_in.1)
            ),
        });
        items
    }
}

/// Cutting list, HST piecing and row-by-row assembly, as text.
pub fn instructions(chart: &Chart) -> String {
    let p = plan(chart);
    let mut out = String::new();
    out.push_str(&format!(
        "Finished quilt top: {} x {} in, {} x {} squares of {} in finished.\n",
        fmt_eighths(p.finished_in.0),
        fmt_eighths(p.finished_in.1),
        chart.width,
        chart.height,
        fmt_eighths(p.square_in)
    ));
    out.push_str(&format!(
        "All seams 1/4 in. Cut strips across the fabric width (assumes {USABLE_WIDTH_IN} in usable).\n\n"
    ));
    out.push_str("Cutting\n");
    for f in &p.fabrics {
        let mut parts = Vec::new();
        if f.squares > 0 {
            parts.push(format!(
                "{} squares {} in",
                f.squares,
                fmt_eighths(p.square_cut_in)
            ));
        }
        if f.hst_squares > 0 {
            parts.push(format!(
                "{} squares {} in (for HSTs)",
                f.hst_squares,
                fmt_eighths(p.hst_cut_in)
            ));
        }
        out.push_str(&format!(
            "  {:<3} {}: {}\n",
            f.code,
            f.label,
            parts.join("; ")
        ));
    }
    if !p.hst_pairs.is_empty() {
        out.push_str(
            "\nHalf-square triangles, two at a time: place one square of each fabric right sides together,\n\
             draw a diagonal, sew 1/4 in on both sides of it, cut on the line, press, and trim to ",
        );
        out.push_str(&format!("{} in.\n", fmt_eighths(p.square_cut_in)));
        for ((a, b), n) in &p.hst_pairs {
            out.push_str(&format!(
                "  {} + {}: {n} unit{} ({} square{} of each)\n",
                fabric_code(chart, *a),
                fabric_code(chart, *b),
                if *n == 1 { "" } else { "s" },
                n.div_ceil(2),
                if n.div_ceil(2) == 1 { "" } else { "s" },
            ));
        }
    }
    if p.ignored_cells > 0 {
        out.push_str(&format!(
            "\nNote: {} square(s) hold cross-stitch part stitches, which aren't quilt patches - counted as background.\n",
            p.ignored_cells
        ));
    }
    out.push_str(
        "\nAssembly - sew each row left to right, press, then join the rows top to bottom.\n",
    );
    out.push_str("A/B = HST split bottom-left to top-right (A top-left, B bottom-right);\n");
    out.push_str("A\\B = HST split top-left to bottom-right (A bottom-left, B top-right).\n");
    if let Some((bw, bh)) = chart.board {
        out.push_str(&format!(
            "Blocks are {bw} x {bh} squares: sew each block first, then join blocks into rows.\n"
        ));
    }
    for y in 0..chart.height {
        let cells: Vec<String> = (0..chart.width)
            .map(|x| match chart.partial(x, y) {
                Some(Partial::Split {
                    diagonal,
                    first,
                    second,
                }) if first != second => format!(
                    "{}{}{}",
                    fabric_code(chart, *first),
                    if *diagonal == Diagonal::Slash {
                        '/'
                    } else {
                        '\\'
                    },
                    fabric_code(chart, *second)
                ),
                Some(Partial::Split { first, .. }) => fabric_code(chart, *first),
                Some(_) => fabric_code(chart, None),
                None => fabric_code(chart, chart.get(x, y)),
            })
            .collect();
        out.push_str(&format!("Row {}: {}\n", y + 1, cells.join(" ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::Floss;
    use crate::profile::GridCraft;

    fn quilt() -> Chart {
        // 4 x 2 quilt of 2in squares: background, two fabrics, HSTs.
        let mut c = Chart::new_for(GridCraft::Quilt);
        c.crop_or_pad(4, 2);
        c.add_floss(Floss::free([200, 0, 0], 'X'));
        c.add_floss(Floss::free([0, 0, 200], 'O'));
        c.set(0, 0, Some(0));
        c.set(1, 0, Some(0));
        c.set(2, 0, Some(1));
        c.set_partial(
            0,
            1,
            Partial::Split {
                diagonal: Diagonal::Slash,
                first: Some(0),
                second: Some(1),
            },
        );
        c.set_partial(
            1,
            1,
            Partial::Split {
                diagonal: Diagonal::Backslash,
                first: Some(1),
                second: Some(0),
            },
        );
        c.set_partial(
            2,
            1,
            Partial::Split {
                diagonal: Diagonal::Slash,
                first: Some(0),
                second: None,
            },
        );
        c
    }

    #[test]
    fn eighths_format_like_a_quilting_ruler() {
        assert_eq!(fmt_eighths(2.875), "2 7/8");
        assert_eq!(fmt_eighths(0.625), "5/8");
        assert_eq!(fmt_eighths(3.0), "3");
        assert_eq!(fmt_eighths(2.5), "2 1/2");
        assert_eq!(ceil_eighth(0.51), 0.625);
        assert_eq!(ceil_eighth(0.5), 0.5);
    }

    #[test]
    fn counts_squares_and_pairs_hsts_two_at_a_time() {
        let p = plan(&quilt());
        assert_eq!(
            (p.square_in, p.square_cut_in, p.hst_cut_in),
            (2.0, 2.5, 2.875)
        );
        assert_eq!(p.finished_in, (8.0, 4.0));
        // X/O twice (either orientation) = one pair with 2 units; X/BG once.
        assert_eq!(p.hst_pairs.len(), 2);
        let xo = p
            .hst_pairs
            .iter()
            .find(|((a, b), _)| *a == Some(0) && *b == Some(1))
            .unwrap();
        assert_eq!(xo.1, 2);
        let fabric = |k: FabricKey| p.fabrics.iter().find(|f| f.key == k).unwrap();
        // Background: squares at (3,0) and (3,1); 1 HST square for X/BG.
        assert_eq!((fabric(None).squares, fabric(None).hst_squares), (2, 1));
        // X: 2 plain; HST squares: 1 (X/O pair, 2 units) + 1 (X/BG) = 2.
        assert_eq!(
            (fabric(Some(0)).squares, fabric(Some(0)).hst_squares),
            (2, 2)
        );
        assert_eq!(
            (fabric(Some(1)).squares, fabric(Some(1)).hst_squares),
            (1, 1)
        );
        assert_eq!(fabric(None).code, "BG");
        assert!(p.fabrics.iter().all(|f| f.yards >= 0.125));
    }

    #[test]
    fn yardage_uses_strips_across_the_fabric_width() {
        // 2.5in cut squares: 16 per 40in strip. 17 squares need 2 strips
        // = 5in; +10% = 5.5in = 0.153 yd -> 1/4 yd.
        assert_eq!(strip_length_in(17, 2.5), 5.0);
        assert_eq!(yards_for(5.0), 0.25);
        assert_eq!(yards_for(0.0), 0.0);
    }

    #[test]
    fn backing_binding_and_batting_for_a_crib_quilt() {
        let c = Chart::new_for(GridCraft::Quilt); // 24 x 32 in
        let p = plan(&c);
        assert_eq!(p.batting_in, (32.0, 40.0));
        // The 40in side runs across the 40in usable width, so one 32in
        // length does it (cheaper than 40in the other way) -> 1 yd.
        assert_eq!((p.backing_panels, p.backing_panel_len_in), (1, 32.0));
        assert_eq!(p.backing_yards, 1.0);
        // Perimeter 112 + 12 = 124in -> 4 strips of 2.5in = 10in -> 3/8 yd.
        assert_eq!((p.binding_strips, p.binding_yards), (4, 0.375));
        let list = p.shopping_list();
        assert_eq!(list.last().unwrap().buy_text(), "32 x 40 in");
        assert!(
            list.iter()
                .any(|i| i.buy_text() == "1 yd (1 panel of 32 in)"),
            "{list:?}"
        );
    }

    #[test]
    fn instructions_list_cutting_hsts_and_rows() {
        let text = instructions(&quilt());
        assert!(
            text.contains("X   X #c80000: 2 squares 2 1/2 in; 2 squares 2 7/8 in (for HSTs)")
                || text.contains("2 squares 2 1/2 in; 2 squares 2 7/8 in (for HSTs)"),
            "{text}"
        );
        assert!(text.contains("X + O: 2 units (1 square of each)"), "{text}");
        assert!(text.contains("Row 1: X X O BG"), "{text}");
        assert!(text.contains("Row 2: X/O O\\X X/BG BG"), "{text}");
    }
}
