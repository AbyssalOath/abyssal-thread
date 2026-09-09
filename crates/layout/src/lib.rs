//! Placement + tension analysis for a `StitchGraph`.
//!
//! `layout_ring` is a first-pass geometry model good enough for
//! amigurumi-style rounds (flat circles, tubes, tapered spheres): every
//! stitch in a round is spread evenly around a circle whose radius is
//! derived from the round's stitch count, and each round steps up in
//! height. It is a closed-form placement, not a physics simulation - `relax`
//! below is the actual mass-spring relaxation pass that nudges it toward
//! equilibrium for asymmetric patterns the closed-form model can't
//! represent exactly on its own; real fabric drape and full collision
//! handling beyond same-round repulsion remain natural further steps.

use abyssal_thread_core::graph::TensionState;
use abyssal_thread_core::{StitchGraph, Vec3};
use std::f32::consts::PI;

/// Fraction of deviation from a stitch's baseline width before it's flagged
/// as loose/stretched. Matches the ~15% threshold CrochetPARADE documents.
pub const TENSION_THRESHOLD: f32 = 0.15;

/// The gauge implicitly baked into `StitchKind`'s current hardcoded
/// `baseline_width_mm`/`baseline_height_mm` constants. A single crochet at
/// 6.0mm wide / 5.0mm tall works out to roughly 17 stitches and 20 rows per
/// 4 inches, which is close enough to round to the same "16 sts/16 rows per
/// 4in, typical worsted weight" default already used as the starting gauge
/// everywhere else in the app (see `gui/image_import.rs`) that using it as
/// the reference point here keeps the whole app's gauge defaults
/// consistent, even though it isn't a bit-for-bit inverse of the constants.
pub const REFERENCE_STS_PER_4IN: f32 = 16.0;
pub const REFERENCE_ROWS_PER_4IN: f32 = 16.0;

/// Gauge as measured from a swatch: stitches and rows per 4 inches. Scales
/// `StitchKind::baseline_width_mm`/`baseline_height_mm` (which encode
/// worsted-weight-yarn *relative* stitch proportions - a dc is always wider
/// than a sc, etc.) up or down to match a crocheter's actual tension,
/// rather than replacing those per-stitch-type constants outright. Mirrors
/// the gauge fields already used for colorwork sizing in
/// `gui/image_import.rs`/`gui/colorwork_grid.rs`/`gui/text_import.rs`, but
/// feeds `layout_ring`/`analyze_tension` (3D layout + tension analysis)
/// instead of a pixel grid's stitch count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gauge {
    pub sts_per_4in: f32,
    pub rows_per_4in: f32,
}

impl Default for Gauge {
    /// Reproduces the untouched hardcoded baseline_width_mm/height_mm
    /// values exactly (scale factors of 1.0) - passing `Gauge::default()`
    /// anywhere `layout_ring`/`analyze_tension` used to take no gauge
    /// argument at all is a no-op change in behavior.
    fn default() -> Self {
        Gauge {
            sts_per_4in: REFERENCE_STS_PER_4IN,
            rows_per_4in: REFERENCE_ROWS_PER_4IN,
        }
    }
}

impl Gauge {
    /// Multiplier applied to `baseline_width_mm()`. A tighter gauge (more
    /// stitches per 4in than the reference) means each stitch is narrower
    /// in real life, hence the inverse ratio. Clamped away from zero/huge
    /// so a stray 0 or negative gauge value from a UI field can't collapse
    /// or blow up the layout.
    pub fn width_scale(&self) -> f32 {
        (REFERENCE_STS_PER_4IN / self.sts_per_4in.max(0.01)).clamp(0.1, 10.0)
    }

    /// Multiplier applied to `baseline_height_mm()`, same reasoning as
    /// `width_scale` but for rows.
    pub fn height_scale(&self) -> f32 {
        (REFERENCE_ROWS_PER_4IN / self.rows_per_4in.max(0.01)).clamp(0.1, 10.0)
    }
}

