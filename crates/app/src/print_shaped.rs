//! Multi-page tiled PDF export for *shaped* patterns (round/row stitch
//! scripting), mirroring `print.rs`'s colorwork pipeline but showing each
//! stitch's abbreviation (e.g. "sc", "flo.dc") instead of a color swatch,
//! tinted by tension state or by an explicit per-stitch color when one's
//! set (see `StitchNode::color` and the `~RRGGBB` DSL syntax).
//!
//! Reuses `print::compute_tiling`/`PageSize` as-is - the tiling math only
//! cares about a width/height in cells, not what a cell contains.
//!
//! One real difference from colorwork: round 0 is drawn at the *bottom* of
//! the chart here, not the top - matching how `export_svg_chart` already
//! draws shaped patterns (rounds are worked bottom-to-top in real
//! crochet), whereas colorwork's row 0 = top matches a photo's natural
//! top-down reading order. Same reasoning, opposite convention, because
//! they're answering different questions ("what does the picture look
//! like" vs. "what order do I crochet this in").
//!
//! Text isn't pixel-perfectly centered in each cell - printpdf's built-in
//! fonts don't expose text-width metrics cheaply, so this left-aligns with
//! a small fixed inset instead of measuring each label's width. Readable,
//! not centered. A bigger "Print cell size" than colorwork's typical
//! 0.10in default is recommended here, since real text needs more room
//! than a color swatch does.

use crate::print::{compute_tiling, PageSize};
use abyssal_thread_core::graph::TensionState;
use abyssal_thread_core::StitchGraph;
use printpdf::*;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

const LABEL_STRIP_MM: f32 = 6.0;
const AXIS_LABEL_INTERVAL: usize = 10;

