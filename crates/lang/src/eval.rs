use crate::ast::{Modifier, Op, Pattern};
use crate::error::ParseError;
use crate::parser;
use crate::raw_def::{self, RawAttachRef, RawOp};
use abyssal_thread_core::graph::StitchEdge;
use abyssal_thread_core::{StitchGraph, StitchKind};
use petgraph::graph::NodeIndex;
use std::collections::{HashMap, VecDeque};

/// Hard ceiling on total stitches a single pattern can produce - a defense
/// against *compounding* multiplication (nested `DEF` invocations or
/// repeat groups that each individually stay under
/// `parser::MAX_LITERAL_COUNT` but multiply out to something enormous
/// together, e.g. `2000shell` where `shell` is itself `(500dc) * 4`),
/// which that per-literal cap alone can't catch. Checked once per
/// `FlatOp` (not per-stitch, to avoid touching every stitch-placement call
/// site's signature) so a single oversized op can overshoot this by at
/// most one op's worth of stitches - acceptable slop for a safety valve,
/// not a precise limit. Generous for any realistic pattern: even a very
/// large blanket or afghan tops out in the tens of thousands of stitches
/// total, not hundreds of thousands.
const MAX_TOTAL_STITCHES: usize = 500_000;

/// Custom stitch expansion (`DEF: name = ...`).
///
/// This supports two forms:
///
/// - **Alias**: a definition's body is ordinary DSL syntax (stitches,
///   repeats, labels/attach), and invoking `Nname` splices `N` copies of
///   that body's flattened ops in place. Handled by `flatten`/
///   `flatten_into` below, purely as text-level macro expansion - it runs
///   before any graph nodes exist. Modifiers applied to the invocation
///   (e.g. `flo.shell`) are NOT propagated into the body's inner stitches.
/// - **Raw stitch geometry**: CrochetPARADE-style bodies using `%`
///   self-reference and `ss@1[%,%-4]`-style relative-attachment brackets
///   (see the `raw_def` module doc) to define genuinely novel internal
///   geometry rather than a combination of existing stitches. Detected via
///   `raw_def::looks_like_raw_body` and expanded by `expand_raw_def` below,
///   which - unlike the alias form - needs live graph access at expansion
///   time, since `%`-family references resolve to actual already-placed
///   `NodeIndex`es, not text.
///
/// Only a direct `Nname` invocation is supported for either form (no
/// modifiers on the invocation itself), and raw bodies may only reference
/// builtin stitch abbreviations, not other `DEF` names - see `raw_def`'s
/// module doc for the full scope of what raw geometry does and doesn't
/// cover.
fn is_builtin_abbrev(abbrev: &str) -> bool {
    matches!(
        abbrev.to_ascii_lowercase().as_str(),
        "ch" | "ss" | "sl" | "slst" | "sc" | "hdc" | "dc" | "tr" | "inc" | "dec"
    )
}

fn apply_modifier(kind: StitchKind, m: &Option<Modifier>) -> StitchKind {
    match m {
        None => kind,
        Some(Modifier::FrontLoopOnly) => StitchKind::FrontLoopOnly(Box::new(kind)),
        Some(Modifier::BackLoopOnly) => StitchKind::BackLoopOnly(Box::new(kind)),
        Some(Modifier::FrontPost) => StitchKind::FrontPost(Box::new(kind)),
        Some(Modifier::BackPost) => StitchKind::BackPost(Box::new(kind)),
    }
}

