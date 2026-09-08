//! Serializes a `ColorGrid` (from image import) into DSL text using the
//! `COLORGRID: WxH` / `ROW n: <hex> <hex> ...` block (see
//! `lexer::Token::ColorGridHeader`/`ColorRow` and `parser::parse_pattern`
//! for the reader side). This is what lets an image-imported colorwork
//! pattern show up as real, re-parseable text in the DSL tab.

use abyssal_thread_core::ColorGrid;

pub fn color_grid_to_dsl(name: Option<&str>, grid: &ColorGrid) -> String {
    let mut out = String::new();
    if let Some(n) = name {
        out.push_str(&format!("PATTERN: {n}\n"));
    }
    out.push_str(&format!("COLORGRID: {}x{}\n", grid.width, grid.height));
    for y in 0..grid.height {
        let hex: Vec<String> = (0..grid.width)
            .map(|x| {
                let [r, g, b] = grid.get(x, y);
                format!("{r:02x}{g:02x}{b:02x}")
            })
            .collect();
        out.push_str(&format!("ROW {}: {}\n", y + 1, hex.join(" ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{eval, parser};

    #[test]
    fn round_trips_through_parse_and_eval() {
        let mut grid = ColorGrid::new(3, 2, [255, 255, 255]);
        grid.set(1, 0, [200, 30, 30]);
        grid.set(2, 1, [10, 20, 30]);

        let dsl = color_grid_to_dsl(Some("test"), &grid);
        let pattern = parser::parse(&dsl).expect("re-parse should succeed");
        assert_eq!(pattern.name.as_deref(), Some("test"));
        let parsed_grid = pattern.color_grid.clone().expect("color_grid should be present");
        assert_eq!(parsed_grid.width, 3);
        assert_eq!(parsed_grid.height, 2);
        assert_eq!(parsed_grid.get(1, 0), [200, 30, 30]);
        assert_eq!(parsed_grid.get(2, 1), [10, 20, 30]);

        let g = eval::eval(&pattern).expect("eval should succeed");
        assert_eq!(g.stitch_count(), 6);
        assert_eq!(g.round_count(), 2);
    }
}