fn stitch_print_rgb(color: Option<[u8; 3]>, tension: Option<TensionState>) -> (f32, f32, f32) {
    if let Some([r, g, b]) = color {
        return (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    }
    match tension {
        Some(TensionState::Normal) => (0.15, 0.55, 0.15),
        Some(TensionState::Loose) => (0.15, 0.35, 0.75),
        Some(TensionState::Stretched) => (0.75, 0.15, 0.15),
        // Black rather than viewport.rs's on-screen gray(180) - gray text
        // on white paper is hard to read; this is a print-specific choice.
        None => (0.0, 0.0, 0.0),
    }
}

pub fn generate_shaped_pattern_pdf(
    graph: &StitchGraph,
    cell_mm: f32,
    pattern_name: &str,
    out_path: &Path,
    page: PageSize,
    margin_mm: f32,
) -> anyhow::Result<()> {
    let total_rounds = graph.rounds.len();
    let max_round_len = graph.rounds.iter().map(|r| r.len()).max().unwrap_or(0);
    let plan = compute_tiling(max_round_len, total_rounds, cell_mm, page, margin_mm);

    let (doc, mut page_idx, mut layer_idx) = PdfDocument::new(
        pattern_name,
        Mm(page.width_mm),
        Mm(page.height_mm),
        "Layer 1",
    );
    let font = doc.add_builtin_font(BuiltinFont::Helvetica)?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)?;

    let grid_origin_x = margin_mm + LABEL_STRIP_MM;
    let grid_top_y = page.height_mm - margin_mm - LABEL_STRIP_MM;

    let mut first_page = true;
    for py in 0..plan.pages_y {
        for px in 0..plan.pages_x {
            if !first_page {
                let (p, l) = doc.add_page(Mm(page.width_mm), Mm(page.height_mm), "Layer 1");
                page_idx = p;
                layer_idx = l;
            }
            first_page = false;
            let layer = doc.get_page(page_idx).get_layer(layer_idx);

            let col_start = px * plan.cells_per_page_x;
            let col_end = (col_start + plan.cells_per_page_x).min(max_round_len);
            // "rft" = row-from-top, i.e. plain top-down pagination index.
            // Round 0 is at the *bottom* of the whole chart, so it maps to
            // the highest rft, not rft = 0 - see the module doc.
            let rft_start = py * plan.cells_per_page_y;
            let rft_end = (rft_start + plan.cells_per_page_y).min(total_rounds);

            let font_size = (cell_mm * 2.5).clamp(4.0, 24.0);

            for (local_y, rft) in (rft_start..rft_end).enumerate() {
                let round_idx = total_rounds - 1 - rft;
                let round = &graph.rounds[round_idx];
                let y0 = grid_top_y - local_y as f32 * cell_mm;

                for (local_x, gx) in (col_start..col_end).enumerate() {
                    let Some(&node_idx) = round.get(gx) else {
                        continue;
                    };
                    let node = &graph.graph[node_idx];
                    let label = node.kind.display_label();
                    let (r, g, b) = stitch_print_rgb(node.color, node.tension);
                    let x0 = grid_origin_x + local_x as f32 * cell_mm;

                    layer.set_fill_color(Color::Rgb(Rgb::new(r, g, b, None)));
                    layer.use_text(
                        &label,
                        font_size,
                        Mm(x0 + 0.5),
                        Mm(y0 - cell_mm * 0.75),
                        &font,
                    );
                }
            }

            layer.set_outline_color(Color::Rgb(Rgb::new(0.6, 0.6, 0.6, None)));
            layer.set_outline_thickness(0.3);
            let tile_w = (col_end - col_start) as f32 * cell_mm;
            let tile_h = (rft_end - rft_start) as f32 * cell_mm;
            for c in 0..=(col_end - col_start) {
                let x = grid_origin_x + c as f32 * cell_mm;
                layer.add_line(Line {
                    points: vec![
                        (Point::new(Mm(x), Mm(grid_top_y - tile_h)), false),
                        (Point::new(Mm(x), Mm(grid_top_y)), false),
                    ],
                    is_closed: false,
                });
            }
            for r in 0..=(rft_end - rft_start) {
                let y = grid_top_y - r as f32 * cell_mm;
                layer.add_line(Line {
                    points: vec![
                        (Point::new(Mm(grid_origin_x), Mm(y)), false),
                        (Point::new(Mm(grid_origin_x + tile_w), Mm(y)), false),
                    ],
                    is_closed: false,
                });
            }

            layer.set_fill_color(Color::Rgb(Rgb::new(0.3, 0.3, 0.3, None)));
            for (local_x, gx) in (col_start..col_end).enumerate() {
                if gx % AXIS_LABEL_INTERVAL == 0 {
                    let x = grid_origin_x + local_x as f32 * cell_mm;
                    layer.use_text(
                        format!("{}", gx + 1),
                        6.0,
                        Mm(x),
                        Mm(grid_top_y + 1.5),
                        &font,
                    );
                }
            }
            for (local_y, rft) in (rft_start..rft_end).enumerate() {
                let round_idx = total_rounds - 1 - rft;
                if round_idx.is_multiple_of(AXIS_LABEL_INTERVAL) {
                    let y = grid_top_y - local_y as f32 * cell_mm;
                    layer.use_text(
                        format!("R{}", round_idx + 1),
                        6.0,
                        Mm(grid_origin_x - LABEL_STRIP_MM + 1.0),
                        Mm(y - cell_mm / 2.0),
                        &font,
                    );
                }
            }

            let round_lo = total_rounds - rft_end;
            let round_hi = total_rounds - 1 - rft_start;
            layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));
            layer.use_text(
                format!(
                    "{pattern_name} - page (row {}, col {}) of ({} x {}) - stitches {}-{}, rounds {}-{}",
                    py + 1, px + 1, plan.pages_y, plan.pages_x,
                    col_start + 1, col_end, round_lo + 1, round_hi + 1,
                ),
                8.0,
                Mm(margin_mm),
                Mm(margin_mm / 2.0),
                &font,
            );
        }
    }

    // Key page: tension-color meaning, plus any explicit per-stitch colors used.
    let (key_page, key_layer_idx) = doc.add_page(Mm(page.width_mm), Mm(page.height_mm), "Layer 1");
    let layer = doc.get_page(key_page).get_layer(key_layer_idx);
    layer.use_text(
        format!("{pattern_name} - key"),
        14.0,
        Mm(margin_mm),
        Mm(page.height_mm - margin_mm),
        &font_bold,
    );
    let tension_rows: [(&str, (f32, f32, f32)); 3] = [
        ("Normal tension", (0.15, 0.55, 0.15)),
        ("Loose", (0.15, 0.35, 0.75)),
        ("Stretched", (0.75, 0.15, 0.15)),
    ];
    for (i, (label, (r, g, b))) in tension_rows.iter().enumerate() {
        let y = page.height_mm - margin_mm - 15.0 - (i as f32) * 8.0;
        layer.set_fill_color(Color::Rgb(Rgb::new(*r, *g, *b, None)));
        layer.use_text(*label, 11.0, Mm(margin_mm), Mm(y), &font);
    }

    let mut custom_colors: Vec<[u8; 3]> = Vec::new();
    for node in graph.graph.node_weights() {
        if let Some(c) = node.color {
            if !custom_colors.contains(&c) {
                custom_colors.push(c);
            }
        }
    }
    if !custom_colors.is_empty() {
        let start_y = page.height_mm - margin_mm - 15.0 - (tension_rows.len() as f32) * 8.0 - 8.0;
        layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));
        layer.use_text(
            "Explicit stitch colors:",
            11.0,
            Mm(margin_mm),
            Mm(start_y),
            &font_bold,
        );
        for (i, c) in custom_colors.iter().enumerate() {
            let y = start_y - 8.0 - (i as f32) * 8.0;
            let name = abyssal_thread_export::nearest_color_name(*c);
            layer.set_fill_color(Color::Rgb(Rgb::new(
                c[0] as f32 / 255.0,
                c[1] as f32 / 255.0,
                c[2] as f32 / 255.0,
                None,
            )));
            let rect = Rect::new(Mm(margin_mm), Mm(y - 5.0), Mm(margin_mm + 6.0), Mm(y + 1.0))
                .with_mode(printpdf::path::PaintMode::Fill);
            layer.add_rect(rect);
            layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));
            layer.use_text(
                format!("#{:02x}{:02x}{:02x} (~ {name})", c[0], c[1], c[2]),
                10.0,
                Mm(margin_mm + 9.0),
                Mm(y - 3.0),
                &font,
            );
        }
    }

    doc.save(&mut BufWriter::new(File::create(out_path)?))?;
    Ok(())
}