/// Walks the AST round by round, threading a "parent cursor" through the
/// previous round's stitches so each new stitch attaches into the right
/// parent by default (or an explicit `@label` when given).
pub fn eval(pattern: &Pattern) -> Result<StitchGraph, ParseError> {
    // A `COLORGRID:` block is a wholly different pattern mode (see
    // ast::Pattern::color_grid) - bypass the round/repeat shaping logic
    // below entirely and build straight from the image-derived grid.
    if let Some(grid) = &pattern.color_grid {
        return Ok(StitchGraph::from_color_grid(grid));
    }

    let mut g = StitchGraph::new();
    let mut prev_round: VecDeque<NodeIndex> = VecDeque::new();
    let definitions: HashMap<String, String> = pattern.definitions.iter().cloned().collect();

    for (round_idx, round) in pattern.rounds.iter().enumerate() {
        let mut last_in_round: Option<NodeIndex> = None;
        let mut pending_label: Option<String> = None;
        let mut pending_attach: Option<String> = None;
        let mut parent_cursor = prev_round.clone();
        let mut this_round: Vec<NodeIndex> = Vec::new();

        // Flatten repeats (and custom-stitch invocations) into a linear op stream.
        let flat_ops = flatten(&round.ops, &definitions)?;

        for op in flat_ops {
            if g.stitch_count() > MAX_TOTAL_STITCHES {
                return Err(ParseError::PatternTooLarge(
                    g.stitch_count(),
                    MAX_TOTAL_STITCHES,
                ));
            }
            match op {
                FlatOp::Label(name) => pending_label = Some(name),
                FlatOp::AttachTo(name) => pending_attach = Some(name),
                FlatOp::RawInvoke {
                    name,
                    raw_ops,
                    count,
                } => {
                    for inv_n in 0..count {
                        // Same "only the first sub-invocation consumes an
                        // explicit `@label`" rule ordinary `FlatOp::Stitch`
                        // uses for `count > 1` - see below.
                        let explicit_parent = if inv_n == 0 {
                            match pending_attach.take() {
                                Some(label_name) => {
                                    Some(*g.labels.get(&label_name).ok_or_else(|| {
                                        ParseError::UnknownLabel(label_name.clone())
                                    })?)
                                }
                                None => None,
                            }
                        } else {
                            None
                        };
                        expand_raw_def(
                            &mut g,
                            round_idx,
                            &name,
                            &raw_ops,
                            &mut RoundCursor {
                                parent_cursor: &mut parent_cursor,
                                last_in_round: &mut last_in_round,
                                this_round: &mut this_round,
                                pending_label: &mut pending_label,
                            },
                            explicit_parent,
                        );
                    }
                }
                FlatOp::Stitch {
                    modifier,
                    abbrev,
                    count,
                    color,
                    def_origin,
                } => {
                    let base_kind = apply_modifier(StitchKind::from_abbrev(&abbrev), &modifier);
                    let is_increase = matches!(base_kind, StitchKind::Increase(_));
                    let is_decrease = matches!(base_kind, StitchKind::Decrease(_));

                    // `count` is the number of increase/decrease *operations*
                    // (e.g. `3inc` = three separate increases = 6 stitches),
                    // not the number of output stitches directly - increase
                    // and decrease each have their own child/parent
                    // multiplicity, handled per-operation below.
                    for op_n in 0..count {
                        // An explicit `@label` attach only applies to the
                        // very first operation in this token; `.take()`
                        // ensures later operations (when count > 1) fall
                        // back to the positional parent cursor.
                        let explicit_parent = if op_n == 0 {
                            match pending_attach.take() {
                                Some(label_name) => {
                                    Some(*g.labels.get(&label_name).ok_or_else(|| {
                                        ParseError::UnknownLabel(label_name.clone())
                                    })?)
                                }
                                None => None,
                            }
                        } else {
                            None
                        };

                        let place_stitch =
                            |g: &mut StitchGraph,
                             last_in_round: &mut Option<NodeIndex>,
                             this_round: &mut Vec<NodeIndex>,
                             pending_label: &mut Option<String>,
                             parent: Option<NodeIndex>| {
                                let label = pending_label.take();
                                let node = g.add_stitch(base_kind.clone(), round_idx, label);
                                if let Some(c) = color {
                                    g.set_color(node, c);
                                }
                                if let Some(origin) = &def_origin {
                                    g.graph[node].def_origin = Some(origin.clone());
                                }
                                if let Some(prev) = *last_in_round {
                                    g.connect(prev, node, StitchEdge::Sequence);
                                }
                                *last_in_round = Some(node);
                                this_round.push(node);
                                if round_idx > 0 {
                                    if let Some(p) = parent {
                                        g.connect(p, node, StitchEdge::Parent);
                                    }
                                }
                                node
                            };

                        if is_increase {
                            // One increase = two children sharing one parent slot.
                            let parent = explicit_parent.or_else(|| {
                                if round_idx > 0 {
                                    parent_cursor.pop_front()
                                } else {
                                    None
                                }
                            });
                            for _ in 0..2 {
                                place_stitch(
                                    &mut g,
                                    &mut last_in_round,
                                    &mut this_round,
                                    &mut pending_label,
                                    parent,
                                );
                            }
                        } else if is_decrease {
                            // One decrease = two parents merged into one child.
                            let (p1, p2) = if let Some(ep) = explicit_parent {
                                (Some(ep), None)
                            } else if round_idx > 0 {
                                (parent_cursor.pop_front(), parent_cursor.pop_front())
                            } else {
                                (None, None)
                            };
                            let node = place_stitch(
                                &mut g,
                                &mut last_in_round,
                                &mut this_round,
                                &mut pending_label,
                                p1,
                            );
                            if let Some(p2) = p2 {
                                g.connect(p2, node, StitchEdge::Parent);
                            }
                        } else {
                            let parent = explicit_parent.or_else(|| {
                                if round_idx > 0 {
                                    parent_cursor.pop_front()
                                } else {
                                    None
                                }
                            });
                            place_stitch(
                                &mut g,
                                &mut last_in_round,
                                &mut this_round,
                                &mut pending_label,
                                parent,
                            );
                        }
                    }
                }
            }
        }

        prev_round = this_round.into();
    }

    Ok(g)
}

