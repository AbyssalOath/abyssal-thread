//! Chart SVG and plain-text floss/materials list.

use crate::chart::{recommended_needle, Chart, Corner, Diagonal, Partial, HOOP_MARGIN_IN};

const CELL: f32 = 14.0;

/// Black or white, whichever reads better on `rgb` - for drawing a symbol
/// on top of its own floss color.
pub fn contrasting_ink(rgb: [u8; 3]) -> [u8; 3] {
    let luma = 0.299 * rgb[0] as f32 + 0.587 * rgb[1] as f32 + 0.114 * rgb[2] as f32;
    if luma < 128.0 {
        [255, 255, 255]
    } else {
        [0, 0, 0]
    }
}

/// Corner points (in squares, relative to the square's top-left) of the
/// triangle `first`/`second` of a split along `diagonal` - see
/// `Partial::Split`.
pub fn split_triangle(diagonal: Diagonal, first: bool) -> [(f32, f32); 3] {
    match (diagonal, first) {
        (Diagonal::Backslash, true) => [(0.0, 0.0), (0.0, 1.0), (1.0, 1.0)],
        (Diagonal::Backslash, false) => [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)],
        (Diagonal::Slash, true) => [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)],
        (Diagonal::Slash, false) => [(1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
    }
}

/// Endpoints (in squares, relative to the square's top-left) of a
/// diagonal.
pub fn diagonal_ends(diagonal: Diagonal) -> [(f32, f32); 2] {
    match diagonal {
        Diagonal::Slash => [(0.0, 1.0), (1.0, 0.0)],
        Diagonal::Backslash => [(0.0, 0.0), (1.0, 1.0)],
    }
}

/// `diagonal_ends` pulled in slightly from the corners, for drawing a
/// half stitch as a thick bar that stays inside its own square.
pub fn half_stitch_ends(diagonal: Diagonal) -> [(f32, f32); 2] {
    const INSET: f32 = 0.14;
    diagonal_ends(diagonal).map(|(x, y)| (x + (0.5 - x) * INSET * 2.0, y + (0.5 - y) * INSET * 2.0))
}

/// Top-left of `corner`'s quarter of a square (in squares).
pub fn quarter_origin(corner: Corner) -> (f32, f32) {
    (
        if corner.is_right() { 0.5 } else { 0.0 },
        if corner.is_bottom() { 0.5 } else { 0.0 },
    )
}

fn svg_escape(c: char) -> String {
    match c {
        '&' => "&amp;".to_string(),
        '<' => "&lt;".to_string(),
        '>' => "&gt;".to_string(),
        c => c.to_string(),
    }
}

fn rgb_attr(c: [u8; 3]) -> String {
    format!("rgb({},{},{})", c[0], c[1], c[2])
}