pub fn layout_ring(g: &mut StitchGraph, gauge: Gauge) {
    let round_count = g.rounds.len();
    for round_idx in 0..round_count {
        let node_ids = g.rounds[round_idx].clone();
        let n = node_ids.len();
        if n == 0 {
            continue;
        }

        // Radius from circumference = n * average baseline stitch width,
        // scaled by the swatch gauge (see `Gauge::width_scale`).
        let avg_width_mm: f32 = node_ids
            .iter()
            .map(|&idx| g.graph[idx].kind.baseline_width_mm())
            .sum::<f32>()
            / n as f32
            * gauge.width_scale();
        let circumference = n as f32 * avg_width_mm;
        let radius = (circumference / (2.0 * PI)).max(avg_width_mm * 0.5);

        // Height: cumulative stitch height of all prior rounds, scaled by
        // the swatch gauge (see `Gauge::height_scale`).
        let height: f32 = (0..round_idx)
            .map(|r| {
                g.rounds[r]
                    .iter()
                    .map(|&idx| g.graph[idx].kind.baseline_height_mm() * gauge.height_scale())
                    .fold(0.0_f32, f32::max)
            })
            .sum();

        for (i, &idx) in node_ids.iter().enumerate() {
            let angle = 2.0 * PI * (i as f32) / (n as f32);
            let pos = Vec3::on_circle(radius, angle, height);
            g.graph[idx].position = Some(pos);
        }
    }
}

/// Default cell size (mm) for `layout_flat_grid` - matches single crochet's
/// baseline width in `StitchKind`, so a colorwork panel's proportions read
/// the same as any other sc-based layout.
pub const FLAT_GRID_CELL_MM: f32 = 6.0;

/// Places a flat-row graph (from `StitchGraph::from_color_grid`) on a plain
/// rectangular grid in the XY plane - column * cell_mm, row * cell_mm, no
/// wrap. Deliberately separate from `layout_ring`: a colorwork panel is
/// worked flat, not tapered into a tube/sphere, so circular placement would
/// misrepresent it. Row 0 (the DSL/image's first row) is placed at y = 0;
/// pair with an SVG/3D viewer that doesn't itself re-flip y, matching the
/// same "no surprise flip" fix applied to `export_color_chart_svg`.
pub fn layout_flat_grid(g: &mut StitchGraph, cell_mm: f32) {
    let rows = g.rounds.len();
    let cols = g.rounds.iter().map(|r| r.len()).max().unwrap_or(0);
    let half_w = (cols as f32 - 1.0).max(0.0) * cell_mm * 0.5;
    let half_h = (rows as f32 - 1.0).max(0.0) * cell_mm * 0.5;
    for (row_idx, round) in g.rounds.iter().enumerate() {
        for (col_idx, &node_idx) in round.iter().enumerate() {
            let pos = Vec3::new(
                col_idx as f32 * cell_mm - half_w,
                half_h - row_idx as f32 * cell_mm,
                0.0,
            );
            g.graph[node_idx].position = Some(pos);
        }
    }
}

/// Compares each `Sequence`-connected stitch pair's actual (laid-out)
/// distance against the stitch's baseline width and flags deviations beyond
/// `TENSION_THRESHOLD`. Positions must already be set (call `layout_ring`
/// first). Pass the *same* `Gauge` used for that `layout_ring` call, or the
/// baseline this compares against won't match the gauge actual positions
/// were laid out at.
pub fn analyze_tension(g: &mut StitchGraph, gauge: Gauge) {
    use petgraph::visit::EdgeRef;

    let edges: Vec<_> = g
        .graph
        .edge_references()
        .filter(|e| *e.weight() == abyssal_thread_core::StitchEdge::Sequence)
        .map(|e| (e.source(), e.target()))
        .collect();

    for (a, b) in edges {
        let (Some(pa), Some(pb)) = (g.graph[a].position, g.graph[b].position) else {
            continue;
        };
        let actual = pa.distance(pb);
        let baseline = g.graph[a].kind.baseline_width_mm() * gauge.width_scale();
        if baseline <= 0.0 {
            continue;
        }
        let deviation = (actual - baseline) / baseline;
        let state = if deviation > TENSION_THRESHOLD {
            TensionState::Stretched
        } else if deviation < -TENSION_THRESHOLD {
            TensionState::Loose
        } else {
            TensionState::Normal
        };
        g.graph[b].tension = Some(state);
    }
}

