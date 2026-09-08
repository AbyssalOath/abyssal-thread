//! Renders a `ColorGrid` (from `abyssal-thread-imageimport`) as a graphgan-
//! style SVG chart: one solid-colored square per stitch, laid out in a flat
//! width x height grid.
//!
//! Row 0 is drawn at the TOP, matching the source image's row order
//! directly - this is deliberately different from `svg::export_svg_chart`'s
//! bottom-to-top convention. That convention exists there because DSL rounds
//! are genuinely listed in the order they're crocheted; here, row 0 is just
//! "the top row of the photo," and a chart is only useful if it visually
//! matches the picture. (An earlier version of this function borrowed the
//! bottom-to-top flip by analogy and it silently turned every imported
//! image upside down - caught by comparing the rendered chart against the
//! source photo, not by any test, which is why the regression test below
//! checks pixel *positions* rather than just "some legend text exists".)
//! If you actually crochet a graphgan bottom-up, flip your source image
//! vertically before importing rather than expecting this export to do it.

use abyssal_thread_core::ColorGrid;

const CELL: f32 = 14.0;

pub fn export_color_chart_svg(grid: &ColorGrid) -> String {
    let width = grid.width as f32 * CELL;
    let height = grid.height as f32 * CELL;
    let mut body = String::new();

    for y in 0..grid.height {
        for x in 0..grid.width {
            let [r, g, b] = grid.get(x, y);
            body.push_str(&format!(
                r##"<rect x="{sx}" y="{sy}" width="{s}" height="{s}" fill="rgb({r},{g},{b})" stroke="#00000022" stroke-width="0.5"/>"##,
                sx = x as f32 * CELL,
                sy = y as f32 * CELL,
                s = CELL,
            ));
            body.push('\n');
        }
    }

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}">
<rect width="100%" height="100%" fill="white"/>
{body}</svg>"#
    )
}

/// A plain-text row-by-row legend export (one letter per color). Rows are
/// listed top-to-bottom, matching the source image and the SVG chart above -
/// see the module doc for why this does NOT flip to a bottom-to-top
/// "as worked" order the way the DSL chart's legend would.
pub fn export_color_grid_legend(grid: &ColorGrid) -> String {
    let palette = grid.palette();
    let symbol_for = |c: [u8; 3]| -> char {
        let idx = palette.iter().position(|&p| p == c).unwrap_or(0);
        (b'A' + (idx % 26) as u8) as char
    };

    let mut out = String::new();
    out.push_str("Legend:\n");
    for (i, c) in palette.iter().enumerate() {
        let symbol = (b'A' + (i % 26) as u8) as char;
        let name = crate::color_names::nearest_color_name(*c);
        out.push_str(&format!(
            "  {symbol} = #{:02x}{:02x}{:02x}  (~ {name})\n",
            c[0], c[1], c[2]
        ));
    }
    out.push_str(&format!(
        "\n{} stitches wide x {} rows (top-to-bottom, matching the source image):\n\n",
        grid.width, grid.height
    ));

    for y in 0..grid.height {
        let row: String = (0..grid.width).map(|x| symbol_for(grid.get(x, y))).collect();
        out.push_str(&format!("Row {}: {row}\n", y + 1));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legend_lists_each_distinct_color_once() {
        let mut grid = ColorGrid::new(2, 1, [255, 255, 255]);
        grid.set(1, 0, [0, 0, 0]);
        let legend = export_color_grid_legend(&grid);
        assert!(legend.contains("A = #ffffff"));
        assert!(legend.contains("B = #000000"));
        assert!(legend.contains("Row 1: AB"));
    }

    #[test]
    fn legend_row_order_matches_image_top_to_bottom_not_flipped() {
        // Regression test for the upside-down bug: row 0 (grid's first row)
        // must print as "Row 1" and keep its own pixel content, not get
        // swapped with the last row.
        let mut grid = ColorGrid::new(1, 2, [255, 255, 255]); // white
        grid.set(0, 0, [255, 0, 0]); // top pixel red
        grid.set(0, 1, [0, 0, 255]); // bottom pixel blue
        let legend = export_color_grid_legend(&grid);
        let lines: Vec<&str> = legend.lines().filter(|l| l.starts_with("Row")).collect();
        assert_eq!(lines[0], "Row 1: A"); // top row -> red -> first color seen -> 'A'
        assert_eq!(lines[1], "Row 2: B"); // bottom row -> blue -> 'B'
    }

    #[test]
    fn svg_places_row_zero_at_the_top_not_the_bottom() {
        let mut grid = ColorGrid::new(1, 2, [255, 255, 255]);
        grid.set(0, 0, [255, 0, 0]); // row 0 (top of source image) = red
        grid.set(0, 1, [0, 0, 255]); // row 1 (bottom of source image) = blue
        let svg = export_color_chart_svg(&grid);
        // CELL = 14.0, so row 0 must be at y="0" and row 1 at y="14" - if
        // the flip bug reappears, red would be at y="14" instead.
        assert!(svg.contains(r#"y="0" width="14" height="14" fill="rgb(255,0,0)""#));
        assert!(svg.contains(r#"y="14" width="14" height="14" fill="rgb(0,0,255)""#));
    }
}

