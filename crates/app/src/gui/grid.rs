//! The clickable grid editor: one row per round, one clickable cell per
//! stitch. Cells are tinted by tension state (reusing the same
//! `TensionState` the CLI's `--tension` report is built from) so tight/loose
//! spots are visible at a glance, the same way the 3D viewport colors
//! points. This is a *view* over the compiled `StitchGraph` - see the
//! module doc in `gui::mod` for how edits round-trip back through the DSL.

use abyssal_thread_core::graph::TensionState;
use abyssal_thread_core::{StitchGraph, StitchKind};
use eframe::egui::{self, Color32};
use petgraph::graph::NodeIndex;

#[derive(Default, Clone)]
pub struct GridState {
    pub rounds: Vec<GridRound>,
}

#[derive(Default, Clone)]
pub struct GridRound {
    pub cells: Vec<GridCell>,
}

/// One clickable cell in the grid. Usually backed by a single stitch node,
/// but an `inc`'s two children (which share one parent) are merged into a
/// single logical cell - `nodes` has two entries in that case, so a real
/// crochet chart's "V for one increase" reads the same way here.
#[derive(Clone)]
pub struct GridCell {
    pub nodes: Vec<NodeIndex>,
    pub kind: StitchKind,
    pub label: Option<String>,
    pub tension: Option<TensionState>,
    /// Yarn color for this stitch, if set (`~hex` in the DSL). Mirrors
    /// `StitchNode::color` - see that field's doc comment for why shaped
    /// patterns can carry per-stitch color at all (it's the same `~hex`
    /// syntax colorwork's `COLORGRID:` block reuses, just attached directly
    /// to a stitch term instead of a grid cell).
    pub color: Option<[u8; 3]>,
    /// Which `DEF` custom stitch produced this cell, if any - mirrors
    /// `StitchNode::def_origin`. Used only to detect and warn about the
    /// round-trip fidelity gap: editing *any* cell in a round flattens the
    /// whole grid back to raw stitches when re-serialized (see `to_dsl`),
    /// silently dropping this the moment it happens (a fresh/edited cell
    /// has no def to attribute it to). See `GridState::def_derived_names`
    /// and the warning banner in `gui::mod`'s `ViewMode::Grid` arm.
    pub def_origin: Option<String>,
}

pub enum GridAction {
    Select(Vec<NodeIndex>),
    AppendStitch {
        round: usize,
        text: String,
        label: Option<String>,
        color: Option<[u8; 3]>,
    },
    EditCell {
        round: usize,
        cell: usize,
        text: String,
        label: Option<String>,
        color: Option<[u8; 3]>,
    },
    DeleteCell {
        round: usize,
        cell: usize,
    },
    AddRound,
    DeleteRound {
        round: usize,
    },
}

/// State for the "add/edit stitch" popup window. Lives on `GoblinApp`
/// (passed in by mutable reference) rather than inside `GridState`, since
/// `GridState` gets wholesale replaced on every recompile but the popup
/// needs to survive across frames while the user is filling it in.
#[derive(Clone)]
pub struct PendingEdit {
    round: usize,
    /// `None` when adding a new stitch; `Some(cell_idx)` when editing.
    cell: Option<usize>,
    modifier_idx: usize,
    abbrev: String,
    label: String,
    /// `None` = no explicit color (the stitch just inherits whatever the
    /// default yarn is); `Some(rgb)` = an explicit `~hex` on this stitch.
    /// Kept separate from `abbrev`/`modifier_idx` rather than folded into
    /// the DSL text those build, since it's structurally independent
    /// metadata - same reasoning as `label` being its own field here.
    color: Option<[u8; 3]>,
}

/// (display name, DSL prefix) - index 0 is "no modifier".
const MODIFIERS: &[(&str, &str)] = &[
    ("(none)", ""),
    ("Front loop only", "flo"),
    ("Back loop only", "blo"),
    ("Front post", "fpost"),
    ("Back post", "bpost"),
];

const PALETTE: &[&str] = &["ch", "ss", "sc", "hdc", "dc", "tr", "inc", "dec"];

impl GridState {
    pub fn from_graph(g: &StitchGraph) -> Self {
        let rounds = g
            .rounds
            .iter()
            .map(|round| GridRound {
                cells: Self::build_cells(g, round),
            })
            .collect();
        GridState { rounds }
    }

