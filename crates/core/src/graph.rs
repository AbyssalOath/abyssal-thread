use crate::{
    geometry::Vec3,
    stitch::StitchKind,
};
use petgraph::{
    graph::{DiGraph, NodeIndex},
    visit::EdgeRef,
};
use std::collections::HashMap;
/// A single stitch instance placed in the pattern.
#[derive(Debug, Clone)]
pub struct StitchNode {
    pub kind: StitchKind,
    pub round: usize,
    pub index_in_round: usize,
    pub label: Option<String>,
    /// Filled in by the layout engine; `None` until then.
    pub position: Option<Vec3>,
    /// Filled in by the tension analyzer.
    pub tension: Option<TensionState>,
    /// Yarn color for this stitch, if known - set directly for
    /// image-derived colorwork graphs (see `StitchGraph::from_color_grid`);
    /// `None` for ordinary shaped-DSL stitches, which have no color concept
    /// yet (the DSL only assigns color via the separate `COLORGRID:` block,
    /// not per-stitch on `sc`/`dc`/etc. - see lang::ast::Pattern::color_grid).
    pub color: Option<[u8; 3]>,
    /// Name of the `DEF:` custom stitch that produced this node, if any -
    /// set by `lang::eval` while expanding either form of custom stitch
    /// (alias or raw geometry; see that module's doc comment), `None` for
    /// an ordinary stitch typed directly in a round. For a stitch nested
    /// inside another custom stitch's body, this is the *innermost*
    /// definition's name, not the outermost invocation's.
    ///
    /// This is provenance for warning purposes only (see
    /// `gui::grid::GridState::def_derived_names`) - it does NOT carry
    /// enough information (no invocation-boundary/count tracking) to
    /// reconstruct the original `Ndefname` invocation once a cell derived
    /// from it has been edited elsewhere in the GUI grid editor. Attempting
    /// that automatically turned out to risk silently emitting a *wrong*
    /// stitch count if an invocation's cells were only partially edited/
    /// deleted, which is worse than the current honest behavior: editing
    /// any round flattens custom stitches to raw stitches in the saved DSL
    /// text, and the GUI warns loudly (banner + status message) when that's
    /// about to happen or just happened, rather than staying silent.
    pub def_origin: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TensionState {
    Loose,
    Normal,
    Stretched,
}
/// How two stitches relate to each other structurally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StitchEdge {
    /// Worked immediately after, in the same round/row (left-right neighbor).
    Sequence,
    /// Worked *into* the parent stitch from the previous round (or an
    /// explicit `@label` attachment point).
    Parent,
}
/// Wraps a `petgraph::DiGraph` with crochet-specific bookkeeping: label
/// lookup, and per-round stitch ordering (needed by both layout and export).
#[derive(Debug, Default)]
pub struct StitchGraph {
    pub graph: DiGraph<StitchNode, StitchEdge>,
    pub labels: HashMap<String, NodeIndex>,
    /// Node indices grouped by round, in stitch order. Rebuilt as stitches are added.
    pub rounds: Vec<Vec<NodeIndex>>,
    /// Non-fatal issues noticed while building this graph (currently: a raw
    /// `DEF` body's `%-N`/`%+N` reference that fell outside the body's
    /// actual stitch range - see `lang::eval::expand_raw_def`). These don't
    /// stop compilation (a typo'd offset just means one fewer attachment
    /// edge, not an invalid pattern), but silently dropping the edge with
    /// no trace anywhere was its own problem - callers should surface
    /// these somewhere the person will actually see them (CLI: printed
    /// after a successful `build`; GUI: the status bar's warning icon).
    pub warnings: Vec<String>,
}
impl StitchGraph {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add_stitch(&mut self, kind: StitchKind, round: usize, label: Option<String>) -> NodeIndex {
        while self.rounds.len() <= round {
            self.rounds.push(Vec::new());
        }
        let index_in_round = self.rounds[round].len();
        let node = StitchNode {
            kind,
            round,
            index_in_round,
            label: label.clone(),
            position: None,
            tension: None,
            color: None,
            def_origin: None,
        };
        let idx = self.graph.add_node(node);
        self.rounds[round].push(idx);
        if let Some(l) = label {
            self.labels.insert(l, idx);
        }
        idx
    }
    pub fn set_color(&mut self, node: NodeIndex, color: [u8; 3]) {
        self.graph[node].color = Some(color);
    }
    /// Records a non-fatal issue - see `StitchGraph::warnings`'s doc comment.
    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }
    pub fn connect(&mut self, from: NodeIndex, to: NodeIndex, edge: StitchEdge) {
        self.graph.add_edge(from, to, edge);
    }
    pub fn parent_of(&self, node: NodeIndex) -> Option<NodeIndex> {
        self.graph
            .edges_directed(node, petgraph::Direction::Incoming)
            .find(|e| *e.weight() == StitchEdge::Parent)
            .map(|e| e.source())
    }
    pub fn sequence_neighbors(&self, node: NodeIndex) -> Vec<NodeIndex> {
        self.graph
            .edges_directed(node, petgraph::Direction::Outgoing)
            .filter(|e| *e.weight() == StitchEdge::Sequence)
            .map(|e| e.target())
            .collect()
    }
    pub fn stitch_count(&self) -> usize {
        self.graph.node_count()
    }
    pub fn round_count(&self) -> usize {
        self.rounds.len()
    }
}
