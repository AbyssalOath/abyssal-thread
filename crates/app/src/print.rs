//! Generates a print-ready, multi-page PDF of a colorwork chart, tiled
//! across US Letter pages the way cross-stitch/graphgan patterns
//! traditionally are: each page shows a portion of the grid plus global
//! row/column reference numbers along its edges, so pages can be lined up
//! and taped together and you can trace which stitch is which across a
//! seam. A final legend page lists hex codes + nearest color names.
//!
//! There's no cross-platform Rust API for "send this to whatever printer
//! the user has" without separate native code per OS (Windows print
//! spooler, macOS print panel, CUPS on Linux). The standard, reliable way
//! around that - and what this does - is: generate a real PDF, then hand
//! it to the OS's default PDF viewer via `opener::open`, which already has
//! a proper Print button. `print_via_system_default` is that hand-off;
//! everything else in this file is pure PDF generation you can also save
//! directly via `generate_pattern_pdf` (see the "Export PDF..." toolbar button).

use abyssal_thread_core::ColorGrid;
use printpdf::*;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

const LABEL_STRIP_MM: f32 = 6.0; // room for row/col index numbers
const FOOTER_STRIP_MM: f32 = 10.0; // room for the "page R,C of RxC" caption
/// Print a reference number every N cells along each tile's edges - dense
/// enough to align tiles confidently, sparse enough not to clutter a page.
const AXIS_LABEL_INTERVAL: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width_mm: f32,
    pub height_mm: f32,
}
impl PageSize {
    pub const US_LETTER: PageSize = PageSize {
        width_mm: 215.9,
        height_mm: 279.4,
    };
    pub const A4: PageSize = PageSize {
        width_mm: 210.0,
        height_mm: 297.0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TilingPlan {
    pub cells_per_page_x: usize,
    pub cells_per_page_y: usize,
    pub pages_x: usize,
    pub pages_y: usize,
}

/// Pure sizing math, kept separate from PDF generation so it's testable
/// without needing to inspect PDF bytes - this is the part most likely to
/// have an off-by-one bug, so it gets its own tests below.
pub fn compute_tiling(
    grid_width: usize,
    grid_height: usize,
    cell_mm: f32,
    page: PageSize,
    margin_mm: f32,
) -> TilingPlan {
    let usable_w = page.width_mm - 2.0 * margin_mm - LABEL_STRIP_MM;
    let usable_h = page.height_mm - 2.0 * margin_mm - LABEL_STRIP_MM - FOOTER_STRIP_MM;
    let cells_per_page_x = ((usable_w / cell_mm).floor() as usize).max(1);
    let cells_per_page_y = ((usable_h / cell_mm).floor() as usize).max(1);
    let pages_x = grid_width.div_ceil(cells_per_page_x);
    let pages_y = grid_height.div_ceil(cells_per_page_y);
    TilingPlan {
        cells_per_page_x,
        cells_per_page_y,
        pages_x,
        pages_y,
    }
}

/// Generates the full tiled pattern PDF (grid pages + one legend page, plus
/// paginated written-instructions pages when `instructions` is `Some` - the
/// filet-mode case, via `abyssal_thread_export::write_filet_instructions`)
/// and writes it to `out_path`.
pub fn generate_pattern_pdf(
    grid: &ColorGrid,
    cell_mm: f32,
    pattern_name: &str,
    out_path: &Path,
    page: PageSize,
    margin_mm: f32,
    instructions: Option<&str>,
) -> anyhow::Result<()> {
    let plan = compute_tiling(grid.width, grid.height, cell_mm, page, margin_mm);

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

            // A visible reminder on the very first page only: the PDF
            // itself is genuine full color (verified directly in its
            // content stream - real distinct R/G/B values, not grayscale
            // operators), so if a printed copy comes out black-and-white,
            // it's the print dialog or printer driver defaulting to
            // Grayscale/Black & White (a very common cost-saving default)
            // rather than anything wrong with this file. There's no way
            // for a PDF to override that setting itself - it's a print-time
            // choice, not a document property - so a note is the best fix
            // available here.
            if py == 0 && px == 0 {
                layer.set_fill_color(Color::Rgb(Rgb::new(0.7, 0.1, 0.1, None)));
                layer.use_text(
                    "Tip: select COLOR (not Grayscale / Black & White) in your print dialog.",
                    9.0,
                    Mm(grid_origin_x),
                    Mm(page.height_mm - margin_mm + 1.0),
                    &font,
                );
            }

            let col_start = px * plan.cells_per_page_x;
            let col_end = (col_start + plan.cells_per_page_x).min(grid.width);
            let row_start = py * plan.cells_per_page_y;
            let row_end = (row_start + plan.cells_per_page_y).min(grid.height);

            // Cells - row 0 is the top of the source image, drawn at the
            // top of the page (no flip), consistent with export_color_chart_svg.
            for (local_y, gy) in (row_start..row_end).enumerate() {
                for (local_x, gx) in (col_start..col_end).enumerate() {
                    let [r, g, b] = grid.get(gx, gy);
                    let x0 = grid_origin_x + local_x as f32 * cell_mm;
                    let y0 = grid_top_y - local_y as f32 * cell_mm;
                    layer.set_fill_color(Color::Rgb(Rgb::new(
                        r as f32 / 255.0,
                        g as f32 / 255.0,
                        b as f32 / 255.0,
                        None,
                    )));
                    let rect = Rect::new(Mm(x0), Mm(y0 - cell_mm), Mm(x0 + cell_mm), Mm(y0))
                        .with_mode(printpdf::path::PaintMode::Fill);
                    layer.add_rect(rect);
                }
            }

            // Grid lines.
            layer.set_outline_color(Color::Rgb(Rgb::new(0.6, 0.6, 0.6, None)));
            layer.set_outline_thickness(0.3);
            let tile_w = (col_end - col_start) as f32 * cell_mm;
            let tile_h = (row_end - row_start) as f32 * cell_mm;
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
            for r in 0..=(row_end - row_start) {
                let y = grid_top_y - r as f32 * cell_mm;
                layer.add_line(Line {
                    points: vec![
                        (Point::new(Mm(grid_origin_x), Mm(y)), false),
                        (Point::new(Mm(grid_origin_x + tile_w), Mm(y)), false),
                    ],
                    is_closed: false,
                });
            }

            // Global row/column reference numbers, so tiles can be aligned
            // and traced across page boundaries when taped together.
            layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));
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
            for (local_y, gy) in (row_start..row_end).enumerate() {
                if gy % AXIS_LABEL_INTERVAL == 0 {
                    let y = grid_top_y - local_y as f32 * cell_mm;
                    layer.use_text(
                        format!("{}", gy + 1),
                        6.0,
                        Mm(grid_origin_x - LABEL_STRIP_MM + 1.0),
                        Mm(y - cell_mm / 2.0),
                        &font,
                    );
                }
            }

