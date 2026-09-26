//! Printable cross-stitch pattern PDF: a cover page with the design's
//! materials (fabric, finished size, hoop, needle) and the floss key
//! (symbol, color, thread, strands, stitch count, skeins), followed by
//! the symbol chart tiled across pages exactly like `print.rs` tiles a
//! colorwork chart - same `compute_tiling` math, same edge reference
//! numbers and page footer, so pages tape together the same way.
//!
//! Cross-stitch-specific chart conventions on top of that: every stitched
//! square carries its floss symbol (in black or white, whichever contrasts
//! with the floss color), unstitched squares are left blank, every 10th
//! grid line is heavier (the counting aid stitchers expect), and red
//! arrows on the page edges mark the design's center row/column. Part
//! stitches are drawn as the usual chart shapes (half = diagonal bar, 3/4
//! = triangle with a small symbol, 1/4 = quarter square), backstitch as
//! colored lines clipped at page edges, and French knots as dots.

use crate::print::{
    add_instructions_pages, compute_tiling_rect, sanitize_filename, PageSize, AXIS_LABEL_INTERVAL,
    LABEL_STRIP_MM,
};
use abyssal_thread_crossstitch::export::{
    contrasting_ink, half_stitch_ends, quarter_origin, split_triangle, usage_summary,
};
use abyssal_thread_crossstitch::{Chart, Corner, Floss, Partial};
use printpdf::*;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

const PT_TO_MM: f32 = 0.3528;
const KEY_ROW_MM: f32 = 7.0;
const KEY_SWATCH_MM: f32 = 5.5;

fn rgb(c: [u8; 3]) -> Color {
    Color::Rgb(Rgb::new(
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        None,
    ))
}

fn line(layer: &PdfLayerReference, x0: f32, y0: f32, x1: f32, y1: f32) {
    layer.add_line(Line {
        points: vec![
            (Point::new(Mm(x0), Mm(y0)), false),
            (Point::new(Mm(x1), Mm(y1)), false),
        ],
        is_closed: false,
    });
}

fn triangle(layer: &PdfLayerReference, pts: [(f32, f32); 3]) {
    polygon(layer, &pts);
}

fn polygon(layer: &PdfLayerReference, pts: &[(f32, f32)]) {
    layer.add_polygon(Polygon {
        rings: vec![pts
            .iter()
            .map(|&(x, y)| (Point::new(Mm(x), Mm(y)), false))
            .collect()],
        mode: printpdf::path::PaintMode::Fill,
        winding_order: printpdf::path::WindingOrder::NonZero,
    });
}

/// Clips segment `a`-`b` to the rectangle `[x0, x1] x [y0, y1]`
/// (Liang-Barsky), so a backstitch crossing a page edge is drawn only up
/// to that edge on each page.
fn clip_segment(
    a: (f32, f32),
    b: (f32, f32),
    (x0, y0, x1, y1): (f32, f32, f32, f32),
) -> Option<((f32, f32), (f32, f32))> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [
        (-dx, a.0 - x0),
        (dx, x1 - a.0),
        (-dy, a.1 - y0),
        (dy, y1 - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 <= t1).then_some((
        (a.0 + t0 * dx, a.1 + t0 * dy),
        (a.0 + t1 * dx, a.1 + t1 * dy),
    ))
}

/// Draws `symbol` centered in the square whose top-left is `(x0, y0)`.
/// Helvetica glyph widths vary, so horizontal centering uses an average
/// glyph width - close enough at chart-symbol sizes.
fn draw_symbol(
    layer: &PdfLayerReference,
    font: &IndirectFontRef,
    symbol: char,
    x0: f32,
    y0: f32,
    cell_mm: f32,
) {
    draw_symbol_scaled(layer, font, symbol, x0, y0, cell_mm, 1.0);
}

/// `draw_symbol` at `scale` of the normal size, still centered in the
/// `cell_mm` square at `(x0, y0)`.
fn draw_symbol_scaled(
    layer: &PdfLayerReference,
    font: &IndirectFontRef,
    symbol: char,
    x0: f32,
    y0: f32,
    cell_mm: f32,
    scale: f32,
) {
    let size_pt = (cell_mm * 2.3).min(14.0) * scale;
    let size_mm = size_pt * PT_TO_MM;
    let x = x0 + cell_mm / 2.0 - size_mm * 0.3;
    let y = y0 - cell_mm / 2.0 - size_mm * 0.36;
    layer.use_text(symbol.to_string(), size_pt, Mm(x), Mm(y), font);
}