/// Nudges `layout_ring`'s closed-form positions toward a mass-spring
/// equilibrium: `Sequence` edges are springs with rest length
/// `baseline_width_mm()` (round-to-round horizontal spacing), `Parent`
/// edges are springs with rest length `baseline_height_mm()` (row-to-row
/// rise) - the same two baseline dimensions, scaled by the same `Gauge`,
/// that `layout_ring`/`analyze_tension` already use, so a relaxed layout
/// stays consistent with what `analyze_tension` calls "normal" tension. A
/// same-round repulsion term discourages stitches from collapsing onto
/// each other, which the closed-form ring model can't misplace *into* on
/// its own but a relaxation pass otherwise could, given an asymmetric
/// pattern (uneven increases/decreases, off-center attachments) pulling
/// neighbors unevenly.
///
/// `layout_ring` is exact for symmetric rounds (perfectly even circles),
/// which is exactly the case this pass leaves alone (springs already at
/// rest length exert no force). It earns its keep on *asymmetric* patterns
/// - lopsided increase/decrease placement, `@label` attachments far from
///   the positional cursor, raw-geometry picots/bobbles pulling on a single
///   point - where the closed-form model's per-round circular symmetry
///   assumption breaks down but the spring graph still has real structure to
///   settle into.
///
/// Repulsion is scoped to *same-round* pairs only (not every pair in the
/// graph), and - since it only ever matters between stitches already
/// closer than `REPULSION_RADIUS_MM` - checked via a uniform spatial hash
/// grid rebuilt once per iteration (`RepulsionGrid`) rather than an
/// all-pairs scan. That keeps the cost roughly linear in a round's size
/// for the realistic case of stitches spread out across the round, instead
/// of the flat quadratic cost an all-pairs scan pays regardless of how
/// spread out stitches actually are - the difference that makes long flat
/// rows (scarves, blankets, big doilies - potentially hundreds of stitches
/// per round) usable interactively rather than just amigurumi-scale rounds
/// of a few dozen.
///
/// No-op if any node touched by a spring or repulsion pair has no position
/// yet - call `layout_ring` or `layout_flat_grid` first.
pub fn relax(g: &mut StitchGraph, gauge: Gauge, iterations: usize) {
    use petgraph::visit::EdgeRef;
    use std::collections::HashMap;

    if iterations == 0 {
        return;
    }

    struct Spring {
        a: petgraph::graph::NodeIndex,
        b: petgraph::graph::NodeIndex,
        rest: f32,
    }

    // Collected once - the graph's edges/kinds don't change during
    // relaxation, only positions do.
    let mut springs = Vec::new();
    for edge in g.graph.edge_references() {
        match edge.weight() {
            abyssal_thread_core::StitchEdge::Sequence => {
                let rest = g.graph[edge.source()].kind.baseline_width_mm() * gauge.width_scale();
                springs.push(Spring {
                    a: edge.source(),
                    b: edge.target(),
                    rest,
                });
            }
            abyssal_thread_core::StitchEdge::Parent => {
                let rest = g.graph[edge.target()].kind.baseline_height_mm() * gauge.height_scale();
                springs.push(Spring {
                    a: edge.source(),
                    b: edge.target(),
                    rest,
                });
            }
        }
    }

    // Fraction of a spring's length error corrected per iteration - well
    // under 1.0 so springs settle gradually across `iterations` rather than
    // overshooting and oscillating.
    const SPRING_STIFFNESS: f32 = 0.2;
    // "Personal space" radius (mm) below which same-round stitches push
    // each other apart - roughly one single-crochet width, since that's
    // the tightest two unrelated stitches should ever legitimately sit.
    const REPULSION_RADIUS_MM: f32 = 6.0;
    // Caps how far any one node can move in a single iteration, so a
    // pathological initial layout (e.g. a raw-geometry body placing a
    // stitch far from where its springs want it) settles gradually instead
    // of overshooting wildly on the first step.
    const MAX_STEP_MM: f32 = 2.0;

    for _ in 0..iterations {
        let mut displacement: HashMap<petgraph::graph::NodeIndex, Vec3> = HashMap::new();

        for spring in &springs {
            let (Some(pa), Some(pb)) = (g.graph[spring.a].position, g.graph[spring.b].position)
            else {
                continue;
            };
            let delta = pb - pa;
            let dist = delta.length();
            if dist < 1e-6 {
                continue;
            }
            let error = dist - spring.rest;
            let correction = delta.normalized().scale(error * SPRING_STIFFNESS * 0.5);
            let da = displacement.entry(spring.a).or_insert(Vec3::ZERO);
            *da = *da + correction;
            let db = displacement.entry(spring.b).or_insert(Vec3::ZERO);
            *db = *db - correction;
        }

        // Rebuilt every iteration since positions move - see
        // `RepulsionGrid`'s doc comment for why this is still cheap.
        for round in &g.rounds {
            let grid = RepulsionGrid::build(g, round, REPULSION_RADIUS_MM);
            for (a, b) in grid.candidate_pairs() {
                let (Some(pa), Some(pb)) = (g.graph[a].position, g.graph[b].position) else {
                    continue;
                };
                let delta = pb - pa;
                let dist = delta.length();
                if !(1e-6..REPULSION_RADIUS_MM).contains(&dist) {
                    continue;
                }
                let push = delta.normalized().scale((REPULSION_RADIUS_MM - dist) * 0.5);
                let da = displacement.entry(a).or_insert(Vec3::ZERO);
                *da = *da - push;
                let db = displacement.entry(b).or_insert(Vec3::ZERO);
                *db = *db + push;
            }
        }

        for (node, delta) in displacement {
            let clamped = if delta.length() > MAX_STEP_MM {
                delta.normalized().scale(MAX_STEP_MM)
            } else {
                delta
            };
            if let Some(pos) = g.graph[node].position {
                g.graph[node].position = Some(pos + clamped);
            }
        }
    }
}