            // Footer: which page this is, and exactly which stitches/rows
            // it covers (handy even without the axis numbers).
            layer.use_text(
                format!(
                    "{pattern_name} - page (row {}, col {}) of ({} x {}) - stitches {}-{}, rows {}-{}",
                    py + 1,
                    px + 1,
                    plan.pages_y,
                    plan.pages_x,
                    col_start + 1,
                    col_end,
                    row_start + 1,
                    row_end,
                ),
                8.0,
                Mm(margin_mm),
                Mm(margin_mm / 2.0),
                &font,
            );
        }
    }

    // Legend page.
    let (legend_page, legend_layer_idx) =
        doc.add_page(Mm(page.width_mm), Mm(page.height_mm), "Layer 1");
    let layer = doc.get_page(legend_page).get_layer(legend_layer_idx);
    layer.use_text(
        format!("{pattern_name} - color legend"),
        14.0,
        Mm(margin_mm),
        Mm(page.height_mm - margin_mm),
        &font_bold,
    );
    for (i, c) in grid.palette().iter().enumerate() {
        let y = page.height_mm - margin_mm - 15.0 - (i as f32) * 10.0;
        layer.set_fill_color(Color::Rgb(Rgb::new(
            c[0] as f32 / 255.0,
            c[1] as f32 / 255.0,
            c[2] as f32 / 255.0,
            None,
        )));
        let rect = Rect::new(Mm(margin_mm), Mm(y - 7.0), Mm(margin_mm + 8.0), Mm(y + 1.0))
            .with_mode(printpdf::path::PaintMode::Fill);
        layer.add_rect(rect);
        layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));
        let symbol = (b'A' + (i % 26) as u8) as char;
        let name = abyssal_thread_export::nearest_color_name(*c);
        layer.use_text(
            format!(
                "{symbol} = #{:02x}{:02x}{:02x} (~ {name})",
                c[0], c[1], c[2]
            ),
            10.0,
            Mm(margin_mm + 12.0),
            Mm(y - 4.0),
            &font,
        );
    }

    if let Some(text) = instructions {
        add_instructions_pages(&doc, &font, &font_bold, pattern_name, text, page, margin_mm);
    }

    doc.save(&mut BufWriter::new(File::create(out_path)?))?;
    Ok(())
}