/// Characters per instructions line that fit a Letter/A4 page at the
/// instructions font size, with margins.
const INSTRUCTION_WRAP_CHARS: usize = 100;

/// Wraps long lines at ", " (row instructions are comma-separated runs),
/// indenting continuations, so wide charts' rows don't run off the page.
fn wrap_lines(text: &str, max: usize) -> String {
    let mut out = String::new();
    for line in text.lines() {
        if line.chars().count() <= max {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let mut current = String::new();
        for (i, part) in line.split(", ").enumerate() {
            let piece = if i == 0 {
                part.to_string()
            } else {
                format!(", {part}")
            };
            // +1 leaves room for the comma that ends a wrapped line.
            if !current.is_empty() && current.chars().count() + piece.chars().count() + 1 > max {
                out.push_str(current.trim_end());
                out.push_str(",\n");
                current = format!("    {}", piece.trim_start_matches(", "));
            } else {
                current.push_str(&piece);
            }
        }
        out.push_str(&current);
        out.push('\n');
    }
    out
}

/// Draws `symbol` centered on `(cx, cy)`, sized for a `cell_mm` cell.
fn draw_symbol_centered(
    layer: &PdfLayerReference,
    font: &IndirectFontRef,
    symbol: char,
    cx: f32,
    cy: f32,
    cell_mm: f32,
    scale: f32,
) {
    let half = cell_mm / 2.0;
    draw_symbol_scaled(layer, font, symbol, cx - half, cy + half, cell_mm, scale);
}

/// Key swatch: the floss color (half-and-half for a blend) with its
/// symbol, top-left at `(sx, sy)`.
fn swatch(layer: &PdfLayerReference, font: &IndirectFontRef, f: &Floss, sx: f32, sy: f32) {
    let s = KEY_SWATCH_MM;
    layer.set_fill_color(rgb(f.rgb));
    layer.add_rect(
        Rect::new(Mm(sx), Mm(sy - s), Mm(sx + s), Mm(sy))
            .with_mode(printpdf::path::PaintMode::Fill),
    );
    if let Some(b) = &f.blend {
        layer.set_fill_color(rgb(b.rgb));
        triangle(layer, [(sx + s, sy), (sx + s, sy - s), (sx, sy - s)]);
    }
    layer.set_fill_color(rgb(contrasting_ink(f.display_rgb())));
    draw_symbol(layer, font, f.symbol, sx, sy, s);
}

fn polygon_points(layer: &PdfLayerReference, points: Vec<(Point, bool)>) {
    layer.add_polygon(Polygon {
        rings: vec![points],
        mode: printpdf::path::PaintMode::Fill,
        winding_order: printpdf::path::WindingOrder::NonZero,
    });
}

pub fn generate_cross_stitch_pdf(
    chart: &Chart,
    cell_mm: f32,
    pattern_name: &str,
    out_path: &Path,
    page: PageSize,
    margin_mm: f32,
) -> anyhow::Result<()> {
    let (doc, cover_page, cover_layer) = PdfDocument::new(
        pattern_name,
        Mm(page.width_mm),
        Mm(page.height_mm),
        "Layer 1",
    );
    let font = doc.add_builtin_font(BuiltinFont::Helvetica)?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)?;

    // --- Cover: materials + floss key (continuing onto more pages if the
    // palette is long).
    let mut layer = doc.get_page(cover_page).get_layer(cover_layer);
    let top = page.height_mm - margin_mm;
    layer.use_text(pattern_name, 18.0, Mm(margin_mm), Mm(top - 4.0), &font_bold);
    let craft = chart.craft;
    let (_, cells) = craft.cell_word();
    let mut info = abyssal_thread_crossstitch::export::info_lines(chart);
    if craft.has_part_stitches() {
        info.push("Work full crosses with the strand count listed per floss.".to_string());
        info.push(
            "Chart key: symbol square = full cross, triangle = 3/4 stitch, diagonal bar = half stitch,"
                .to_string(),
        );
        info.push(
            "small square = 1/4 stitch, colored line = backstitch, dot = French knot.".to_string(),
        );
    }
    if chart.board.is_some() {
        info.push(format!(
            "Red lines on the chart mark {} edges - build one {} at a time.",
            craft.board_word(),
            craft.board_word()
        ));
    }
    info.push(
        "Tip: select COLOR (not Grayscale / Black & White) in your print dialog.".to_string(),
    );
    for (i, text) in info.iter().enumerate() {
        layer.use_text(
            text.as_str(),
            10.0,
            Mm(margin_mm),
            Mm(top - 14.0 - i as f32 * 5.5),
            &font,
        );
    }

    let mut y = top - 14.0 - info.len() as f32 * 5.5 - 8.0;
    let columns: [(f32, &str); 4] = if craft.has_part_stitches() {
        [
            (0.0, "Symbol"),
            (18.0, "Floss"),
            (108.0, "Strands"),
            (126.0, "Used for"),
        ]
    } else {
        [
            (0.0, "Symbol"),
            (18.0, "Color"),
            (108.0, cells),
            (126.0, ""),
        ]
    };
    let header = |layer: &PdfLayerReference, y: f32| {
        layer.set_fill_color(rgb([0, 0, 0]));
        for (x, label) in columns {
            layer.use_text(label, 9.0, Mm(margin_mm + x), Mm(y), &font_bold);
        }
    };
    // Starts a continuation page when fewer than `rows` rows fit.
    let ensure_room = |layer: &mut PdfLayerReference, y: &mut f32, rows: f32| {
        if *y < margin_mm + KEY_ROW_MM * rows {
            let (p, l) = doc.add_page(Mm(page.width_mm), Mm(page.height_mm), "Layer 1");
            *layer = doc.get_page(p).get_layer(l);
            *y = top - 4.0;
            true
        } else {
            false
        }
    };
    header(&layer, y);
    y -= KEY_ROW_MM;
    for (f, u) in chart.palette.iter().zip(chart.usage()) {
        if ensure_room(&mut layer, &mut y, 1.0) {
            header(&layer, y);
            y -= KEY_ROW_MM;
        }
        let sx = margin_mm + 2.0;
        let sy = y + KEY_SWATCH_MM - 1.5;
        swatch(&layer, &font, f, sx, sy);
        layer.set_fill_color(rgb([0, 0, 0]));
        if !craft.has_part_stitches() {
            layer.use_text(f.label(), 9.0, Mm(margin_mm + 18.0), Mm(y), &font);
            layer.use_text(u.full.to_string(), 9.0, Mm(margin_mm + 108.0), Mm(y), &font);
            y -= KEY_ROW_MM;
            continue;
        }
        let strands = if f.blend.is_some() {
            format!("{} each", f.strands)
        } else {
            f.strands.to_string()
        };
        let mut used = usage_summary(&u);
        if u.backstitch_squares > 0.0 {
            used.push_str(&format!(" ({} str.)", f.bs_strands));
        }
        let label_pt = if f.blend.is_some() { 7.5 } else { 9.0 };
        layer.use_text(f.label(), label_pt, Mm(margin_mm + 18.0), Mm(y), &font);
        layer.use_text(strands, 9.0, Mm(margin_mm + 108.0), Mm(y), &font);
        layer.use_text(used, 8.0, Mm(margin_mm + 126.0), Mm(y), &font);
        y -= KEY_ROW_MM;
    }

    // Shopping list: one line per thing to buy (for cross stitch, per
    // physical thread with blends split out).
    y -= KEY_ROW_MM;
    ensure_room(&mut layer, &mut y, 3.0);
    layer.set_fill_color(rgb([0, 0, 0]));
    layer.use_text("Shopping list", 11.0, Mm(margin_mm), Mm(y), &font_bold);
    y -= KEY_ROW_MM;
    let mut total = 0;
    for item in chart.shopping_list() {
        ensure_room(&mut layer, &mut y, 1.0);
        total += item.buy;
        let sx = margin_mm + 2.0;
        let sy = y + KEY_SWATCH_MM - 1.5;
        layer.set_fill_color(rgb(item.rgb));
        layer.add_rect(
            Rect::new(
                Mm(sx),
                Mm(sy - KEY_SWATCH_MM),
                Mm(sx + KEY_SWATCH_MM),
                Mm(sy),
            )
            .with_mode(printpdf::path::PaintMode::Fill),
        );
        layer.set_fill_color(rgb([0, 0, 0]));
        layer.use_text(item.label.as_str(), 9.0, Mm(margin_mm + 18.0), Mm(y), &font);
        let buy = item.buy_text();
        layer.use_text(
            if buy.is_empty() {
                format!("{} {cells}", item.used)
            } else {
                buy
            },
            9.0,
            Mm(margin_mm + 108.0),
            Mm(y),
            &font,
        );
        y -= KEY_ROW_MM;
    }
    let total_line = if craft == abyssal_thread_crossstitch::GridCraft::CrossStitch {
        Some(format!(
            "Total: {total} skeins (estimates - round up when unsure)"
        ))
    } else {
        craft
            .packaging(chart.fabric.count)
            .map(|p| format!("Total: {total} packs ({}; sizes vary by seller)", p.unit))
    };
    if let Some(line) = total_line {
        ensure_room(&mut layer, &mut y, 1.0);
        layer.use_text(line, 9.0, Mm(margin_mm + 18.0), Mm(y), &font_bold);
    }

    // --- Chart pages.
    // Knitting cells are shorter than wide (cw x ch); square otherwise.
    let (cw, ch) = (cell_mm, cell_mm * chart.fabric.cell_aspect());
    let cmin = cw.min(ch);
    let plan = compute_tiling_rect(chart.width, chart.height, cw, ch, page, margin_mm);
    let grid_origin_x = margin_mm + LABEL_STRIP_MM;
    let grid_top_y = page.height_mm - margin_mm - LABEL_STRIP_MM;
    let (center_x, center_y) = (chart.width / 2, chart.height / 2);
    for py in 0..plan.pages_y {
        for px in 0..plan.pages_x {
            let (p, l) = doc.add_page(Mm(page.width_mm), Mm(page.height_mm), "Layer 1");
            let layer = doc.get_page(p).get_layer(l);
            let col_start = px * plan.cells_per_page_x;
            let col_end = (col_start + plan.cells_per_page_x).min(chart.width);
            let row_start = py * plan.cells_per_page_y;
            let row_end = (row_start + plan.cells_per_page_y).min(chart.height);
            let tile_w = (col_end - col_start) as f32 * cw;
            let tile_h = (row_end - row_start) as f32 * ch;

            for (ly, gy) in (row_start..row_end).enumerate() {
                for (lx, gx) in (col_start..col_end).enumerate() {
                    let Some(f) = chart
                        .get(gx, gy)
                        .and_then(|i| chart.palette.get(i as usize))
                    else {
                        continue;
                    };
                    let x0 = grid_origin_x + lx as f32 * cw;
                    let y0 = grid_top_y - ly as f32 * ch;
                    layer.set_fill_color(rgb(f.display_rgb()));
                    layer.add_rect(
                        Rect::new(Mm(x0), Mm(y0 - ch), Mm(x0 + cw), Mm(y0))
                            .with_mode(printpdf::path::PaintMode::Fill),
                    );
                    layer.set_fill_color(rgb(contrasting_ink(f.display_rgb())));
                    draw_symbol_centered(
                        &layer,
                        &font,
                        f.symbol,
                        x0 + cw / 2.0,
                        y0 - ch / 2.0,
                        cmin,
                        1.0,
                    );
                }
            }

            // Page position (mm) of a global point given in squares.
            let to_page = |(sx, sy): (f32, f32)| {
                (
                    grid_origin_x + (sx - col_start as f32) * cw,
                    grid_top_y - (sy - row_start as f32) * ch,
                )
            };
            let floss = |i: u16| chart.palette.get(i as usize);
            for (&(gy, gx), p) in chart
                .partials
                .range((row_start, 0)..(row_end, 0))
                .filter(|(&(_, x), _)| (col_start..col_end).contains(&x))
            {
                let at = |(dx, dy): (f32, f32)| to_page((gx as f32 + dx, gy as f32 + dy));
                match p {
                    Partial::Half { diagonal, floss: i } => {
                        let Some(f) = floss(*i) else { continue };
                        let [a, b] = half_stitch_ends(*diagonal).map(at);
                        layer.set_outline_color(rgb(f.display_rgb()));
                        layer.set_outline_thickness(cmin * 0.3 / PT_TO_MM);
                        line(&layer, a.0, a.1, b.0, b.1);
                    }
                    Partial::Split {
                        diagonal,
                        first,
                        second,
                    } => {
                        for (side, v) in [(true, first), (false, second)] {
                            let Some(f) = v.and_then(floss) else { continue };
                            let pts = split_triangle(*diagonal, side);
                            layer.set_fill_color(rgb(f.display_rgb()));
                            triangle(&layer, pts.map(at));
                            // Small symbol at the triangle's centroid.
                            let (cx, cy) = (
                                pts.iter().map(|p| p.0).sum::<f32>() / 3.0,
                                pts.iter().map(|p| p.1).sum::<f32>() / 3.0,
                            );
                            let (px, py) = at((cx, cy));
                            layer.set_fill_color(rgb(contrasting_ink(f.display_rgb())));
                            draw_symbol_centered(&layer, &font, f.symbol, px, py, cmin, 0.5);
                        }
                    }
                    Partial::Quarters(q) => {
                        for corner in Corner::ALL {
                            let Some(f) = q[corner as usize].and_then(floss) else {
                                continue;
                            };
                            let (ox, oy) = quarter_origin(corner);
                            let (x0, y0) = at((ox, oy));
                            layer.set_fill_color(rgb(f.display_rgb()));
                            layer.add_rect(
                                Rect::new(Mm(x0), Mm(y0 - ch / 2.0), Mm(x0 + cw / 2.0), Mm(y0))
                                    .with_mode(printpdf::path::PaintMode::Fill),
                            );
                        }
                    }
                }
            }

            // Grid: thin every square, heavy every 10th (by global index,
            // so the heavy lines line up across taped-together pages).
            for heavy in [false, true] {
                layer.set_outline_color(if heavy {
                    rgb([0, 0, 0])
                } else {
                    rgb([150, 150, 150])
                });
                layer.set_outline_thickness(if heavy { 0.9 } else { 0.25 });
                for (c, gx) in (col_start..=col_end).enumerate() {
                    if chart.is_heavy_col_line(gx) == heavy {
                        let x = grid_origin_x + c as f32 * cw;
                        line(&layer, x, grid_top_y - tile_h, x, grid_top_y);
                    }
                }
                for (r, gy) in (row_start..=row_end).enumerate() {
                    if chart.is_heavy_row_line(gy) == heavy {
                        let y = grid_top_y - r as f32 * ch;
                        line(&layer, grid_origin_x, y, grid_origin_x + tile_w, y);
                    }
                }
            }

            // Backstitch and knots go on top of the grid lines.
            let tile = (
                col_start as f32,
                row_start as f32,
                col_end as f32,
                row_end as f32,
            );
            let half = |(x, y): (u32, u32)| (x as f32 / 2.0, y as f32 / 2.0);
            for b in &chart.backstitches {
                let Some(f) = floss(b.floss) else { continue };
                let Some((a, c)) = clip_segment(half(b.from), half(b.to), tile) else {
                    continue;
                };
                let (a, c) = (to_page(a), to_page(c));
                layer.set_outline_color(rgb(f.display_rgb()));
                layer.set_outline_thickness((cmin * 0.16 / PT_TO_MM).max(1.0));
                line(&layer, a.0, a.1, c.0, c.1);
            }
            for k in &chart.knots {
                let Some(f) = floss(k.floss) else { continue };
                let (kx, ky) = half(k.at);
                if kx < tile.0 || kx > tile.2 || ky < tile.1 || ky > tile.3 {
                    continue;
                }
                let (px, py) = to_page((kx, ky));
                let r = cmin * 0.25;
                layer.set_fill_color(rgb([0, 0, 0]));
                polygon_points(
                    &layer,
                    printpdf::utils::calculate_points_for_circle(Mm(r), Mm(px), Mm(py)),
                );
                layer.set_fill_color(rgb(f.display_rgb()));
                polygon_points(
                    &layer,
                    printpdf::utils::calculate_points_for_circle(Mm(r * 0.75), Mm(px), Mm(py)),
                );
            }

            // Board (pegboard/baseplate) edges, by global index so they
            // line up across taped-together pages.
            if let Some((bw, bh)) = chart.board {
                layer.set_outline_color(rgb([220, 30, 30]));
                layer.set_outline_thickness(1.6);
                for (c, gx) in (col_start..=col_end).enumerate() {
                    if gx % bw.max(1) == 0 || gx == chart.width {
                        let x = grid_origin_x + c as f32 * cw;
                        line(&layer, x, grid_top_y - tile_h, x, grid_top_y);
                    }
                }
                for (r, gy) in (row_start..=row_end).enumerate() {
                    if gy % bh.max(1) == 0 || gy == chart.height {
                        let y = grid_top_y - r as f32 * ch;
                        line(&layer, grid_origin_x, y, grid_origin_x + tile_w, y);
                    }
                }
            }

            // Center arrows, on whichever page edges the center falls.
            layer.set_fill_color(rgb([200, 0, 0]));
            let a = 2.2;
            if (col_start..col_end).contains(&center_x) {
                let x = grid_origin_x + (center_x - col_start) as f32 * cw;
                triangle(
                    &layer,
                    [
                        (x, grid_top_y),
                        (x - a, grid_top_y + a * 1.4),
                        (x + a, grid_top_y + a * 1.4),
                    ],
                );
                let yb = grid_top_y - tile_h;
                triangle(
                    &layer,
                    [(x, yb), (x - a, yb - a * 1.4), (x + a, yb - a * 1.4)],
                );
            }
            if (row_start..row_end).contains(&center_y) {
                let y = grid_top_y - (center_y - row_start) as f32 * ch;
                triangle(
                    &layer,
                    [
                        (grid_origin_x, y),
                        (grid_origin_x - a * 1.4, y + a),
                        (grid_origin_x - a * 1.4, y - a),
                    ],
                );
                let xr = grid_origin_x + tile_w;
                triangle(
                    &layer,
                    [(xr, y), (xr + a * 1.4, y + a), (xr + a * 1.4, y - a)],
                );
            }

            // Reference numbers every 10 (1, 11, 21...), in the chart's own
            // numbering - knitting counts rows from the bottom and
            // stitches from the right, with row numbers on the right edge
            // where the knitter starts reading each right-side row.
            layer.set_fill_color(rgb([0, 0, 0]));
            let labeled = |n: usize| (n - 1).is_multiple_of(AXIS_LABEL_INTERVAL);
            for (lx, gx) in (col_start..col_end).enumerate() {
                let n = chart.col_number(gx);
                if labeled(n) {
                    let mut x = grid_origin_x + lx as f32 * cw;
                    // Right-to-left numbering: align to the column's right
                    // edge, next to the heavy line it counts from.
                    if craft.numbers_from_bottom_right() {
                        x += cw - 0.9 * n.to_string().len() as f32 * 1.2;
                    }
                    layer.use_text(format!("{n}"), 6.0, Mm(x), Mm(grid_top_y + 1.5), &font);
                }
            }
            let rows_on_right = craft.numbers_from_bottom_right();
            for (ly, gy) in (row_start..row_end).enumerate() {
                let n = chart.row_number(gy);
                if labeled(n) {
                    let y = grid_top_y - ly as f32 * ch;
                    let x = if rows_on_right {
                        grid_origin_x + tile_w + 1.0
                    } else {
                        grid_origin_x - LABEL_STRIP_MM + 0.5
                    };
                    layer.use_text(format!("{n}"), 6.0, Mm(x), Mm(y - ch / 2.0 - 1.0), &font);
                }
            }
            let span = |a: usize, b: usize| (a.min(b), a.max(b));
            let (c0, c1) = span(chart.col_number(col_start), chart.col_number(col_end - 1));
            let (r0, r1) = span(chart.row_number(row_start), chart.row_number(row_end - 1));
            layer.use_text(
                format!(
                    "{pattern_name} - chart page (row {}, col {}) of ({} x {}) - columns {}-{}, rows {}-{}",
                    py + 1,
                    px + 1,
                    plan.pages_y,
                    plan.pages_x,
                    c0,
                    c1,
                    r0,
                    r1,
                ),
                8.0,
                Mm(margin_mm),
                Mm(margin_mm / 2.0),
                &font,
            );
        }
    }

    // Knitting's row-by-row directions / a quilt's cutting list and
    // assembly, after the chart pages.
    if let Some(text) = abyssal_thread_crossstitch::export::instructions_text(chart) {
        let text = wrap_lines(&text, INSTRUCTION_WRAP_CHARS);
        add_instructions_pages(
            &doc,
            &font,
            &font_bold,
            pattern_name,
            &text,
            page,
            margin_mm,
        );
    }

    doc.save(&mut BufWriter::new(File::create(out_path)?))?;
    Ok(())
}