/// Symbol chart: each stitched square filled with its floss color and
/// marked with its symbol, unstitched squares left fabric-colored, part
/// stitches drawn as triangles/diagonals/quarters, backstitch as lines and
/// knots as dots on top, a heavier line every 10 squares (the standard
/// counting aid), and a center mark on each edge.
pub fn export_chart_svg(chart: &Chart) -> String {
    // Knit stitches are wider than tall: cells are `cw` x `ch`.
    let (cw, ch) = (CELL, CELL * chart.fabric.cell_aspect());
    let (w, h) = (chart.width as f32 * cw, chart.height as f32 * ch);
    let mut body = format!(
        r#"<rect x="0" y="0" width="{w}" height="{h}" fill="{}"/>"#,
        rgb_attr(chart.fabric.rgb)
    );
    body.push('\n');
    let floss = |i: u16| chart.palette.get(i as usize);
    let text = |body: &mut String, x: f32, y: f32, size: f32, f: &crate::Floss| {
        body.push_str(&format!(
            r#"<text x="{x}" y="{}" font-size="{size}" fill="{}">{}</text>"#,
            y + size * 0.35,
            rgb_attr(contrasting_ink(f.display_rgb())),
            svg_escape(f.symbol)
        ));
    };
    for y in 0..chart.height {
        for x in 0..chart.width {
            let Some(f) = chart.get(x, y).and_then(floss) else {
                continue;
            };
            let (sx, sy) = (x as f32 * cw, y as f32 * ch);
            body.push_str(&format!(
                r#"<rect x="{sx}" y="{sy}" width="{cw}" height="{ch}" fill="{}"/>"#,
                rgb_attr(f.display_rgb())
            ));
            text(&mut body, sx + cw / 2.0, sy + ch / 2.0, cw.min(ch) * 0.8, f);
            body.push('\n');
        }
    }
    for (&(y, x), p) in &chart.partials {
        let (sx, sy) = (x as f32 * cw, y as f32 * ch);
        match p {
            Partial::Half { diagonal, floss: i } => {
                let Some(f) = floss(*i) else { continue };
                let [(x0, y0), (x1, y1)] = half_stitch_ends(*diagonal);
                body.push_str(&format!(
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}" stroke-linecap="round"/>"#,
                    sx + x0 * cw,
                    sy + y0 * ch,
                    sx + x1 * cw,
                    sy + y1 * ch,
                    rgb_attr(f.display_rgb()),
                    cw.min(ch) * 0.3
                ));
            }
            Partial::Split {
                diagonal,
                first,
                second,
            } => {
                for (side, v) in [(true, first), (false, second)] {
                    let Some(f) = v.and_then(floss) else { continue };
                    let pts = split_triangle(*diagonal, side);
                    let path: Vec<String> = pts
                        .iter()
                        .map(|(px, py)| format!("{},{}", sx + px * cw, sy + py * ch))
                        .collect();
                    body.push_str(&format!(
                        r#"<polygon points="{}" fill="{}"/>"#,
                        path.join(" "),
                        rgb_attr(f.display_rgb())
                    ));
                    let cx = sx + cw * pts.iter().map(|p| p.0).sum::<f32>() / 3.0;
                    let cy = sy + ch * pts.iter().map(|p| p.1).sum::<f32>() / 3.0;
                    text(&mut body, cx, cy, cw.min(ch) * 0.45, f);
                }
            }
            Partial::Quarters(q) => {
                for corner in Corner::ALL {
                    let Some(f) = q[corner as usize].and_then(floss) else {
                        continue;
                    };
                    let (ox, oy) = quarter_origin(corner);
                    body.push_str(&format!(
                        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
                        sx + ox * cw,
                        sy + oy * ch,
                        cw / 2.0,
                        ch / 2.0,
                        rgb_attr(f.display_rgb())
                    ));
                }
            }
        }
        body.push('\n');
    }
    for col in 0..=chart.width {
        let x = col as f32 * cw;
        let sw = if chart.is_heavy_col_line(col) {
            1.2
        } else {
            0.4
        };
        body.push_str(&format!(r##"<line x1="{x}" y1="0" x2="{x}" y2="{h}" stroke="#000" stroke-opacity="0.6" stroke-width="{sw}"/>"##));
    }
    for row in 0..=chart.height {
        let y = row as f32 * ch;
        let sw = if chart.is_heavy_row_line(row) {
            1.2
        } else {
            0.4
        };
        body.push_str(&format!(r##"<line x1="0" y1="{y}" x2="{w}" y2="{y}" stroke="#000" stroke-opacity="0.6" stroke-width="{sw}"/>"##));
    }
    body.push('\n');
    let px = |v: u32| v as f32 * cw / 2.0;
    let py = |v: u32| v as f32 * ch / 2.0;
    for b in &chart.backstitches {
        let Some(f) = floss(b.floss) else { continue };
        body.push_str(&format!(
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}" stroke-linecap="round"/>"#,
            px(b.from.0),
            py(b.from.1),
            px(b.to.0),
            py(b.to.1),
            rgb_attr(f.display_rgb()),
            CELL * 0.18
        ));
        body.push('\n');
    }
    for k in &chart.knots {
        let Some(f) = floss(k.floss) else { continue };
        body.push_str(&format!(
            r##"<circle cx="{}" cy="{}" r="{}" fill="{}" stroke="#000" stroke-width="0.8"/>"##,
            px(k.at.0),
            py(k.at.1),
            CELL * 0.25,
            rgb_attr(f.display_rgb())
        ));
        body.push('\n');
    }
    let (cx, cy) = (w / 2.0, h / 2.0);
    body.push_str(&format!(
        r##"<path d="M{cx} -2 l-5 -8 h10 z M{cx} {hb} l-5 8 h10 z M-2 {cy} l-8 -5 v10 z M{wr} {cy} l8 -5 v10 z" fill="#c00"/>"##,
        hb = h + 2.0,
        wr = w + 2.0,
    ));
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-12 -12 {vw} {vh}" font-family="sans-serif" font-size="{fs}" text-anchor="middle">
{body}
</svg>
"#,
        vw = w + 24.0,
        vh = h + 24.0,
        fs = CELL * 0.8,
    )
}

/// What each palette entry is used for, e.g. "812 full, 14 3/4, 6.5 sq
/// backstitch, 3 knots" - empty kinds omitted.
pub fn usage_summary(u: &crate::chart::Usage) -> String {
    let mut parts = Vec::new();
    for (n, what) in [
        (u.full, "full"),
        (u.three_quarter, "3/4"),
        (u.half, "half"),
        (u.quarter, "1/4"),
    ] {
        if n > 0 {
            parts.push(format!("{n} {what}"));
        }
    }
    if u.backstitch_squares > 0.0 {
        parts.push(format!("{:.1} sq backstitch", u.backstitch_squares));
    }
    if u.knots > 0 {
        parts.push(format!(
            "{} knot{}",
            u.knots,
            if u.knots == 1 { "" } else { "s" }
        ));
    }
    if parts.is_empty() {
        "unused".to_string()
    } else {
        parts.join(", ")
    }
}

/// The design/material summary lines shared by the materials text, the
/// PDF cover and the Materials tab - worded for the chart's craft.
pub fn info_lines(chart: &Chart) -> Vec<String> {
    let craft = chart.craft;
    let (_, cells) = craft.cell_word();
    let count = chart.fabric.count;
    let (w_in, h_in) = chart.finished_size_in();
    let mut lines = Vec::new();
    if craft.has_part_stitches() {
        lines.push(format!(
            "Design: {} x {} squares - {} full stitches, {} part-stitch squares, {} backstitch lines, {} French knots",
            chart.width,
            chart.height,
            chart.total_stitches(),
            chart.partials.len(),
            chart.backstitches.len(),
            chart.knots.len()
        ));
        lines.push(format!(
            "Fabric: {count}-count Aida, #{:02x}{:02x}{:02x}",
            chart.fabric.rgb[0], chart.fabric.rgb[1], chart.fabric.rgb[2]
        ));
    } else {
        lines.push(format!(
            "Design: {} x {} grid - {} {cells} in {} colors",
            chart.width,
            chart.height,
            chart.total_stitches(),
            chart.palette.len()
        ));
        let size = match craft.size_label_xy(count, chart.fabric.count_y) {
            Some(label) => label.to_string(),
            None if craft.has_row_gauge() => format!(
                "{:.1} sts x {:.1} rows per 4 in",
                count * 4.0,
                chart.fabric.rows_per_inch() * 4.0
            ),
            None if craft == crate::GridCraft::Quilt => {
                format!(
                    "{} in finished squares",
                    crate::quilt::fmt_eighths(1.0 / count.max(0.01))
                )
            }
            None => format!("{count:.2} {cells} per inch"),
        };
        // "3.75 holes/in rug canvas" already says what it's on.
        if size.contains(craft.surface()) {
            lines.push(format!("Size: {size}"));
        } else {
            lines.push(format!("Size: {size}, on {}", craft.surface()));
        }
    }
    lines.push(format!(
        "Finished size: {w_in:.1} x {h_in:.1} in ({:.1} x {:.1} cm)",
        w_in * 2.54,
        h_in * 2.54
    ));
    if craft.has_row_gauge() {
        lines.push(
            if chart.worked_in_round {
                "Worked in the round: read every round right to left, starting at the bottom."
            } else {
                "Worked flat: RS rows right to left, WS rows left to right, starting at the bottom."
            }
            .to_string(),
        );
        lines.push("Gauge matters - knit and measure a swatch before starting.".to_string());
    }
    if craft == crate::GridCraft::Quilt {
        lines.push(format!(
            "Seams 1/4 in; yardage assumes {} in of usable fabric width, plus 10%.",
            crate::quilt::USABLE_WIDTH_IN
        ));
    }
    if craft.has_hoop() {
        let (cut_w, cut_h) = chart.fabric_cut_size_in();
        lines.push(format!("Cut fabric at least: {cut_w:.0} x {cut_h:.0} in"));
        lines.push(match chart.smallest_hoop() {
            Some(d) => format!("Smallest hoop it fits (with {HOOP_MARGIN_IN}in margin): {d}in"),
            None => "Too large for a 12in hoop - use a scroll frame or Q-snaps".to_string(),
        });
        lines.push(format!(
            "Needle: size {} tapestry",
            recommended_needle(count)
        ));
    }
    if let (Some((bw, bh)), Some((nx, ny))) = (chart.board, chart.boards_needed()) {
        let n = nx * ny;
        lines.push(format!(
            "{}: {n} {}{} of {bw} x {bh} ({nx} across x {ny} down)",
            capitalize(craft.board_word()),
            craft.board_word(),
            if n == 1 { "" } else { "s" },
        ));
    }
    if let Some(p) = craft.packaging(count) {
        let spare = if p.spare > 0.0 {
            format!(", including {:.0}% spare", p.spare * 100.0)
        } else {
            String::new()
        };
        lines.push(format!(
            "Amounts assume one {}{spare} - pack sizes vary by seller.",
            p.unit
        ));
    }
    lines
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// Craft-specific written instructions: knitting's row-by-row
/// directions with float notes, a quilt's cutting list and assembly.
pub fn instructions_text(chart: &Chart) -> Option<String> {
    match chart.craft {
        crate::GridCraft::Knitting => Some(crate::knit::instructions(
            chart,
            crate::knit::DEFAULT_MAX_FLOAT,
        )),
        crate::GridCraft::Quilt => Some(crate::quilt::instructions(chart)),
        _ => None,
    }
}

/// Human-readable color key + shopping list.
pub fn materials_text(chart: &Chart) -> String {
    let mut out = String::new();
    let name = chart.name.as_deref().unwrap_or("Untitled");
    let craft = chart.craft;
    out.push_str(&format!(
        "{name} - {} materials\n\n",
        craft.label().to_lowercase()
    ));
    for line in info_lines(chart) {
        out.push_str(&line);
        out.push('\n');
    }
    out.push('\n');
    if craft.has_part_stitches() {
        out.push_str(
            "Key\nSym  Floss                                              Strands  Used for\n",
        );
        for (f, u) in chart.palette.iter().zip(chart.usage()) {
            let strands = if f.blend.is_some() {
                format!("{} each", f.strands)
            } else {
                f.strands.to_string()
            };
            let bs = if u.backstitch_squares > 0.0 {
                format!(
                    " (backstitch: {} strand{})",
                    f.bs_strands,
                    if f.bs_strands == 1 { "" } else { "s" }
                )
            } else {
                String::new()
            };
            out.push_str(&format!(
                "{:<4} {:<50} {:>7}  {}{bs}\n",
                f.symbol,
                f.label(),
                strands,
                usage_summary(&u)
            ));
        }
    } else {
        let (_, cells) = craft.cell_word();
        out.push_str(&format!(
            "Key\nSym  Color                                              {cells}\n"
        ));
        for (f, u) in chart.palette.iter().zip(chart.usage()) {
            out.push_str(&format!(
                "{:<4} {:<50} {:>6}\n",
                f.symbol,
                f.label(),
                u.full
            ));
        }
    }
    out.push_str("\nShopping list\n");
    let list = chart.shopping_list();
    let mut total = 0;
    for item in &list {
        total += item.buy;
        let buy = item.buy_text();
        if buy.is_empty() {
            let (_, cells) = craft.cell_word();
            out.push_str(&format!("{:<50} {} {cells}\n", item.label, item.used));
        } else {
            out.push_str(&format!("{:<50} {buy}\n", item.label));
        }
    }
    if let Some(text) = instructions_text(chart) {
        out.push('\n');
        out.push_str(&text);
    }
    if let Some(unit) = list.first().map(|i| i.unit).filter(|u| !u.is_empty()) {
        let unit = unit.split(' ').next().unwrap_or(unit);
        out.push_str(&format!(
            "\nTotal: {total} {unit}{} (estimates - round up when unsure)\n",
            if total == 1 { "" } else { "s" }
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::{Backstitch, Knot};
    use crate::threads::find_dmc;
    use crate::Floss;

    fn sample() -> Chart {
        let mut c = Chart::new(12, 3);
        c.add_floss(Floss::from_thread(find_dmc("310").unwrap(), '&'));
        c.set(0, 0, Some(0));
        c.set(11, 2, Some(0));
        c
    }

    #[test]
    fn svg_has_one_symbol_per_stitch_and_escapes_it() {
        let svg = export_chart_svg(&sample());
        assert_eq!(svg.matches("<text").count(), 2);
        assert!(svg.contains(">&amp;</text>"));
        assert!(!svg.contains(">&</text>"));
    }

    #[test]
    fn svg_draws_part_stitches_backstitch_and_knots() {
        let mut c = sample();
        c.set_partial(
            1,
            0,
            Partial::Split {
                diagonal: Diagonal::Slash,
                first: Some(0),
                second: None,
            },
        );
        c.set_partial(
            2,
            0,
            Partial::Half {
                diagonal: Diagonal::Backslash,
                floss: 0,
            },
        );
        c.backstitches.push(Backstitch {
            from: (0, 0),
            to: (4, 4),
            floss: 0,
        });
        c.knots.push(Knot {
            at: (3, 3),
            floss: 0,
        });
        let svg = export_chart_svg(&c);
        assert_eq!(svg.matches("<polygon").count(), 1);
        assert_eq!(svg.matches("<circle").count(), 1);
        assert!(svg.contains(r#"stroke-linecap="round""#));
    }

    #[test]
    fn materials_for_other_crafts_use_their_own_words_and_packs() {
        let mut c = Chart::new_for(crate::GridCraft::FuseBeads);
        c.add_floss(Floss::from_thread(
            crate::Catalog::Hama.find("H01").unwrap(),
            'X',
        ));
        c.set(0, 0, Some(0));
        c.crop_or_pad(40, 29);
        let text = materials_text(&c);
        assert!(text.contains("fuse beads materials"), "{text}");
        assert!(text.contains("Midi beads (5 mm), on pegboards"), "{text}");
        assert!(
            text.contains("Pegboard: 2 pegboards of 29 x 29 (2 across x 1 down)"),
            "{text}"
        );
        assert!(text.contains("Hama H01 White"), "{text}");
        assert!(text.contains("1 bag of 1,000 beads"), "{text}");
        assert!(!text.contains("hoop"), "{text}");

        let mut art = Chart::new_for(crate::GridCraft::PixelArt);
        art.add_floss(Floss::free([0, 0, 0], 'X'));
        art.set(0, 0, Some(0));
        let text = materials_text(&art);
        assert!(text.contains("Black #000000"), "{text}");
        assert!(!text.contains("Total:"), "no packs for pixel art: {text}");
    }

    #[test]
    fn knitting_and_quilt_materials_include_their_instructions() {
        let mut k = Chart::new_for(crate::GridCraft::Knitting);
        k.crop_or_pad(4, 2);
        k.add_floss(Floss::free([200, 0, 0], 'X'));
        k.set(0, 0, Some(0));
        let text = materials_text(&k);
        assert!(text.contains("DK (22 sts x 30 rows / 4 in)"), "{text}");
        assert!(text.contains("Row 1 (RS): 4 MC"), "{text}");
        assert!(text.contains("% of the yarn"), "{text}");
        assert!(!text.contains("Total:"), "{text}");

        let mut q = Chart::new_for(crate::GridCraft::Quilt);
        q.add_floss(Floss::free([0, 0, 200], 'O'));
        q.set(0, 0, Some(0));
        let text = materials_text(&q);
        assert!(text.contains("2 in finished squares"), "{text}");
        assert!(text.contains("Backing"), "{text}");
        assert!(text.contains("Row 1: O BG"), "{text}");

        // Non-square cells in the SVG: knit cells are shorter than wide.
        let svg = export_chart_svg(&k);
        // 14 x 22/30 = 10.27
        assert!(
            svg.contains(r#"width="14" height="10.26"#),
            "{}",
            &svg[..400]
        );
    }

    #[test]
    fn materials_lists_key_and_shopping_list() {
        let mut c = sample();
        c.knots.push(Knot {
            at: (3, 3),
            floss: 0,
        });
        let text = materials_text(&c);
        assert!(text.contains("DMC 310 Black"));
        assert!(text.contains("2 full, 1 knot"), "{text}");
        assert!(text.contains("Total: 1 skein "), "{text}");
        assert!(text.contains("size 24 tapestry"));
    }
}