const INSTRUCTIONS_FONT_SIZE: f32 = 9.0;
const INSTRUCTIONS_LINE_HEIGHT_MM: f32 = 5.5;
const INSTRUCTIONS_HEADING_SPACE_MM: f32 = 15.0;

/// Adds as many pages as needed to lay out `text` (one PDF text line per
/// input line - callers are expected to have already wrapped/formatted it,
/// as `write_filet_instructions` does with one line per pattern row) below
/// a heading, continuing onto further pages once a page's line budget
/// runs out.
fn add_instructions_pages(
    doc: &PdfDocumentReference,
    font: &IndirectFontRef,
    font_bold: &IndirectFontRef,
    pattern_name: &str,
    text: &str,
    page: PageSize,
    margin_mm: f32,
) {
    let lines: Vec<&str> = text.lines().collect();
    let usable_h = page.height_mm - 2.0 * margin_mm - INSTRUCTIONS_HEADING_SPACE_MM;
    let lines_per_page = ((usable_h / INSTRUCTIONS_LINE_HEIGHT_MM).floor() as usize).max(1);

    for (page_idx, chunk) in lines.chunks(lines_per_page).enumerate() {
        let (p, l) = doc.add_page(Mm(page.width_mm), Mm(page.height_mm), "Layer 1");
        let layer = doc.get_page(p).get_layer(l);
        let heading = if page_idx == 0 {
            format!("{pattern_name} - written instructions")
        } else {
            format!("{pattern_name} - written instructions (cont.)")
        };
        layer.use_text(
            heading,
            14.0,
            Mm(margin_mm),
            Mm(page.height_mm - margin_mm),
            font_bold,
        );
        for (i, line) in chunk.iter().enumerate() {
            let y = page.height_mm
                - margin_mm
                - INSTRUCTIONS_HEADING_SPACE_MM
                - (i as f32) * INSTRUCTIONS_LINE_HEIGHT_MM;
            layer.use_text(*line, INSTRUCTIONS_FONT_SIZE, Mm(margin_mm), Mm(y), font);
        }
    }
}