/// A uniform spatial hash grid over one round's current positions, cell
/// size equal to the repulsion radius - so any pair of stitches within
/// that radius of each other are guaranteed to land in the same cell or an
/// adjacent one, and `candidate_pairs` only has to check within-cell and
/// the 26 neighboring cells rather than every other stitch in the round.
/// Nodes with no position yet are skipped entirely (same leniency `relax`
/// already applies per-pair).
///
/// This is rebuilt fresh every relaxation iteration rather than maintained
/// incrementally - positions do move each iteration, but only by up to
/// `MAX_STEP_MM`, and rebuilding a hash map from scratch each time is
/// still far cheaper than the all-pairs scan it replaces once a round has
/// more than a few dozen stitches (a long flat row can have hundreds).
struct RepulsionGrid {
    buckets: std::collections::HashMap<(i32, i32, i32), Vec<petgraph::graph::NodeIndex>>,
}

impl RepulsionGrid {
    fn build(g: &StitchGraph, round: &[petgraph::graph::NodeIndex], cell_size: f32) -> Self {
        let mut buckets: std::collections::HashMap<
            (i32, i32, i32),
            Vec<petgraph::graph::NodeIndex>,
        > = std::collections::HashMap::new();
        for &idx in round {
            let Some(pos) = g.graph[idx].position else {
                continue;
            };
            buckets
                .entry(Self::cell_of(pos, cell_size))
                .or_default()
                .push(idx);
        }
        RepulsionGrid { buckets }
    }

    fn cell_of(pos: Vec3, cell_size: f32) -> (i32, i32, i32) {
        (
            (pos.x / cell_size).floor() as i32,
            (pos.y / cell_size).floor() as i32,
            (pos.z / cell_size).floor() as i32,
        )
    }