enum FlatOp {
    Stitch {
        modifier: Option<Modifier>,
        abbrev: String,
        color: Option<[u8; 3]>,
        count: u32,
        /// Which `DEF` (innermost, if nested) produced this stitch, if
        /// any - see `StitchNode::def_origin`'s doc comment for exactly
        /// what this is (and isn't) used for.
        def_origin: Option<String>,
    },
    Label(String),
    AttachTo(String),
    /// A raw stitch-geometry `DEF` invocation (see the module doc and
    /// `raw_def`). Carries the already-parsed body so `eval`'s main loop
    /// doesn't need to re-look-up/re-parse the definition text, and the
    /// def's own name for `def_origin` tagging and warning messages.
    RawInvoke {
        name: String,
        raw_ops: Vec<RawOp>,
        count: u32,
    },
}

fn flatten(ops: &[Op], defs: &HashMap<String, String>) -> Result<Vec<FlatOp>, ParseError> {
    let mut out = Vec::new();
    let mut expanding: Vec<String> = Vec::new();
    for op in ops {
        flatten_into(op, defs, &mut expanding, None, &mut out)?;
    }
    Ok(out)
}

/// `origin` is the name of the `DEF` currently being expanded, if any -
/// `None` for a plain top-level op. Threaded through (rather than derived
/// from `expanding`'s stack top) so a nested custom stitch's own body
/// naturally shadows the outer one's name for its own stitches without
/// needing any special-casing here: recursing into a further `DEF` just
/// passes `Some(that_def's_name)` instead of forwarding `origin` along,
/// while `Op::Repeat` (which never itself defines a new origin) always
/// forwards whatever `origin` it received unchanged.
fn flatten_into(
    op: &Op,
    defs: &HashMap<String, String>,
    expanding: &mut Vec<String>,
    origin: Option<&str>,
    out: &mut Vec<FlatOp>,
) -> Result<(), ParseError> {
    // The real place to catch a *compounding* blowup, not just `eval`'s
    // main loop (see `MAX_TOTAL_STITCHES`'s doc comment): an alias `DEF`
    // body's own internal repeat group multiplied by a large invocation
    // count (e.g. `DEF: shell = (500dc) * 20` invoked as `2000shell`) fully
    // unrolls into `out` right here, recursively, before any graph node
    // exists for `eval` to have a chance to check - each individual
    // literal count/times can stay under `parser::MAX_LITERAL_COUNT` while
    // the product still blows past this. Checked on every recursive call
    // (not just once per top-level op) so it fires as soon as `out`
    // actually crosses the line, not only after a whole subtree finishes
    // unrolling.
    if out.len() > MAX_TOTAL_STITCHES {
        return Err(ParseError::PatternTooLarge(out.len(), MAX_TOTAL_STITCHES));
    }
    match op {
        Op::Stitch {
            modifier,
            abbrev,
            count,
            color,
        } => {
            if !is_builtin_abbrev(abbrev) {
                if let Some(body_src) = defs.get(abbrev) {
                    if raw_def::looks_like_raw_body(body_src) {
                        // Raw stitch-geometry bodies are resolved later,
                        // directly against the graph (see
                        // `eval::expand_raw_def`) rather than spliced into
                        // plain `Op`s here the way alias bodies are, since
                        // `%`/`%-N`/`%+N` need actual `NodeIndex`es that
                        // don't exist yet at this (purely textual) stage.
                        // Parsed once here rather than not at all, so a
                        // syntax error surfaces immediately rather than
                        // only once this invocation is actually reached.
                        let raw_ops = raw_def::parse_raw_def(body_src)?;
                        for raw_op in &raw_ops {
                            let inner_abbrev = match raw_op {
                                RawOp::Stitch { abbrev, .. } => abbrev,
                                RawOp::RelativeStitch { abbrev, .. } => abbrev,
                            };
                            if !is_builtin_abbrev(inner_abbrev) {
                                return Err(ParseError::UnexpectedToken(
                                    format!(
                                        "raw DEF body '{abbrev}' may only use builtin stitch \
                                         abbreviations, found '{inner_abbrev}'"
                                    ),
                                    0,
                                ));
                            }
                        }
                        out.push(FlatOp::RawInvoke {
                            name: abbrev.clone(),
                            raw_ops,
                            count: *count,
                        });
                        return Ok(());
                    }
                    if expanding.contains(abbrev) {
                        return Err(ParseError::CyclicDefinition(abbrev.clone()));
                    }
                    // Parsed on every invocation rather than cached - DEF
                    // bodies are short and this keeps the expansion logic
                    // simple; revisit if patterns get large enough to matter.
                    let body_ops = parser::parse_ops(body_src)?;
                    expanding.push(abbrev.clone());
                    for _ in 0..*count {
                        for inner in &body_ops {
                            // `Some(abbrev)`, not `origin` - a stitch
                            // straight out of *this* def's body belongs to
                            // this def, even if we're nested inside some
                            // outer def's expansion already.
                            flatten_into(inner, defs, expanding, Some(abbrev), out)?;
                        }
                    }
                    expanding.pop();
                    return Ok(());
                }
            }
            out.push(FlatOp::Stitch {
                modifier: modifier.clone(),
                abbrev: abbrev.clone(),
                count: *count,
                color: *color,
                def_origin: origin.map(|s| s.to_string()),
            });
        }
        Op::Label(name) => out.push(FlatOp::Label(name.clone())),
        Op::AttachTo(name) => out.push(FlatOp::AttachTo(name.clone())),
        Op::Repeat { body, times } => {
            for _ in 0..*times {
                for inner in body {
                    flatten_into(inner, defs, expanding, origin, out)?;
                }
            }
        }
    }
    Ok(())
}