/// Writes the PDF to a temp file and opens it with the OS's default PDF
/// viewer - from there, the user hits that viewer's own Print button. This
/// is the "Print..." toolbar action; `generate_pattern_pdf` alone (writing
/// to a user-chosen path) is the "Export PDF..." action.
pub fn print_via_system_default(
    grid: &ColorGrid,
    cell_mm: f32,
    pattern_name: &str,
    page: PageSize,
    margin_mm: f32,
    instructions: Option<&str>,
) -> anyhow::Result<()> {
    let mut path = std::env::temp_dir();
    path.push(format!("{}_print.pdf", sanitize_filename(pattern_name)));
    generate_pattern_pdf(
        grid,
        cell_mm,
        pattern_name,
        &path,
        page,
        margin_mm,
        instructions,
    )?;
    opener::open(&path)?;
    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "pattern".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiling_matches_known_good_layouts() {
        // These exact numbers were verified visually against real rendered
        // PDF output (grid lines, axis labels, and footer captions all
        // checked by eye) before being pinned here as a regression test.
        // 12.7mm (0.5in) margin, matching the GUI's default.
        assert_eq!(
            compute_tiling(60, 32, 6.0, PageSize::US_LETTER, 12.7),
            TilingPlan {
                cells_per_page_x: 30,
                cells_per_page_y: 39,
                pages_x: 2,
                pages_y: 1
            }
        );
        assert_eq!(
            compute_tiling(150, 97, 6.0, PageSize::US_LETTER, 12.7),
            TilingPlan {
                cells_per_page_x: 30,
                cells_per_page_y: 39,
                pages_x: 5,
                pages_y: 3
            }
        );
    }

    #[test]
    fn tiling_covers_every_cell_exactly_once_with_no_gaps() {
        for (w, h, cell_mm) in [(60, 32, 6.0), (150, 97, 6.0), (13, 200, 5.0), (1, 1, 6.0)] {
            let plan = compute_tiling(w, h, cell_mm, PageSize::US_LETTER, 12.7);
            let covered_w = plan.pages_x * plan.cells_per_page_x;
            let covered_h = plan.pages_y * plan.cells_per_page_y;
            assert!(
                covered_w >= w,
                "tiles must cover the full width ({w}), got {covered_w}"
            );
            assert!(
                covered_h >= h,
                "tiles must cover the full height ({h}), got {covered_h}"
            );
            // No more than one extra tile's worth of slack in either direction.
            assert!(covered_w - w < plan.cells_per_page_x);
            assert!(covered_h - h < plan.cells_per_page_y);
        }
    }

    #[test]
    fn sanitize_filename_strips_unsafe_characters() {
        assert_eq!(
            sanitize_filename("merci pour le venin!"),
            "merci_pour_le_venin_"
        );
        assert_eq!(sanitize_filename(""), "pattern");
    }

    fn small_test_grid() -> ColorGrid {
        let mut grid = ColorGrid::new(4, 3, [255, 255, 255]);
        grid.set(0, 0, [0, 0, 0]);
        grid.set(1, 0, [0, 0, 0]);
        grid
    }

    #[test]
    fn generates_a_real_nonempty_pdf_file_with_instructions() {
        // Same reasoning as print_shaped.rs's equivalent test: exercises
        // `generate_pattern_pdf` end to end with a real `instructions`
        // value (the filet written-instructions path), not just
        // `add_instructions_pages`'s pure line-count math.
        let grid = small_test_grid();
        let dir = std::env::temp_dir();
        let out_path = dir.join(format!(
            "abyssal_thread_test_filet_{}.pdf",
            std::process::id()
        ));

        let result = generate_pattern_pdf(
            &grid,
            6.0,
            "test filet pattern",
            &out_path,
            PageSize::US_LETTER,
            12.7,
            Some("Foundation chain: 16\n\nRow 1: 2 B, 2 SP\nRow 2: 4 SP\nRow 3: 4 SP\n"),
        );
        assert!(result.is_ok(), "generate_pattern_pdf failed: {result:?}");

        let metadata = std::fs::metadata(&out_path).expect("output PDF should exist");
        assert!(
            metadata.len() > 500,
            "output PDF is suspiciously small: {} bytes",
            metadata.len()
        );

        let _ = std::fs::remove_file(&out_path);
    }

    #[test]
    fn omitting_instructions_still_generates_a_valid_pdf() {
        // The plain colorwork path (no filet instructions) must keep
        // working unchanged now that `instructions` is a parameter.
        let grid = small_test_grid();
        let dir = std::env::temp_dir();
        let out_path = dir.join(format!(
            "abyssal_thread_test_nocolorwork_{}.pdf",
            std::process::id()
        ));
        let result = generate_pattern_pdf(
            &grid,
            6.0,
            "no instructions",
            &out_path,
            PageSize::US_LETTER,
            12.7,
            None,
        );
        assert!(result.is_ok(), "generate_pattern_pdf failed: {result:?}");
        let _ = std::fs::remove_file(&out_path);
    }

    #[test]
    fn long_instructions_spill_onto_a_second_page() {
        let grid = small_test_grid();
        let dir = std::env::temp_dir();
        let out_path = dir.join(format!(
            "abyssal_thread_test_long_instructions_{}.pdf",
            std::process::id()
        ));
        // Comfortably more rows than fit on one US Letter page at the
        // default line height/margin - forces `add_instructions_pages`
        // to actually paginate rather than exercising only its
        // single-page path.
        let long_text: String = (1..=200).map(|n| format!("Row {n}: 4 SP\n")).collect();

        let with_instructions_len = {
            generate_pattern_pdf(
                &grid,
                6.0,
                "long instructions",
                &out_path,
                PageSize::US_LETTER,
                12.7,
                Some(&long_text),
            )
            .expect("generate_pattern_pdf failed");
            std::fs::metadata(&out_path).unwrap().len()
        };
        generate_pattern_pdf(
            &grid,
            6.0,
            "long instructions",
            &out_path,
            PageSize::US_LETTER,
            12.7,
            None,
        )
        .expect("generate_pattern_pdf failed");
        let without_instructions_len = std::fs::metadata(&out_path).unwrap().len();

        // 200 lines of text across multiple extra pages is a real,
        // substantial size difference - not just noise from one extra
        // near-empty page.
        assert!(
            with_instructions_len > without_instructions_len + 1000,
            "expected instructions pages to add meaningfully to file size: {with_instructions_len} vs {without_instructions_len}"
        );

        let _ = std::fs::remove_file(&out_path);
    }
}