    /// Every same-round pair that's *plausibly* within the repulsion
    /// radius of each other - i.e. sharing a cell or an adjacent one.
    /// `relax` still does the exact distance check before applying any
    /// force; this only narrows down which pairs are worth checking at
    /// all, the same role an all-pairs scan's full loop used to play.
    fn candidate_pairs(&self) -> Vec<(petgraph::graph::NodeIndex, petgraph::graph::NodeIndex)> {
        let mut pairs = Vec::new();
        for (&(cx, cy, cz), cell_nodes) in &self.buckets {
            // Within this cell: every pair once.
            for i in 0..cell_nodes.len() {
                for j in (i + 1)..cell_nodes.len() {
                    pairs.push((cell_nodes[i], cell_nodes[j]));
                }
            }
            // Against half the neighboring cells only (a fixed forward
            // ordering of the 13 "ahead" offsets, skipping the mirror-image
            // 13 "behind" ones plus the cell itself already handled above)
            // so each cross-cell pair is produced exactly once rather than
            // twice from each cell's perspective.
            const FORWARD_NEIGHBORS: [(i32, i32, i32); 13] = [
                (1, 0, 0),
                (1, 1, 0),
                (0, 1, 0),
                (-1, 1, 0),
                (1, 0, 1),
                (1, 1, 1),
                (0, 1, 1),
                (-1, 1, 1),
                (1, 0, -1),
                (1, 1, -1),
                (0, 1, -1),
                (-1, 1, -1),
                (0, 0, 1),
            ];
            for (dx, dy, dz) in FORWARD_NEIGHBORS {
                if let Some(neighbor_nodes) = self.buckets.get(&(cx + dx, cy + dy, cz + dz)) {
                    for &a in cell_nodes {
                        for &b in neighbor_nodes {
                            pairs.push((a, b));
                        }
                    }
                }
            }
        }
        pairs
    }
}

pub struct TensionReport {
    pub loose: usize,
    pub stretched: usize,
    pub normal: usize,
}