/// The mutable round-building state `expand_raw_def` needs all four pieces
/// of, bundled together rather than passed as four separate parameters -
/// clippy flags a function with this many arguments (`too_many_arguments`),
/// and every one of these four already travels together everywhere it's
/// used, so a small struct is a better fit than trimming something else.
struct RoundCursor<'a> {
    parent_cursor: &'a mut VecDeque<NodeIndex>,
    last_in_round: &'a mut Option<NodeIndex>,
    this_round: &'a mut Vec<NodeIndex>,
    pending_label: &'a mut Option<String>,
}

/// Expands one invocation of a raw stitch-geometry `DEF` body (see the
/// `raw_def` module doc) directly into the graph.
///
/// `history[0]` is the "anchor": whatever was last placed in the round
/// before this invocation (`None` for round 0's very first stitch, where
/// there's nothing yet to reference). Every subsequent stitch actually
/// placed - one per unit of a `Stitch { count }`, not one per `RawOp` -
/// appends to `history`, so `%`/`%-N` count individual stitch positions,
/// matching how a crocheter counts actual stitches back, not DSL terms.
///
/// All of `%`/`%-N`/`@N`(the relative primary parent)/refs are resolved
/// against the cursor as it stood *before* the term currently being placed
/// - i.e. `%` in `ss@1[%,%-4]` and the `1` in `@1` both count back from the
///   same starting point, which is what makes a picot's "close back into the
///   stitch before these chains, and separately into one four further back"
///   reading self-consistent (see the module doc's worked example).
///
/// `def_name` is used for two things: tagging every node this invocation
/// places with `StitchNode::def_origin = Some(def_name)`, and naming the
/// definition in any out-of-range warning pushed to `g.warnings` (see
/// below) - a bare "reference out of range" with no indication of *which*
/// custom stitch or which reference wouldn't help much when tracking down
/// a typo like `%-14` instead of `%-4`.
fn expand_raw_def(
    g: &mut StitchGraph,
    round_idx: usize,
    def_name: &str,
    raw_ops: &[RawOp],
    cursor: &mut RoundCursor,
    mut explicit_parent: Option<NodeIndex>,
) {
    let mut history: Vec<Option<NodeIndex>> = vec![*cursor.last_in_round];
    // (node needing the edge, cursor length when the `%+N` ref was
    // written, offset N) - resolved in a second pass once the whole body
    // has been placed, since a forward reference can't be resolved until
    // that many further stitches actually exist.
    let mut pending_forward: Vec<(NodeIndex, usize, u32)> = Vec::new();

    // `pre_len` is `history.len()` *before* placing the term currently
    // being resolved, i.e. the cursor position `%`/`@N` count back from.
    let resolve_back =
        |history: &[Option<NodeIndex>], pre_len: usize, n: u32| -> Option<NodeIndex> {
            pre_len
                .checked_sub(1)?
                .checked_sub(n as usize)
                .and_then(|i| history.get(i).copied().flatten())
        };

    for raw_op in raw_ops {
        let (abbrev, count, back, refs): (&str, u32, Option<u32>, &[RawAttachRef]) = match raw_op {
            RawOp::Stitch { abbrev, count } => (abbrev, *count, None, &[]),
            RawOp::RelativeStitch { abbrev, back, refs } => (abbrev, 1, *back, refs.as_slice()),
        };
        for _ in 0..count {
            let pre_len = history.len();
            let parent = match back {
                Some(n) => {
                    let resolved = resolve_back(&history, pre_len, n);
                    if resolved.is_none() {
                        g.warn(format!(
                            "raw stitch '{def_name}' (round {round_idx}): '@{n}' had nothing \
                             {n} stitch(es) back yet - no primary attachment edge added"
                        ));
                    }
                    resolved
                }
                None => explicit_parent.take().or_else(|| {
                    if round_idx > 0 {
                        cursor.parent_cursor.pop_front()
                    } else {
                        None
                    }
                }),
            };
            let kind = StitchKind::from_abbrev(abbrev);
            let label = cursor.pending_label.take();
            let node = g.add_stitch(kind, round_idx, label);
            g.graph[node].def_origin = Some(def_name.to_string());
            if let Some(prev) = *cursor.last_in_round {
                g.connect(prev, node, StitchEdge::Sequence);
            }
            *cursor.last_in_round = Some(node);
            cursor.this_round.push(node);
            if round_idx > 0 {
                if let Some(p) = parent {
                    g.connect(p, node, StitchEdge::Parent);
                }
            }
            for r in refs {
                match r {
                    RawAttachRef::SelfRef => {
                        if let Some(target) = resolve_back(&history, pre_len, 0) {
                            g.connect(target, node, StitchEdge::Parent);
                        } else {
                            g.warn(format!(
                                "raw stitch '{def_name}' (round {round_idx}): '%' had no \
                                 preceding stitch to refer to - no attachment edge added"
                            ));
                        }
                    }
                    RawAttachRef::Back(n) => {
                        if let Some(target) = resolve_back(&history, pre_len, *n) {
                            g.connect(target, node, StitchEdge::Parent);
                        } else {
                            g.warn(format!(
                                "raw stitch '{def_name}' (round {round_idx}): '%-{n}' had \
                                 nothing {n} stitch(es) back yet - no attachment edge added"
                            ));
                        }
                    }
                    RawAttachRef::Forward(n) => {
                        pending_forward.push((node, pre_len, *n));
                    }
                }
            }
            history.push(Some(node));
        }
    }

    // Second pass: resolve `%+N` refs now that the whole body is placed.
    // `at_len - 1 + n` mirrors `resolve_back`'s `pre_len - 1 - n` with the
    // sign flipped, counting forward from the same cursor point instead of
    // back from it.
    for (node, at_len, n) in pending_forward {
        let idx = at_len - 1 + n as usize;
        match history.get(idx) {
            Some(Some(target)) => g.connect(*target, node, StitchEdge::Parent),
            _ => g.warn(format!(
                "raw stitch '{def_name}' (round {round_idx}): '%+{n}' never resolved (the \
                 body ended before reaching that position) - no attachment edge added"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use abyssal_thread_core::StitchKind;
    use petgraph::visit::EdgeRef;

    fn round_sizes(src: &str) -> Vec<usize> {
        let pattern = parser::parse(src).unwrap();
        let g = eval(&pattern).unwrap();
        g.rounds.iter().map(|r| r.len()).collect()
    }

    #[test]
    fn expands_a_simple_custom_stitch_alias() {
        // "shell" = 3dc; two shells into a 6-stitch round should consume
        // exactly the 6 parent slots (2 * 3 = 6) and produce 6 dc stitches.
        let src = "DEF: shell = 3dc\n6sc\n2shell\n";
        let pattern = parser::parse(src).unwrap();
        let g = eval(&pattern).unwrap();
        assert_eq!(g.rounds[1].len(), 6);
        for &idx in &g.rounds[1] {
            assert_eq!(g.graph[idx].kind, StitchKind::DoubleCrochet);
        }
    }

    #[test]
    fn detects_cyclic_custom_stitch_definitions() {
        let src = "DEF: a = 1b\nDEF: b = 1a\n6sc\n1a\n";
        let pattern = parser::parse(src).unwrap();
        let err = eval(&pattern).unwrap_err();
        assert!(matches!(err, ParseError::CyclicDefinition(_)));
    }

    #[test]
    fn rejects_a_compounding_stitch_count_blowup() {
        // Neither "500" nor "20" nor "2000" individually exceeds the
        // parser's per-literal cap, but the body's own repeat (10,000
        // stitches per invocation) times the invocation count (2000)
        // multiplies out to 20 million - this has to be caught during
        // `flatten`'s unrolling, not just by the per-literal parser check.
        let src = "DEF: shell = (500dc) * 20\n6sc\n2000shell\n";
        let pattern = parser::parse(src).unwrap();
        let err = eval(&pattern).unwrap_err();
        assert!(matches!(err, ParseError::PatternTooLarge(_, _)));
    }

    #[test]
    fn increase_shares_one_parent_between_two_children() {
        let sizes = round_sizes("6sc\n(inc) * 6\n");
        assert_eq!(sizes, vec![6, 12]);
    }

    #[test]
    fn decrease_merges_two_parents_into_one_child() {
        let sizes = round_sizes("12sc\n12sc\n(dec) * 6\n");
        assert_eq!(sizes, vec![12, 12, 6]);
    }

    #[test]
    fn attach_to_label_skips_positional_cursor() {
        let pattern =
            parser::parse("6sc\nsc, sc, anchor!, sc, sc, sc, sc\n@anchor, sc, 5sc\n").unwrap();
        let g = eval(&pattern).unwrap();
        let anchor_idx = *g.labels.get("anchor").unwrap();
        // The stitch attached via @anchor should have a Parent edge from anchor_idx.
        assert_eq!(g.parent_of(g.rounds[2][0]), Some(anchor_idx));
    }

    #[test]
    fn expands_raw_stitch_geometry_picot() {
        // A "picot": 3 chains, then a slip stitch worked 1 back
        // positionally (the primary parent, from `@1`), with extra closing
        // edges into `%` (the chain immediately before it - resolves) and
        // `%-4` (out of range this early in the pattern - see
        // `warns_on_out_of_range_raw_attach_ref` for the warning this now
        // produces; no edge is added either way).
        let src = "DEF: p = 3ch, ss@1[%,%-4]\n2sc\n1p\n";
        let pattern = parser::parse(src).unwrap();
        let g = eval(&pattern).unwrap();

        assert_eq!(g.rounds[0].len(), 2); // the base "2sc"
        assert_eq!(g.rounds[1].len(), 4); // 3 chains + 1 slip stitch
        for &idx in &g.rounds[1][..3] {
            assert_eq!(g.graph[idx].kind, StitchKind::Chain);
        }
        assert_eq!(g.graph[g.rounds[1][3]].kind, StitchKind::SlipStitch);

        let ss = g.rounds[1][3];
        let mut parents: Vec<_> = g
            .graph
            .edges_directed(ss, petgraph::Direction::Incoming)
            .filter(|e| *e.weight() == StitchEdge::Parent)
            .map(|e| e.source())
            .collect();
        parents.sort();
        // 2nd chain (`@1`, one back from the cursor) and 3rd chain (`%`,
        // the cursor itself) - the `%-4` ref goes out of range this early
        // in the body and adds no edge.
        let mut expected = vec![g.rounds[1][1], g.rounds[1][2]];
        expected.sort();
        assert_eq!(parents, expected);
    }

    #[test]
    fn warns_on_out_of_range_raw_attach_ref() {
        // Same picot as above - its `%-4` is out of range this early in
        // the pattern, which should now surface as a warning naming both
        // the def and the specific reference, not just vanish.
        let src = "DEF: p = 3ch, ss@1[%,%-4]\n2sc\n1p\n";
        let pattern = parser::parse(src).unwrap();
        let g = eval(&pattern).unwrap();
        assert!(g
            .warnings
            .iter()
            .any(|w| w.contains('p') && w.contains("%-4")));
    }

    #[test]
    fn tags_def_origin_for_alias_and_raw_custom_stitches() {
        // Alias form: every stitch "shell" produces should be tagged
        // "shell". Raw form: every stitch "p" produces should be tagged
        // "p". Plain top-level stitches (the "6sc") should carry no
        // def_origin at all.
        let src = "DEF: shell = 3dc\nDEF: p = 3ch, ss@1[%,%-4]\n6sc\n2shell\n1p\n";
        let pattern = parser::parse(src).unwrap();
        let g = eval(&pattern).unwrap();

        for &idx in &g.rounds[0] {
            assert_eq!(g.graph[idx].def_origin, None);
        }
        for &idx in &g.rounds[1] {
            assert_eq!(g.graph[idx].def_origin.as_deref(), Some("shell"));
        }
        for &idx in &g.rounds[2] {
            assert_eq!(g.graph[idx].def_origin.as_deref(), Some("p"));
        }
    }

    #[test]
    fn a_realistic_large_flat_pattern_evaluates_correctly() {
        // A big flat tube (e.g. a long scarf or cowl worked in the round)
        // - 300 separate rounds of 300sc each, 90,000 stitches total.
        // Comfortably larger than any pattern a real person would
        // realistically work by hand, but still well under
        // MAX_TOTAL_STITCHES (500,000) - this is the "confirm the limit
        // doesn't false-positive-reject something a real large pattern
        // could actually need" half of exercising it, as opposed to
        // `rejects_a_compounding_stitch_count_blowup`'s "confirm it
        // actually catches a runaway" half. Built with `.repeat` rather
        // than a `(300sc) * 300` repeat *group* - that syntax multiplies
        // stitches within a single round, not across separate rounds,
        // which isn't what "300 rounds" needs here.
        let src: String = "300sc\n".repeat(300);
        let pattern = parser::parse(&src).expect("realistic large pattern should parse");
        let g = eval(&pattern)
            .expect("realistic large pattern should evaluate without hitting the safety limit");
        assert_eq!(g.stitch_count(), 300 * 300);
        assert_eq!(g.round_count(), 300);
    }

    #[test]
    fn a_pattern_just_under_the_total_stitch_limit_succeeds() {
        // 499,900 stitches (one round, via a repeat group) - deliberately
        // close to (but under) MAX_TOTAL_STITCHES, confirming the eval-loop
        // check (`g.stitch_count() > MAX_TOTAL_STITCHES`, a *strict*
        // inequality) doesn't off-by-one reject a pattern that's actually
        // still within budget.
        let src = "(100sc) * 4999\n";
        let pattern = parser::parse(src).expect("pattern should parse");
        let g = eval(&pattern)
            .expect("a pattern just under the total-stitch limit should still succeed");
        assert_eq!(g.stitch_count(), 499_900);
    }

    #[test]
    fn a_pattern_with_many_small_custom_stitch_invocations_evaluates_correctly() {
        // Realistic use of a custom stitch at scale: a shell-stitch trim
        // used many times across a large pattern, rather than one huge
        // invocation - exercises that per-invocation def_origin tagging
        // and the compounding-blowup check's recursive out.len() check
        // both stay correct (and fast enough to finish this test) at a
        // realistic "large real pattern" scale rather than just a handful
        // of invocations like the other DEF-focused tests use.
        let src = "DEF: shell = 3dc, ch1\n300sc\n(100shell) * 3\n";
        let pattern = parser::parse(src).expect("pattern should parse");
        let g = eval(&pattern).expect(
            "many custom-stitch invocations should evaluate without hitting the safety limit",
        );
        // 3 repeats * 100 invocations * 4 stitches per "shell" (3dc + ch1).
        assert_eq!(g.rounds[1].len(), 3 * 100 * 4);
        for &idx in &g.rounds[1] {
            assert_eq!(g.graph[idx].def_origin.as_deref(), Some("shell"));
        }
    }
}