    /// Walks one round's node list, merging a same-parent pair of `Increase`
    /// children into a single logical cell (see the `GridCell` doc comment).
    /// `dec` never needs this - it's already one node with two parent edges.
    fn build_cells(g: &StitchGraph, round: &[NodeIndex]) -> Vec<GridCell> {
        let mut cells = Vec::new();
        let mut i = 0;
        while i < round.len() {
            let idx = round[i];
            let node = &g.graph[idx];
            let is_increase = matches!(node.kind, StitchKind::Increase(_));

            if is_increase && i + 1 < round.len() {
                let next_idx = round[i + 1];
                let next = &g.graph[next_idx];
                let next_is_increase = matches!(next.kind, StitchKind::Increase(_));
                let shares_parent = g.parent_of(idx) == g.parent_of(next_idx);
                // Also require matching color and def_origin before
                // merging - an increase's two children could in principle
                // carry different `~hex` colors (unusual, but the DSL
                // doesn't forbid it) or come from different custom stitches
                // entirely (if one child's provenance metadata is stale
                // from a partial edit), and silently picking one over the
                // other would lose that distinction rather than just
                // display it as two separate cells the way it should.
                if next_is_increase
                    && shares_parent
                    && node.label.is_none()
                    && next.label.is_none()
                    && node.color == next.color
                    && node.def_origin == next.def_origin
                {
                    cells.push(GridCell {
                        nodes: vec![idx, next_idx],
                        kind: node.kind.clone(),
                        label: None,
                        tension: combine_tension(node.tension, next.tension),
                        color: node.color,
                        def_origin: node.def_origin.clone(),
                    });
                    i += 2;
                    continue;
                }
            }

            cells.push(GridCell {
                nodes: vec![idx],
                kind: node.kind.clone(),
                label: node.label.clone(),
                tension: node.tension,
                color: node.color,
                def_origin: node.def_origin.clone(),
            });
            i += 1;
        }
        cells
    }

