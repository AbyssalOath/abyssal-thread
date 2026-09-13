//! Filet crochet math: foundation chain length and written block/space
//! row instructions for a two-color `ColorGrid` (one cell = one mesh
//! square, worked as a double-crochet net rather than solid fabric).
//!
//! A filet mesh is NOT routed through `StitchGraph` the way solid
//! colorwork is (see `abyssal_thread_core::colorwork`) - that bridge
//! models one single crochet per cell, which is the wrong stitch
//! structure entirely for an open dc+chain net, so filet stays a
//! `ColorGrid`-only feature: this module's two functions are its whole
//! pattern-generation surface.

use abyssal_thread_core::ColorGrid;

/// The turning chain counted as the first row's first double crochet,
/// per standard filet convention.
pub const TURNING_CHAIN: usize = 3;

/// Foundation chain length for a filet mesh `width` blocks/spaces wide.
/// Each chart column consumes 3 stitches in the foundation row (block and
/// space are both 3 stitches wide; adjacent columns share their edge
/// stitch), plus 1 to close out the mesh, plus the turning chain that
/// becomes the first double crochet of row 1.
pub fn starting_chain(width: usize) -> usize {
    3 * width + 1 + TURNING_CHAIN
}

/// One run of same-type cells within a row, e.g. "3 blocks" or "2 spaces".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub filled: bool,
    pub count: usize,
}

/// Run-length-encodes chart row `y` into alternating block/space runs,
/// comparing each cell against `block_color` exactly - a filet grid should
/// only ever contain the two colors its mode locked it to.
fn encode_row(grid: &ColorGrid, y: usize, block_color: [u8; 3]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for x in 0..grid.width {
        let filled = grid.get(x, y) == block_color;
        match runs.last_mut() {
            Some(run) if run.filled == filled => run.count += 1,
            _ => runs.push(Run { filled, count: 1 }),
        }
    }
    runs
}

/// Written row-by-row instructions in standard filet shorthand: the
/// foundation chain, then each row's runs as "<n>B"/"<n>SP", one row per
/// chart row in the order the `ColorGrid` stores them (row 0 first,
/// matching every other consumer of this grid - see the module doc on
/// `abyssal_thread_core::ColorGrid`).
///
/// Simplification: each row's runs are listed in left-to-right charted
/// order regardless of which physical direction that row is crocheted.
/// Filet is worked flat (turning at the end of every row), so on paper
/// some designers reverse the text for wrong-side rows to match hook
/// direction - this always gives the chart-reading order and leaves that
/// adjustment to the crocheter, rather than guessing a convention.
pub fn write_instructions(grid: &ColorGrid, block_color: [u8; 3]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Foundation chain: {} (a ch-{TURNING_CHAIN} at the start of row 1 counts as its first dc)\n\n",
        starting_chain(grid.width)
    ));
    for y in 0..grid.height {
        let runs = encode_row(grid, y, block_color);
        let text: Vec<String> = runs
            .iter()
            .map(|r| format!("{} {}", r.count, if r.filled { "B" } else { "SP" }))
            .collect();
        out.push_str(&format!("Row {}: {}\n", y + 1, text.join(", ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: [u8; 3] = [0, 0, 0];
    const SPACE: [u8; 3] = [255, 255, 255];

    #[test]
    fn starting_chain_accounts_for_mesh_width_plus_closing_and_turning_chain() {
        // 5 columns: 3*5 + 1 (close the mesh) + 3 (turning chain) = 19.
        assert_eq!(starting_chain(5), 19);
    }

    #[test]
    fn write_instructions_encodes_runs_and_leading_chain() {
        let mut grid = ColorGrid::new(5, 1, SPACE);
        grid.set(0, 0, BLOCK);
        grid.set(1, 0, BLOCK);
        grid.set(2, 0, BLOCK);
        // cells 3,4 stay SPACE
        let text = write_instructions(&grid, BLOCK);
        assert!(text.contains("Foundation chain: 19"));
        assert!(text.contains("Row 1: 3 B, 2 SP"));
    }

    #[test]
    fn write_instructions_handles_a_row_that_is_all_one_type() {
        let grid = ColorGrid::new(4, 1, SPACE);
        let text = write_instructions(&grid, BLOCK);
        assert!(text.contains("Row 1: 4 SP"));
    }

    #[test]
    fn write_instructions_lists_rows_in_stored_grid_order() {
        let mut grid = ColorGrid::new(1, 2, SPACE);
        grid.set(0, 0, BLOCK);
        grid.set(0, 1, SPACE);
        let text = write_instructions(&grid, BLOCK);
        let lines: Vec<&str> = text.lines().filter(|l| l.starts_with("Row")).collect();
        assert_eq!(lines[0], "Row 1: 1 B");
        assert_eq!(lines[1], "Row 2: 1 SP");
    }
}