pub fn print_cross_stitch_via_system_default(
    chart: &Chart,
    cell_mm: f32,
    pattern_name: &str,
    page: PageSize,
    margin_mm: f32,
) -> anyhow::Result<()> {
    let mut path = std::env::temp_dir();
    path.push(format!("{}_print.pdf", sanitize_filename(pattern_name)));
    generate_cross_stitch_pdf(chart, cell_mm, pattern_name, &path, page, margin_mm)?;
    opener::open(&path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::print::compute_tiling;
    use abyssal_thread_crossstitch::threads::dmc;
    use abyssal_thread_crossstitch::Floss;

    fn big_chart(colors: usize) -> Chart {
        let mut c = Chart::new(120, 90);
        for t in dmc().iter().take(colors) {
            let s = c.next_symbol();
            c.add_floss(Floss::from_thread(t, s));
        }
        for y in 0..c.height {
            for x in 0..c.width {
                if (x + y) % 3 != 0 {
                    c.set(x, y, Some(((x * 7 + y) % colors) as u16));
                }
            }
        }
        // Every other kind of stitch, including a backstitch that spans
        // several chart pages (exercises the page-edge clipping).
        c.palette[0].blend = Some(abyssal_thread_crossstitch::BlendThread::from_thread(
            &dmc()[100],
        ));
        c.set_partial(
            0,
            0,
            Partial::Half {
                diagonal: abyssal_thread_crossstitch::Diagonal::Slash,
                floss: 1,
            },
        );
        c.set_partial(
            3,
            0,
            Partial::Split {
                diagonal: abyssal_thread_crossstitch::Diagonal::Backslash,
                first: Some(2),
                second: Some(3),
            },
        );
        c.set_partial(6, 0, Partial::Quarters([Some(1), None, None, Some(2)]));
        c.backstitches.push(abyssal_thread_crossstitch::Backstitch {
            from: (0, 0),
            to: (240, 180),
            floss: 4,
        });
        c.knots.push(abyssal_thread_crossstitch::Knot {
            at: (5, 5),
            floss: 5,
        });
        c
    }

    #[test]
    fn generates_a_multi_page_pdf_with_a_long_floss_key() {
        // 60 colors overflows the cover page's key; 120x90 at 3mm tiles
        // across several pages.
        let chart = big_chart(60);
        let path = std::env::temp_dir().join("abyssal_thread_xstitch_test.pdf");
        generate_cross_stitch_pdf(&chart, 3.0, "Test", &path, PageSize::US_LETTER, 12.7)
            .expect("PDF generation should succeed");
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        // The tiling itself is pinned by print.rs's tests; here just make
        // sure this chart really does span several chart pages and the
        // output has real content for all of them.
        let plan = compute_tiling(120, 90, 3.0, PageSize::US_LETTER, 12.7);
        assert!(plan.pages_x * plan.pages_y >= 2);
        assert!(
            bytes.len() > 20_000,
            "suspiciously small PDF: {} bytes",
            bytes.len()
        );
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod clip_tests {
    use super::{clip_segment, wrap_lines};

    #[test]
    fn wraps_long_instruction_lines_at_commas() {
        let long = format!("Row 1 (RS): {}", vec!["3 MC"; 40].join(", "));
        let wrapped = wrap_lines(&long, 40);
        assert!(
            wrapped.lines().all(|l| l.chars().count() <= 40),
            "{wrapped}"
        );
        assert!(wrapped.lines().skip(1).all(|l| l.starts_with("    ")));
        assert_eq!(wrapped.matches("3 MC").count(), 40, "nothing lost");
        assert_eq!(wrap_lines("short", 40), "short\n");
    }

    #[test]
    fn clips_segments_to_the_page_tile() {
        let tile = (0.0, 0.0, 10.0, 10.0);
        assert_eq!(
            clip_segment((1.0, 1.0), (2.0, 3.0), tile),
            Some(((1.0, 1.0), (2.0, 3.0)))
        );
        assert_eq!(
            clip_segment((5.0, 5.0), (15.0, 5.0), tile),
            Some(((5.0, 5.0), (10.0, 5.0)))
        );
        assert_eq!(
            clip_segment((-5.0, 5.0), (15.0, 5.0), tile),
            Some(((0.0, 5.0), (10.0, 5.0)))
        );
        assert_eq!(clip_segment((11.0, 0.0), (12.0, 5.0), tile), None);
        assert_eq!(
            clip_segment((5.0, 12.0), (5.0, 11.0), tile),
            None,
            "vertical, fully outside"
        );
    }
}