    /// Names of every `DEF` custom stitch that produced at least one cell
    /// currently in this grid, sorted and deduplicated. Empty for a
    /// pattern that doesn't use any custom stitches (or uses them but no
    /// longer has any cells derived from one, e.g. after they've already
    /// been edited away). See the `GridCell::def_origin` doc comment for
    /// what this drives in the GUI.
    pub fn def_derived_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .rounds
            .iter()
            .flat_map(|r| r.cells.iter())
            .filter_map(|c| c.def_origin.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Applies a UI-driven edit to this *in-memory* grid, ahead of
    /// re-serializing to DSL text (see `to_dsl`) and recompiling. The
    /// authoritative grid is always the one rebuilt from the freshly
    /// recompiled graph right after, so this mutation is speculative.
    pub fn apply(&mut self, action: GridAction) {
        match action {
            GridAction::Select(_) => {}
            GridAction::AppendStitch {
                round,
                text,
                label,
                color,
            } => {
                if let Some(r) = self.rounds.get_mut(round) {
                    r.cells.push(GridCell {
                        nodes: Vec::new(), // no graph node yet; filled in on recompile
                        kind: parse_stitch_text(&text),
                        label,
                        tension: None,
                        color,
                        def_origin: None,
                    });
                }
            }
            GridAction::EditCell {
                round,
                cell,
                text,
                label,
                color,
            } => {
                if let Some(c) = self
                    .rounds
                    .get_mut(round)
                    .and_then(|r| r.cells.get_mut(cell))
                {
                    c.kind = parse_stitch_text(&text);
                    c.label = label;
                    c.color = color;
                    c.def_origin = None;
                }
            }
            GridAction::DeleteCell { round, cell } => {
                if let Some(r) = self.rounds.get_mut(round) {
                    if cell < r.cells.len() {
                        r.cells.remove(cell);
                    }
                }
            }
            GridAction::AddRound => self.rounds.push(GridRound::default()),
            GridAction::DeleteRound { round } => {
                if round < self.rounds.len() {
                    self.rounds.remove(round);
                }
            }
        }
    }

    pub fn to_dsl(&self, pattern_name: Option<&str>) -> String {
        let mut out = String::new();
        if let Some(name) = pattern_name {
            out.push_str(&format!("PATTERN: {name}\n"));
        }
        for round in &self.rounds {
            out.push_str(&Self::round_to_dsl(round));
            out.push('\n');
        }
        out
    }

    /// Emits a round's DSL text, trying to recover a compact `(...) * N`
    /// repeat group when the round's stitches are literally a repeating
    /// block - this is what lets patterns like `sphere.cgp`'s
    /// `(2sc, inc) * 6` survive a trip through the grid editor instead of
    /// coming back out as 18 flat tokens. Rounds containing a labeled
    /// stitch skip repeat-detection (a `label!` marks a specific point, not
    /// a position inside an anonymous repeat) and fall straight to
    /// `cells_to_dsl`. This recompresses the *output* stitch sequence and
    /// does not recover original `DEF:` custom-stitch invocations or
    /// nested/irregular repeat groups - see the module doc in `gui::mod`
    /// for that trade-off.
    fn round_to_dsl(round: &GridRound) -> String {
        let n = round.cells.len();
        if n == 0 {
            return "ch".to_string(); // an empty round isn't valid DSL; placeholder stitch
        }

        let has_label = round.cells.iter().any(|c| c.label.is_some());
        if !has_label {
            // Includes color in each token (see `stitch_dsl_text`) so a
            // repeat block is only recognized when every repetition's
            // stitches *and* colors line up - otherwise "6sc" with one
            // stitch colored differently could get misrecognized as a
            // shorter repeating block that silently drops that color.
            let tokens: Vec<String> = round
                .cells
                .iter()
                .map(|c| stitch_dsl_text(&c.kind, c.color))
                .collect();
            for p in 2..=(n / 2) {
                if !n.is_multiple_of(p) {
                    continue;
                }
                let block = &tokens[0..p];
                let repeats = n / p;
                let all_match = (1..repeats).all(|r| &tokens[r * p..(r + 1) * p] == block);
                if all_match {
                    let block_text = Self::cells_to_dsl(&round.cells[0..p]);
                    return format!("({block_text}) * {repeats}");
                }
            }
        }

        Self::cells_to_dsl(&round.cells)
    }

    /// Run-length-compresses a slice of cells into comma-separated DSL
    /// tokens (e.g. `6sc` rather than `sc, sc, sc, sc, sc, sc`), preserving
    /// any label as its own `name!` token ahead of the stitch it marks. A
    /// run only merges into one `Ncount` token when both the stitch kind
    /// *and* the color match (see `stitch_dsl_text`) - `3sc~ff0000` applies
    /// that one color to all three, so cells with differing colors have to
    /// stay separate tokens rather than losing the distinction.
    fn cells_to_dsl(cells: &[GridCell]) -> String {
        let mut tokens: Vec<String> = Vec::new();
        let mut i = 0;
        while i < cells.len() {
            let cell = &cells[i];
            if let Some(label) = &cell.label {
                tokens.push(format!("{label}!"));
            }
            let text = stitch_dsl_text(&cell.kind, cell.color);
            let mut count = 1;
            let mut j = i + 1;
            if cell.label.is_none() {
                while j < cells.len()
                    && cells[j].label.is_none()
                    && stitch_dsl_text(&cells[j].kind, cells[j].color) == text
                {
                    count += 1;
                    j += 1;
                }
            }
            tokens.push(if count > 1 {
                format!("{count}{text}")
            } else {
                text
            });
            i = j;
        }
        tokens.join(", ")
    }
}

fn combine_tension(a: Option<TensionState>, b: Option<TensionState>) -> Option<TensionState> {
    fn rank(t: Option<TensionState>) -> u8 {
        match t {
            Some(TensionState::Stretched) => 3,
            Some(TensionState::Loose) => 2,
            Some(TensionState::Normal) => 1,
            None => 0,
        }
    }
    if rank(a) >= rank(b) {
        a
    } else {
        b
    }
}

/// DSL text for one stitch, including its trailing `~hex` color suffix if
/// it has one (e.g. `dc` vs `dc~ff0000`) - matches the lexer's `~RRGGBB`
/// syntax exactly (see `lexer::Token::HexColor`), so this round-trips
/// through `parser::parse` unchanged.
fn stitch_dsl_text(kind: &StitchKind, color: Option<[u8; 3]>) -> String {
    let base = kind.display_label();
    match color {
        Some([r, g, b]) => format!("{base}~{r:02x}{g:02x}{b:02x}"),
        None => base,
    }
}

/// Mirrors the DSL parser's `modifier.base` handling for the constrained
/// "already validated" text the popup produces - NOT a general DSL parser.
/// Color is handled separately (see `GridCell::color`/`stitch_dsl_text`),
/// not part of this text at all.
fn parse_stitch_text(text: &str) -> StitchKind {
    if let Some((prefix, base)) = text.split_once('.') {
        let inner = Box::new(StitchKind::from_abbrev(base));
        match prefix {
            "flo" => StitchKind::FrontLoopOnly(inner),
            "blo" => StitchKind::BackLoopOnly(inner),
            "fpost" => StitchKind::FrontPost(inner),
            "bpost" => StitchKind::BackPost(inner),
            _ => StitchKind::from_abbrev(text),
        }
    } else {
        StitchKind::from_abbrev(text)
    }
}

fn modifier_idx_for(kind: &StitchKind) -> usize {
    match kind.modifier_prefix() {
        Some("flo") => 1,
        Some("blo") => 2,
        Some("fpost") => 3,
        Some("bpost") => 4,
        _ => 0,
    }
}

fn tension_color(t: Option<TensionState>) -> Color32 {
    match t {
        Some(TensionState::Normal) => Color32::from_rgb(120, 200, 120),
        Some(TensionState::Loose) => Color32::from_rgb(120, 170, 230),
        Some(TensionState::Stretched) => Color32::from_rgb(230, 120, 120),
        None => Color32::from_gray(210),
    }
}

/// Text form of everything this cell's colors encode - tension
/// (background) and yarn color (text) - plus its custom-stitch origin if
/// any, all as one combined hover tooltip. See the call site's comment for
/// why this matters for colorblind accessibility.
fn cell_tooltip(cell: &GridCell) -> String {
    let tension_text = match cell.tension {
        Some(TensionState::Normal) => "tension: normal",
        Some(TensionState::Loose) => "tension: loose",
        Some(TensionState::Stretched) => "tension: stretched",
        None => "tension: not analyzed",
    };
    let mut lines = vec![cell.kind.display_label(), tension_text.to_string()];
    if let Some([r, g, b]) = cell.color {
        lines.push(format!("color: #{r:02x}{g:02x}{b:02x}"));
    }
    if let Some(def_name) = &cell.def_origin {
        lines.push(format!(
            "from custom stitch '{def_name}' - editing anything in this round will flatten it \
             (and any other custom stitch use in this pattern) to raw stitches in the saved DSL \
             text."
        ));
    }
    lines.join("\n")
}

/// Renders the grid, and the add/edit popup if one is open. Returns at most
/// one `GridAction` per frame (egui is immediate-mode; returning an action
/// and letting `GoblinApp` apply + resync keeps ownership simple).
pub fn show(
    ui: &mut egui::Ui,
    state: &GridState,
    selected: &[NodeIndex],
    pending: &mut Option<PendingEdit>,
    recent: &mut super::recent_colors::RecentColors,
) -> Option<GridAction> {
    let mut action = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (round_idx, round) in state.rounds.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("R{round_idx}:"));
                for (cell_idx, cell) in round.cells.iter().enumerate() {
                    let is_selected = cell.nodes.iter().any(|n| selected.contains(n));
                    let mut text = egui::RichText::new(cell.kind.display_label())
                        .background_color(tension_color(cell.tension));
                    // The stitch's own yarn color (if set) drives the
                    // button's *text* color - tension already owns the
                    // background, so this is the one other channel
                    // available without adding a second visual element per
                    // cell. Not a substitute for genuinely rendering yarn
                    // color (e.g. as a swatch), just a quick at-a-glance cue.
                    if let Some([r, g, b]) = cell.color {
                        text = text.color(Color32::from_rgb(r, g, b));
                    }
                    // Italicized as a quiet per-cell echo of the banner
                    // above the grid (see `gui::mod`'s `ViewMode::Grid`
                    // arm) - the banner names *which* custom stitches are
                    // in play; this marks exactly *where*.
                    if cell.def_origin.is_some() {
                        text = text.italics();
                    }
                    if is_selected {
                        text = text.strong().underline();
                    }
                    let mut button = ui.add(egui::Button::new(text));
                    // Tension is coded via background color and yarn color
                    // via text color - two pure-hue channels stacked on one
                    // small widget, which is rough for colorblind users
                    // (and anyone glancing at a screenshot). This tooltip
                    // spells out the same information as text: tension
                    // state by name, the color's hex code, and (unchanged
                    // from before) which custom stitch produced the cell,
                    // if any - one combined tooltip rather than several
                    // separate `on_hover_text` calls, so it reads as one
                    // note instead of a stack of unrelated popups.
                    button = button.on_hover_text(cell_tooltip(cell));
                    if button.clicked() {
                        action = Some(GridAction::Select(cell.nodes.clone()));
                        *pending = Some(PendingEdit {
                            round: round_idx,
                            cell: Some(cell_idx),
                            modifier_idx: modifier_idx_for(&cell.kind),
                            abbrev: cell.kind.base_abbrev(),
                            label: cell.label.clone().unwrap_or_default(),
                            color: cell.color,
                        });
                    }
                    button.context_menu(|ui| {
                        if ui.button("Delete stitch").clicked() {
                            action = Some(GridAction::DeleteCell {
                                round: round_idx,
                                cell: cell_idx,
                            });
                            ui.close_menu();
                        }
                    });
                }
                if ui.button("+").on_hover_text("Add a stitch here").clicked() {
                    *pending = Some(PendingEdit {
                        round: round_idx,
                        cell: None,
                        modifier_idx: 0,
                        abbrev: "sc".to_string(),
                        label: String::new(),
                        color: None,
                    });
                }
                if ui.small_button("del round").clicked() {
                    action = Some(GridAction::DeleteRound { round: round_idx });
                }
            });
        }
        if ui.button("+ round").clicked() {
            action = Some(GridAction::AddRound);
        }
    });

    if let Some(mut edit) = pending.clone() {
        let mut window_open = true;
        let mut close_after = false;
        let title = if edit.cell.is_some() {
            "Edit stitch"
        } else {
            "Add stitch"
        };

        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .open(&mut window_open)
            .show(ui.ctx(), |ui| {
                egui::ComboBox::from_label("Modifier")
                    .selected_text(MODIFIERS[edit.modifier_idx].0)
                    .show_ui(ui, |ui| {
                        for (i, (name, _)) in MODIFIERS.iter().enumerate() {
                            ui.selectable_value(&mut edit.modifier_idx, i, *name);
                        }
                    });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Stitch:");
                    for &p in PALETTE {
                        if ui.selectable_label(edit.abbrev == p, p).clicked() {
                            edit.abbrev = p.to_string();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Custom abbrev:");
                    ui.text_edit_singleline(&mut edit.abbrev);
                });
                ui.horizontal(|ui| {
                    ui.label("Label (optional):");
                    ui.text_edit_singleline(&mut edit.label);
                });
                ui.horizontal(|ui| {
                    let mut has_color = edit.color.is_some();
                    if ui.checkbox(&mut has_color, "Custom color").changed() {
                        edit.color = if has_color {
                            // A neutral mid-gray starting point rather than
                            // defaulting to black, which would read as "I
                            // deliberately chose black" the instant the
                            // checkbox is ticked.
                            Some(edit.color.unwrap_or([180, 180, 180]))
                        } else {
                            None
                        };
                    }
                    if let Some(c) = &mut edit.color {
                        if ui.color_edit_button_srgb(c).changed() {
                            recent.record(*c);
                        }
                    }
                });
                // Quick-pick from colors used recently in either this popup
                // or the colorwork palette - see `recent_colors` module doc.
                if let Some(picked) = recent.show(ui) {
                    edit.color = Some(picked);
                }
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        let modifier_prefix = MODIFIERS[edit.modifier_idx].1;
                        let text = if modifier_prefix.is_empty() {
                            edit.abbrev.clone()
                        } else {
                            format!("{modifier_prefix}.{}", edit.abbrev)
                        };
                        let label_opt = if edit.label.trim().is_empty() {
                            None
                        } else {
                            Some(edit.label.trim().to_string())
                        };
                        action = Some(match edit.cell {
                            Some(cell_idx) => GridAction::EditCell {
                                round: edit.round,
                                cell: cell_idx,
                                text,
                                label: label_opt,
                                color: edit.color,
                            },
                            None => GridAction::AppendStitch {
                                round: edit.round,
                                text,
                                label: label_opt,
                                color: edit.color,
                            },
                        });
                        close_after = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close_after = true;
                    }
                });
            });

        if close_after || !window_open {
            *pending = None;
        } else {
            *pending = Some(edit);
        }
    }

    action
}
