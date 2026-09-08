use crate::graph::{StitchEdge, StitchGraph};
use crate::stitch::StitchKind;
use crate::ColorGrid;

impl StitchGraph {
    /// Builds a flat-row stitch graph from a `ColorGrid`: one single crochet
    /// per pixel, one round per image row. Each stitch attaches (`Parent`
    /// edge) straight down into the same-column stitch of the row below -
    /// row 0 has no parent, since it's the foundation row. This is
    /// deliberately NOT the circular/tapered layout shaped patterns use
    /// (see `abyssal_thread_layout::layout_flat_grid` vs. `layout_ring`): a
    /// colorwork panel is worked flat, not in the round.
    ///
    /// This is the bridge that lets an image-imported colorwork pattern
    /// flow through the exact same `StitchGraph` -> layout -> tension ->
    /// export pipeline that shaped DSL patterns use, and lets the grid/DSL/
    /// 3D views all show the same underlying data.
    pub fn from_color_grid(grid: &ColorGrid) -> StitchGraph {
        let mut g = StitchGraph::new();
        let mut prev_row: Vec<petgraph::graph::NodeIndex> = Vec::new();

        for y in 0..grid.height {
            let mut this_row = Vec::with_capacity(grid.width);
            let mut last = None;
            for x in 0..grid.width {
                let idx = g.add_stitch(StitchKind::SingleCrochet, y, None);
                g.set_color(idx, grid.get(x, y));
                if let Some(prev) = last {
                    g.connect(prev, idx, StitchEdge::Sequence);
                }
                last = Some(idx);
                if let Some(&parent) = prev_row.get(x) {
                    g.connect(parent, idx, StitchEdge::Parent);
                }
                this_row.push(idx);
            }
            prev_row = this_row;
        }
        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_one_stitch_per_pixel_with_matching_color() {
        let mut grid = ColorGrid::new(3, 2, [255, 255, 255]);
        grid.set(1, 0, [200, 30, 30]);
        let g = StitchGraph::from_color_grid(&grid);
        assert_eq!(g.stitch_count(), 6);
        assert_eq!(g.round_count(), 2);
        assert_eq!(g.rounds[0].len(), 3);
        let colored_node = g.rounds[0][1];
        assert_eq!(g.graph[colored_node].color, Some([200, 30, 30]));
    }

    #[test]
    fn row_zero_has_no_parents_later_rows_attach_straight_down() {
        let grid = ColorGrid::new(2, 2, [0, 0, 0]);
        let g = StitchGraph::from_color_grid(&grid);
        for &idx in &g.rounds[0] {
            assert_eq!(g.parent_of(idx), None);
        }
        for (col, &idx) in g.rounds[1].iter().enumerate() {
            assert_eq!(g.parent_of(idx), Some(g.rounds[0][col]));
        }
    }
}