pub fn print_shaped_via_system_default(
    graph: &StitchGraph,
    cell_mm: f32,
    pattern_name: &str,
    page: PageSize,
    margin_mm: f32,
) -> anyhow::Result<()> {
    let mut path = std::env::temp_dir();
    let cleaned: String = pattern_name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    path.push(format!(
        "{}_shaped_print.pdf",
        if cleaned.is_empty() {
            "pattern".to_string()
        } else {
            cleaned
        }
    ));
    generate_shaped_pattern_pdf(graph, cell_mm, pattern_name, &path, page, margin_mm)?;
    opener::open(&path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use abyssal_thread_core::graph::StitchEdge;
    use abyssal_thread_core::StitchKind;

    #[test]
    fn explicit_color_overrides_tension() {
        // An explicit `~RRGGBB` color should win over tension coloring
        // entirely - a stitch shouldn't lose its actual assigned color
        // just because it also happens to be flagged loose/stretched.
        let (r, g, b) = stitch_print_rgb(Some([255, 0, 0]), Some(TensionState::Stretched));
        assert_eq!((r, g, b), (1.0, 0.0, 0.0));
    }

    #[test]
    fn tension_states_map_to_distinct_colors() {
        let normal = stitch_print_rgb(None, Some(TensionState::Normal));
        let loose = stitch_print_rgb(None, Some(TensionState::Loose));
        let stretched = stitch_print_rgb(None, Some(TensionState::Stretched));
        // All three distinct from each other...
        assert_ne!(normal, loose);
        assert_ne!(normal, stretched);
        assert_ne!(loose, stretched);
        // ...and from the "no tension analyzed" fallback.
        let none = stitch_print_rgb(None, None);
        assert_ne!(normal, none);
        assert_ne!(loose, none);
        assert_ne!(stretched, none);
    }

    #[test]
    fn no_color_and_no_tension_is_black() {
        // Deliberately black (not viewport.rs's on-screen gray) - see the
        // function's own doc comment for why: gray text is hard to read
        // on white paper.
        assert_eq!(stitch_print_rgb(None, None), (0.0, 0.0, 0.0));
    }

    /// Builds a tiny but real, connected 3-round shaped pattern (a magic
    /// ring -> increase round -> plain round), positions it with the
    /// actual layout engine, and analyzes tension - so the PDF generator
    /// below is exercised against a graph shaped exactly like a real
    /// pattern's output, not a hand-rolled one that happens to avoid edge
    /// cases the real pipeline hits (an empty round, a missing position, etc).
    fn small_test_graph() -> StitchGraph {
        let pattern = abyssal_thread_lang::parser::parse("6sc\n(sc, inc) * 6\n12sc\n")
            .expect("fixture pattern should parse");
        let mut g = abyssal_thread_lang::eval::eval(&pattern).expect("fixture pattern should eval");
        let gauge = abyssal_thread_layout::Gauge::default();
        abyssal_thread_layout::layout_ring(&mut g, gauge);
        abyssal_thread_layout::analyze_tension(&mut g, gauge);
        g
    }

    #[test]
    fn generates_a_real_nonempty_pdf_file() {
        // The exact kind of test that would have caught the print.rs
        // compute_tiling/TilingPlan signature mismatch (an earlier real
        // regression in the sibling colorwork print path) before a human
        // ever ran `cargo build` and hit it - actually calling the
        // generator end to end, not just its pure sub-functions.
        let g = small_test_graph();
        let dir = std::env::temp_dir();
        let out_path = dir.join(format!("abyssal_thread_test_{}.pdf", std::process::id()));

        let result = generate_shaped_pattern_pdf(
            &g,
            6.0,
            "test pattern",
            &out_path,
            PageSize::US_LETTER,
            12.7,
        );
        assert!(
            result.is_ok(),
            "generate_shaped_pattern_pdf failed: {result:?}"
        );

        let metadata = std::fs::metadata(&out_path).expect("output PDF should exist");
        // A real multi-page PDF (grid pages + a key page) is always at
        // least a few KB - a near-empty file would mean generation
        // silently produced a broken/truncated document.
        assert!(
            metadata.len() > 500,
            "output PDF is suspiciously small: {} bytes",
            metadata.len()
        );

        let _ = std::fs::remove_file(&out_path);
    }

    #[test]
    fn handles_a_graph_with_no_explicit_colors() {
        // Regression guard for the "Explicit stitch colors:" section,
        // which should just not appear (not panic on an empty Vec) when
        // no stitch has a `~RRGGBB` color set.
        let g = small_test_graph();
        assert!(g.graph.node_weights().all(|n| n.color.is_none()));
        let dir = std::env::temp_dir();
        let out_path = dir.join(format!(
            "abyssal_thread_test_nocolor_{}.pdf",
            std::process::id()
        ));
        let result =
            generate_shaped_pattern_pdf(&g, 6.0, "no color", &out_path, PageSize::US_LETTER, 12.7);
        assert!(
            result.is_ok(),
            "generate_shaped_pattern_pdf failed: {result:?}"
        );
        let _ = std::fs::remove_file(&out_path);
    }

    #[test]
    fn handles_an_explicit_per_stitch_color() {
        // Round-trips a graph through the ~RRGGBB path (StitchNode::color)
        // to exercise the "Explicit stitch colors:" key-page section too.
        let mut g = StitchGraph::new();
        let a = g.add_stitch(StitchKind::SingleCrochet, 0, None);
        g.set_color(a, [255, 0, 0]);
        let b = g.add_stitch(StitchKind::SingleCrochet, 0, None);
        g.connect(a, b, StitchEdge::Sequence);
        let gauge = abyssal_thread_layout::Gauge::default();
        abyssal_thread_layout::layout_ring(&mut g, gauge);

        let dir = std::env::temp_dir();
        let out_path = dir.join(format!(
            "abyssal_thread_test_color_{}.pdf",
            std::process::id()
        ));
        let result = generate_shaped_pattern_pdf(
            &g,
            6.0,
            "with color",
            &out_path,
            PageSize::US_LETTER,
            12.7,
        );
        assert!(
            result.is_ok(),
            "generate_shaped_pattern_pdf failed: {result:?}"
        );
        let _ = std::fs::remove_file(&out_path);
    }
}