pub fn summarize_tension(g: &StitchGraph) -> TensionReport {
    let mut r = TensionReport {
        loose: 0,
        stretched: 0,
        normal: 0,
    };
    for node in g.graph.node_weights() {
        match node.tension {
            Some(TensionState::Loose) => r.loose += 1,
            Some(TensionState::Stretched) => r.stretched += 1,
            Some(TensionState::Normal) => r.normal += 1,
            None => {}
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use abyssal_thread_core::ColorGrid;

    #[test]
    fn layout_flat_grid_places_stitches_on_a_plain_rectangular_grid() {
        let grid = ColorGrid::new(3, 2, [0, 0, 0]);
        let mut g = StitchGraph::from_color_grid(&grid);
        layout_flat_grid(&mut g, 6.0);

        let row0_col2 = g.rounds[0][2];
        let pos = g.graph[row0_col2].position.expect("position set");
        assert_eq!((pos.x, pos.y, pos.z), (6.0, 3.0, 0.0));

        let row1_col0 = g.rounds[1][0];
        let pos = g.graph[row1_col0].position.expect("position set");
        assert_eq!((pos.x, pos.y, pos.z), (-6.0, -3.0, 0.0));
    }

    #[test]
    fn relax_pulls_a_too_close_sequence_pair_toward_baseline_width() {
        use abyssal_thread_core::graph::StitchEdge;
        use abyssal_thread_core::StitchKind;

        let mut g = StitchGraph::new();
        let a = g.add_stitch(StitchKind::SingleCrochet, 0, None);
        let b = g.add_stitch(StitchKind::SingleCrochet, 0, None);
        g.connect(a, b, StitchEdge::Sequence);
        // sc's baseline width is 6.0mm (see StitchKind); placed at 1.0mm
        // apart here, well below rest length.
        g.graph[a].position = Some(Vec3::new(0.0, 0.0, 0.0));
        g.graph[b].position = Some(Vec3::new(1.0, 0.0, 0.0));

        let before = g.graph[a]
            .position
            .unwrap()
            .distance(g.graph[b].position.unwrap());
        relax(&mut g, Gauge::default(), 30);
        let after = g.graph[a]
            .position
            .unwrap()
            .distance(g.graph[b].position.unwrap());

        assert!(
            after > before,
            "expected relax to pull the pair apart toward baseline width, got {before} -> {after}"
        );
    }

    #[test]
    fn relax_is_a_no_op_at_zero_iterations() {
        use abyssal_thread_core::graph::StitchEdge;
        use abyssal_thread_core::StitchKind;

        let mut g = StitchGraph::new();
        let a = g.add_stitch(StitchKind::SingleCrochet, 0, None);
        let b = g.add_stitch(StitchKind::SingleCrochet, 0, None);
        g.connect(a, b, StitchEdge::Sequence);
        g.graph[a].position = Some(Vec3::new(0.0, 0.0, 0.0));
        g.graph[b].position = Some(Vec3::new(1.0, 0.0, 0.0));

        relax(&mut g, Gauge::default(), 0);
        assert_eq!(g.graph[b].position, Some(Vec3::new(1.0, 0.0, 0.0)));
    }

    #[test]
    fn repulsion_grid_finds_same_cell_pairs_exactly_once() {
        use abyssal_thread_core::StitchKind;

        let mut g = StitchGraph::new();
        let nodes: Vec<_> = (0..3)
            .map(|_| g.add_stitch(StitchKind::SingleCrochet, 0, None))
            .collect();
        // All three within 1mm of each other - well inside one 6mm cell.
        for (i, &n) in nodes.iter().enumerate() {
            g.graph[n].position = Some(Vec3::new(i as f32 * 0.5, 0.0, 0.0));
        }
        let grid = RepulsionGrid::build(&g, &nodes, 6.0);
        let pairs = grid.candidate_pairs();
        // 3 choose 2 = 3, no duplicates and no missing pairs.
        assert_eq!(pairs.len(), 3);
    }

    #[test]
    fn repulsion_grid_finds_adjacent_cell_pairs_without_double_counting() {
        use abyssal_thread_core::StitchKind;

        let mut g = StitchGraph::new();
        let a = g.add_stitch(StitchKind::SingleCrochet, 0, None);
        let b = g.add_stitch(StitchKind::SingleCrochet, 0, None);
        // 6mm cells: x=5.9 and x=6.1 land in adjacent cells (0 and 1) but
        // are only 0.2mm apart - well within repulsion range.
        g.graph[a].position = Some(Vec3::new(5.9, 0.0, 0.0));
        g.graph[b].position = Some(Vec3::new(6.1, 0.0, 0.0));
        let round = vec![a, b];
        let grid = RepulsionGrid::build(&g, &round, 6.0);
        let pairs = grid.candidate_pairs();
        assert_eq!(
            pairs.len(),
            1,
            "expected exactly one candidate pair, got {pairs:?}"
        );
    }

    #[test]
    fn repulsion_grid_skips_nodes_with_no_position_yet() {
        use abyssal_thread_core::StitchKind;

        let mut g = StitchGraph::new();
        let a = g.add_stitch(StitchKind::SingleCrochet, 0, None);
        let b = g.add_stitch(StitchKind::SingleCrochet, 0, None); // never positioned
        g.graph[a].position = Some(Vec3::new(0.0, 0.0, 0.0));
        let round = vec![a, b];
        let grid = RepulsionGrid::build(&g, &round, 6.0);
        assert_eq!(grid.candidate_pairs().len(), 0);
    }

    #[test]
    fn relax_spreads_a_larger_tightly_clustered_round() {
        use abyssal_thread_core::StitchKind;

        // Regression guard for the spatial-grid rewrite: with the old
        // all-pairs scan every one of these pairs was checked directly; the
        // grid-based version has to actually find all of them via
        // same-cell/adjacent-cell lookups to produce the same behavior.
        let mut g = StitchGraph::new();
        let nodes: Vec<_> = (0..8)
            .map(|_| g.add_stitch(StitchKind::SingleCrochet, 0, None))
            .collect();
        for (i, &n) in nodes.iter().enumerate() {
            // All 8 crammed into a 3.5mm span - far tighter than the 6mm
            // repulsion radius, so every pair should push apart.
            g.graph[n].position = Some(Vec3::new(i as f32 * 0.5, 0.0, 0.0));
        }

        let before_span = g.graph[*nodes.last().unwrap()].position.unwrap().x
            - g.graph[nodes[0]].position.unwrap().x;
        relax(&mut g, Gauge::default(), 30);
        let after_span = g.graph[*nodes.last().unwrap()].position.unwrap().x
            - g.graph[nodes[0]].position.unwrap().x;

        assert!(
            after_span > before_span,
            "expected the cluster to spread out, got {before_span} -> {after_span}"
        );
    }
}
