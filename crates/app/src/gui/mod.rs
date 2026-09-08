//! Interactive grid editor + 3D viewport GUI, built on `eframe`/`egui`.
//!
//! Single source of truth is `dsl_source` (the DSL text). The grid editor
//! and 3D viewport are *views* over the compiled `StitchGraph`: editing a
//! grid cell mutates the grid's in-memory representation, re-serializes it
//! back to DSL text, and recompiles from scratch (parse -> eval -> layout
//! -> tension). This keeps the DSL text, the grid, and the 3D view always
//! in sync, at the cost of losing repeat-group/DEF compression on
//! round-trip (see `grid::GridState::to_dsl`) - undo/redo below is the
//! safety net for that, not a fix for it.

pub mod colorwork_grid;
pub mod grid;
pub mod image_import;
pub mod text_import;
pub mod viewport;
pub mod fonts;
pub mod recent_colors;

use abyssal_thread_core::StitchGraph;
use abyssal_thread_lang::Pattern;
use eframe::egui;
use petgraph::graph::NodeIndex;
use std::path::{Path, PathBuf};

/// How many DSL-text snapshots we keep for undo. Cheap to raise; patterns
/// are small text, so 50 snapshots is nothing memory-wise.
const HISTORY_LIMIT: usize = 50;

/// Where the live pattern is continuously autosaved (see
/// `GoblinApp::recompile_from_dsl`'s success path and the
/// `pending_recovery` field) - a fixed, well-known temp path rather than
/// anything per-session, since recovery only means anything if the next
/// launch knows where to look. A crash (or force-quit, or just forgetting
/// to hit Save) before now meant losing whatever DSL text wasn't written
/// to a real `.cgp` file; this is the safety net for that, independent of
/// undo/redo (which is in-memory only and dies with the process too).
fn autosave_path() -> PathBuf {
    // Include the OS username so two accounts on a shared machine can't
    // collide on the same autosave file - temp dirs are per-user on most
    // desktop OSes already, but not guaranteed on every setup, and a
    // tester's in-progress pattern is their own data.
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    std::env::temp_dir().join(format!("abyssal-thread-autosave-{user}.cgp"))
}

/// Whether an expensive follow-up action (here: a full `layout_ring ->
/// relax -> analyze_tension` recompile) should run in response to this
/// frame's change to a `DragValue`/`Slider`. A plain click or keyboard edit
/// applies immediately (`changed()` fires once, `dragged()` is false the
/// whole time). An active click-drag gesture instead fires `changed()` on
/// every pixel of pointer movement - applying on every one of those used
/// to mean one full recompile per pixel dragged, noticeable on anything
/// past amigurumi-scale patterns - so this only reports "commit" once,
/// when the drag actually stops (`drag_stopped()`), regardless of whether
/// this exact frame happened to change the value.
fn commit_after_drag(response: &egui::Response) -> bool {
    response.drag_stopped() || (response.changed() && !response.dragged())
}

/// Which single view fills the central panel. Defaults to `Grid` - the
/// point-and-click cell editor is the primary interface (closer to Stitch
/// Fiddle's workflow); DSL text and the 3D viewport are there when wanted
/// but no longer permanently on screen.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Grid,
    Viewport3D,
    Dsl,
    ImageImport,
    TextImport,
}

pub fn run(initial_input: Option<PathBuf>) -> anyhow::Result<()> {
    let mut app = GoblinApp::default();
    if let Some(path) = initial_input {
        // An explicitly-requested file takes precedence over offering to
        // recover the autosave - the person told us exactly what they
        // want open.
        app.pending_recovery = None;
        app.load_path = path.display().to_string();
        app.load_from_path();
    }

    let native_options = eframe::NativeOptions::default();
    eframe::run_native("abyssal-thread", native_options, Box::new(|_cc| Box::new(app)))
        .map_err(|e| anyhow::anyhow!("gui error: {e}"))
}

pub struct GoblinApp {
    view_mode: ViewMode,
    dsl_source: String,
    /// Snapshot of `dsl_source` taken when the DSL text box gains focus, so
    /// "Apply" can push the *pre-edit* text onto history rather than
    /// clobbering it with what's already in the box.
    dsl_focus_snapshot: Option<String>,
    pattern_name: Option<String>,
    pattern: Option<Pattern>,
    graph: Option<StitchGraph>,
    grid: grid::GridState,
    /// Transient state for the "add/edit stitch" popup - lives here (not in
    /// `GridState`) because it must survive across frames while `GridState`
    /// itself gets wholesale replaced on every recompile.
    edit_target: Option<grid::PendingEdit>,
    viewport: viewport::ViewportState,
    image_import: image_import::ImageImportState,
    text_import: text_import::TextImportState,
    /// `Some` when the currently-loaded pattern is an image-derived
    /// colorwork grid rather than shaped-DSL rounds - drives which editor
    /// the Grid tab shows (see `ViewMode::Grid`'s match arm in `update`)
    /// and is kept in sync with `dsl_source`/`graph` by
    /// `sync_dsl_from_colorwork` and `recompile_from_dsl`.
    colorwork: Option<colorwork_grid::ColorworkGridState>,
    selected_nodes: Vec<NodeIndex>,
    history: Vec<String>,
    future: Vec<String>,
    status: String,
    load_path: String,
    save_path: String,
    export_svg_path: String,
    export_obj_path: String,
    export_legend_path: String,
    export_pdf_path: String,
    print_cell_size_in: f32,
    print_page_size: crate::print::PageSize,
    print_margin_in: f32,
    /// Swatch gauge for shaped patterns: stitches/rows per 4 inches, fed
    /// into `abyssal_thread_layout::Gauge` and threaded through
    /// `layout_ring`/`analyze_tension` in `recompile_from_dsl`. Mirrors the
    /// `gauge_sts_per_4in`/`gauge_rows_per_4in` fields already on
    /// `ImageImportState`/`ColorworkGridState`/`TextImportState`, but drives
    /// 3D layout + tension analysis rather than a pixel grid's stitch
    /// count. Only shown/relevant when `self.colorwork` is `None` (i.e. a
    /// shaped, not colorwork, pattern is loaded) - see the `ViewMode::Grid`
    /// arm in `update`.
    shaped_gauge_sts_per_4in: f32,
    shaped_gauge_rows_per_4in: f32,
    /// Mass-spring relaxation passes run after `layout_ring` (see
    /// `abyssal_thread_layout::relax`) - refines the closed-form ring
    /// layout for asymmetric patterns instead of leaving it as pure
    /// per-round circular placement. 0 disables it.
    shaped_relax_iterations: usize,
    /// `StitchGraph::warnings` from the most recent successful compile
    /// (currently: a raw `DEF` body's `%-N`/`%+N` reference that fell out
    /// of range). Cleared at the start of every `recompile_from_dsl` call
    /// and repopulated only on success, so a stale warning from a
    /// since-fixed pattern doesn't linger. Surfaced via a hoverable
    /// indicator in the bottom status bar.
    compile_warnings: Vec<String>,
    /// Shared recently-used-colors strip between the colorwork palette and
    /// the shaped grid editor's per-stitch color picker - see
    /// `recent_colors` module doc.
    recent_colors: recent_colors::RecentColors,
    /// `Some(recovered_dsl_text)` when a non-empty autosave file was found
    /// at startup, offered to the person via a recovery prompt in `update`
    /// before anything else renders. `None` once they've made a choice
    /// (recover or discard) for this session, or if the CLI was given a
    /// specific file to open instead (see `run`).
    pending_recovery: Option<String>,
}

impl Default for GoblinApp {
    fn default() -> Self {
        // Start directly in a blank colorwork canvas rather than a
        // one-round shaped-stitch placeholder ("6sc") - the Grid tab is
        // meant to be a paintable grid from the moment the app opens, not
        // something that only becomes a real grid after importing an
        // image. `recompile_from_dsl` (called below) already knows how to
        // detect a `COLORGRID:` pattern and populate `self.colorwork`
        // accordingly, so no other startup logic needs to change.
        let blank_grid = abyssal_thread_core::ColorGrid::new(20, 20, [255, 255, 255]);
        let blank_dsl = abyssal_thread_lang::color_grid_to_dsl(Some("untitled"), &blank_grid);

        let mut app = Self {
            view_mode: ViewMode::Grid,
            dsl_source: blank_dsl,
            dsl_focus_snapshot: None,
            pattern_name: Some("untitled".to_string()),
            pattern: None,
            graph: None,
            grid: grid::GridState::default(),
            edit_target: None,
            viewport: viewport::ViewportState::default(),
            image_import: image_import::ImageImportState::default(),
            text_import: text_import::TextImportState::default(),
            colorwork: None,
            selected_nodes: Vec::new(),
            history: Vec::new(),
            future: Vec::new(),
            status: String::new(),
            load_path: String::new(),
            save_path: "pattern.cgp".to_string(),
            export_svg_path: "chart.svg".to_string(),
            export_obj_path: "armature.obj".to_string(),
            export_legend_path: "legend.txt".to_string(),
            export_pdf_path: "pattern.pdf".to_string(),
            // A common cross-stitch/graphgan print size - big enough to
            // mark off with a pencil while working, small enough that a
            // typical pattern doesn't explode into dozens of pages.
            print_cell_size_in: 0.10,
            print_page_size: crate::print::PageSize::US_LETTER,
            print_margin_in: 0.5,
            // Matches `abyssal_thread_layout::Gauge::default()` (a no-op
            // relative to the old hardcoded-only behavior) and the same
            // "typical worsted weight" starting point used by the
            // colorwork gauge fields elsewhere in the GUI.
            shaped_gauge_sts_per_4in: abyssal_thread_layout::REFERENCE_STS_PER_4IN,
            shaped_gauge_rows_per_4in: abyssal_thread_layout::REFERENCE_ROWS_PER_4IN,
            // Matches the CLI build command's `--relax-iterations` default.
            shaped_relax_iterations: 20,
            compile_warnings: Vec::new(),
            recent_colors: recent_colors::RecentColors::default(),
            pending_recovery: None,
        };
        app.recompile_from_dsl();
        // Checked after the fresh blank-canvas state is already fully
        // built and compiled, so declining recovery (or the file being
        // unreadable/empty) just leaves that normal blank start in place -
        // this is purely additive, not a precondition for startup to work.
        if let Ok(recovered) = std::fs::read_to_string(autosave_path()) {
            if !recovered.trim().is_empty() && recovered != app.dsl_source {
                app.pending_recovery = Some(recovered);
            }
        }
        app
    }
}

impl GoblinApp {
    fn recompile_from_dsl(&mut self) {
        self.compile_warnings.clear();
        match abyssal_thread_lang::parser::parse(&self.dsl_source) {
            Ok(pattern) => {
                self.pattern_name = pattern.name.clone();
                match abyssal_thread_lang::eval::eval(&pattern) {
                    Ok(mut g) => {
                        if let Some(color_grid) = &pattern.color_grid {
                            // Colorwork mode: flat-row layout, no tension
                            // analysis (it isn't meaningful for a
                            // straight-down-attached image grid).
                            abyssal_thread_layout::layout_flat_grid(
                                &mut g,
                                abyssal_thread_layout::FLAT_GRID_CELL_MM,
                            );
                            // Update the existing paint-grid state in place
                            // rather than replacing it wholesale. This used
                            // to call `ColorworkGridState::from_grid(...)`
                            // unconditionally, which resets `selected` to 0
                            // (and drops any not-yet-painted "+Add color"
                            // swatches) - since every single paint calls
                            // `sync_dsl_from_colorwork` -> here, that meant
                            // the color selection silently reverted to
                            // black after literally every click.
                            match &mut self.colorwork {
                                Some(existing) => {
                                    existing.grid = color_grid.clone();
                                    existing.sync_pending_from_grid();
                                    existing.mark_texture_dirty();
                                }
                                None => {
                                    self.colorwork = Some(
                                        colorwork_grid::ColorworkGridState::from_grid(color_grid.clone()),
                                    );
                                }
                            }
                        } else {
                            let gauge = abyssal_thread_layout::Gauge {
                                sts_per_4in: self.shaped_gauge_sts_per_4in,
                                rows_per_4in: self.shaped_gauge_rows_per_4in,
                            };
                            abyssal_thread_layout::layout_ring(&mut g, gauge);
                            abyssal_thread_layout::relax(&mut g, gauge, self.shaped_relax_iterations);
                            abyssal_thread_layout::analyze_tension(&mut g, gauge);
                            self.grid = grid::GridState::from_graph(&g);
                            self.colorwork = None;
                        }
                        // e.g. a raw `DEF` body's `%-N`/`%+N` reference that
                        // fell out of range - see `StitchGraph::warnings`'s
                        // doc comment. Cloned ahead of the move into
                        // `self.graph` below.
                        self.compile_warnings = g.warnings.clone();
                        self.graph = Some(g);
                        self.pattern = Some(pattern);
                        self.status = "Compiled OK.".to_string();
                        // Best-effort: a successfully-compiling pattern is
                        // exactly the state worth recovering after a crash
                        // or force-quit. Errors are deliberately swallowed
                        // - this is a safety net running silently in the
                        // background, not a user-facing save action, and a
                        // transient disk issue here shouldn't surface as a
                        // confusing status message overwriting "Compiled OK."
                        let _ = std::fs::write(autosave_path(), &self.dsl_source);
                    }
                    Err(e) => self.status = format!("eval error: {e}"),
                }
            }
            Err(e) => self.status = format!("parse error: {e}"),
        }
    }

    /// Serializes `self.grid` back to DSL text and recompiles. If the
    /// pre-edit grid had any cell derived from a `DEF` custom stitch
    /// (anywhere in the pattern, not just the round actually touched -
    /// `to_dsl` re-serializes every round from scratch each time), this
    /// edit flattens it/them to raw stitches - see `GridCell::def_origin`'s
    /// doc comment for why that's not automatically reconstructed. Rather
    /// than let that happen invisibly, this appends a specific note to the
    /// status bar naming exactly which custom stitch(es) just got
    /// flattened, right when it happens.
    fn sync_dsl_from_grid(&mut self) {
        let flattened = self.grid.def_derived_names();
        let old = self.dsl_source.clone();
        let new = self.grid.to_dsl(self.pattern_name.as_deref());
        if new != old {
            self.push_history(old);
        }
        self.dsl_source = new;
        self.recompile_from_dsl();
        if !flattened.is_empty() {
            self.status = format!(
                "{} NOTE: flattened custom stitch(es) [{}] to raw stitches in the saved DSL \
                 text - their DEF: definitions are untouched, but no round now invokes them by \
                 name. Undo to recover, or edit the DSL tab directly to keep using them.",
                self.status,
                flattened.join(", "),
            );
        }
    }

    /// Mirrors `sync_dsl_from_grid` for the colorwork paint grid: whenever a
    /// cell is painted, re-serialize `self.colorwork`'s `ColorGrid` back to
    /// DSL text and recompile, keeping DSL text and the 3D view in sync
    /// with manual touch-ups the same way the shaped-stitch grid does.
    fn sync_dsl_from_colorwork(&mut self) {
        let Some(state) = &self.colorwork else { return };
        let old = self.dsl_source.clone();
        let new = abyssal_thread_lang::color_grid_to_dsl(self.pattern_name.as_deref(), &state.grid);
        if new != old {
            self.push_history(old);
        }
        self.dsl_source = new;
        self.recompile_from_dsl();
    }

    /// Loads an Image Import result (grid + source photo) as the active
    /// pattern. Builds `self.colorwork` directly via `from_image_import`
    /// *before* calling `recompile_from_dsl` - that function's own
    /// colorwork branch only ever updates an existing `ColorworkGridState`
    /// in place (see its doc comment), so constructing it here first is
    /// what actually attaches the source image; if we instead let
    /// `recompile_from_dsl` create the state (as it would if `self.colorwork`
    /// was still `None`), it would only know about the parsed `ColorGrid`
    /// from DSL text, which has no way to carry an original photo at all.
    fn load_color_grid(&mut self, payload: image_import::GridImportPayload) {
        self.push_history(self.dsl_source.clone());
        self.dsl_source =
            abyssal_thread_lang::color_grid_to_dsl(self.pattern_name.as_deref(), &payload.grid);
        self.colorwork = Some(colorwork_grid::ColorworkGridState::from_image_import(
            payload.grid,
            payload.source,
            payload.colors,
            payload.filter,
        ));
        self.recompile_from_dsl();
        self.view_mode = ViewMode::Grid;
    }

    fn push_history(&mut self, prev: String) {
        self.history.push(prev);
        if self.history.len() > HISTORY_LIMIT {
            self.history.remove(0);
        }
        self.future.clear();
    }

    fn undo(&mut self) {
        if let Some(prev) = self.history.pop() {
            self.future.push(self.dsl_source.clone());
            self.dsl_source = prev;
            self.recompile_from_dsl();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.future.pop() {
            self.history.push(self.dsl_source.clone());
            self.dsl_source = next;
            self.recompile_from_dsl();
        }
    }

    fn load_from_path(&mut self) {
        match std::fs::read_to_string(&self.load_path) {
            Ok(src) => {
                self.push_history(self.dsl_source.clone());
                self.dsl_source = src;
                self.recompile_from_dsl();
            }
            Err(e) => self.status = format!("couldn't read {}: {e}", self.load_path),
        }
    }

    fn save_to_path(&mut self) {
        match std::fs::write(&self.save_path, &self.dsl_source) {
            Ok(()) => self.status = format!("saved to {}", self.save_path),
            Err(e) => self.status = format!("couldn't write {}: {e}", self.save_path),
        }
    }

    fn export_svg(&mut self) {
        // Colorwork patterns need the colored chart (export_color_chart_svg),
        // not export_svg_chart's stitch-symbol chart - the latter draws
        // every stitch as the same "sc" symbol since colorwork graphs are
        // all-SingleCrochet, silently discarding all color information.
        // This was a real bug: exporting SVG from this toolbar while
        // editing a colorwork pattern produced a useless uniform grid
        // instead of the actual image.
        if let Some(state) = &self.colorwork {
            let chart = abyssal_thread_export::export_color_chart_svg(&state.grid);
            match std::fs::write(&self.export_svg_path, chart) {
                Ok(()) => self.status = format!("wrote SVG chart to {}", self.export_svg_path),
                Err(e) => self.status = format!("couldn't write {}: {e}", self.export_svg_path),
            }
            return;
        }
        let Some(g) = &self.graph else {
            self.status = "nothing compiled to export yet".to_string();
            return;
        };
        let chart = abyssal_thread_export::export_svg_chart(g);
        match std::fs::write(&self.export_svg_path, chart) {
            Ok(()) => self.status = format!("wrote SVG chart to {}", self.export_svg_path),
            Err(e) => self.status = format!("couldn't write {}: {e}", self.export_svg_path),
        }
    }

    /// Colorwork-only: writes the hex/name legend for the currently loaded
    /// colorwork pattern. Exposed in the top toolbar (not just the Image
    /// Import tab) so it's still reachable after sending a grid to the
    /// paint editor and making manual touch-ups there.
    fn export_legend(&mut self) {
        let Some(state) = &self.colorwork else {
            self.status = "legend export is only available for colorwork patterns".to_string();
            return;
        };
        let legend = abyssal_thread_export::export_color_grid_legend(&state.grid);
        match std::fs::write(&self.export_legend_path, legend) {
            Ok(()) => self.status = format!("wrote legend to {}", self.export_legend_path),
            Err(e) => self.status = format!("couldn't write {}: {e}", self.export_legend_path),
        }
    }

    /// Colorwork-only, same reasoning as `export_legend`: writes the
    /// multi-page tiled pattern PDF to a user-chosen path (see `print.rs`).
    fn export_pdf(&mut self) {
        let Some(state) = &self.colorwork else {
            self.status = "PDF export is only available for colorwork patterns".to_string();
            return;
        };
        let cell_mm = self.print_cell_size_in * 25.4;
        let name = self.pattern_name.clone().unwrap_or_else(|| "pattern".to_string());
        match crate::print::generate_pattern_pdf(&state.grid, cell_mm, &name, Path::new(&self.export_pdf_path), self.print_page_size, self.print_margin_in * 25.4) {
            Ok(()) => self.status = format!("wrote pattern PDF to {}", self.export_pdf_path),
            Err(e) => self.status = format!("couldn't write {}: {e}", self.export_pdf_path),
        }
    }

    /// Writes the pattern PDF to a temp file and opens it in the OS's
    /// default PDF viewer - from there the user hits that viewer's own
    /// Print button. See the module doc on `print.rs` for why this (rather
    /// than a direct "send to printer" call) is the actual reliable
    /// cross-platform approach.
    fn print_chart(&mut self) {
        let Some(state) = &self.colorwork else {
            self.status = "printing is only available for colorwork patterns".to_string();
            return;
        };
        let cell_mm = self.print_cell_size_in * 25.4;
        let name = self.pattern_name.clone().unwrap_or_else(|| "pattern".to_string());
        match crate::print::print_via_system_default(&state.grid, cell_mm, &name, self.print_page_size, self.print_margin_in * 25.4) {
            Ok(()) => {
                self.status =
                    "opened pattern PDF for printing - select Color (not Grayscale/B&W) in the print dialog"
                        .to_string()
            }
            Err(e) => self.status = format!("couldn't open print dialog: {e}"),
        }
    }

    fn export_obj(&mut self) {
        let Some(g) = &self.graph else {
            self.status = "nothing compiled to export yet".to_string();
            return;
        };
        let obj_text = abyssal_thread_export::export_obj(g);
        match std::fs::write(&self.export_obj_path, obj_text) {
            Ok(()) => self.status = format!("wrote OBJ armature to {}", self.export_obj_path),
            Err(e) => self.status = format!("couldn't write {}: {e}", self.export_obj_path),
        }
    }
}

impl eframe::App for GoblinApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(recovered) = self.pending_recovery.clone() {
            let mut recover = false;
            let mut discard = false;
            egui::Window::new("Recover previous session?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(
                        "Found an autosaved pattern from a previous session (a crash, force-quit, \
                         or just forgetting to hit Save before closing). Recover it, or discard it \
                         and start fresh?",
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Recover").clicked() {
                            recover = true;
                        }
                        if ui.button("Discard").clicked() {
                            discard = true;
                        }
                    });
                });
            if recover {
                self.push_history(self.dsl_source.clone());
                self.dsl_source = recovered;
                self.recompile_from_dsl();
                self.pending_recovery = None;
            } else if discard {
                self.pending_recovery = None;
            }
            // Nothing else renders underneath while this is up - the
            // person needs to make this call before touching anything
            // else, since "Discard" and continuing to edit are
            // indistinguishable to the rest of the app otherwise.
            return;
        }

        let undo_pressed =
            ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z) && !i.modifiers.shift);
        let redo_pressed = ctx.input(|i| {
            i.modifiers.command
                && ((i.key_pressed(egui::Key::Z) && i.modifiers.shift) || i.key_pressed(egui::Key::Y))
        });
        if undo_pressed {
            self.undo();
        }
        if redo_pressed {
            self.redo();
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!self.history.is_empty(), egui::Button::new("\u{21b6} Undo")).clicked() {
                    self.undo();
                }
                if ui.add_enabled(!self.future.is_empty(), egui::Button::new("\u{21b7} Redo")).clicked() {
                    self.redo();
                }
                ui.separator();

                if ui.button("Open...").clicked() {
                    if let Some(path) =
                        rfd::FileDialog::new().add_filter("Crochet pattern", &["cgp"]).pick_file()
                    {
                        self.load_path = path.display().to_string();
                        self.load_from_path();
                    }
                }
                ui.label(&self.load_path);
                ui.separator();

                if ui.button("Save").clicked() {
                    self.save_to_path();
                }
                if ui.button("Save As...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Crochet pattern", &["cgp"])
                        .set_file_name(&self.save_path)
                        .save_file()
                    {
                        self.save_path = path.display().to_string();
                        self.save_to_path();
                    }
                }
                ui.label(&self.save_path);
                ui.separator();

                if ui.button("Export SVG...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("SVG", &["svg"])
                        .set_file_name(&self.export_svg_path)
                        .save_file()
                    {
                        self.export_svg_path = path.display().to_string();
                        self.export_svg();
                    }
                }
                if ui.button("Export OBJ...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Wavefront OBJ", &["obj"])
                        .set_file_name(&self.export_obj_path)
                        .save_file()
                    {
                        self.export_obj_path = path.display().to_string();
                        self.export_obj();
                    }
                }
                if ui
                    .add_enabled(
                        self.colorwork.is_some(),
                        egui::Button::new("Export legend..."),
                    )
                    .on_hover_text("Colorwork patterns only - hex/name legend for the current colors.")
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Text", &["txt"])
                        .set_file_name(&self.export_legend_path)
                        .save_file()
                    {
                        self.export_legend_path = path.display().to_string();
                        self.export_legend();
                    }
                }
                if ui
                    .add_enabled(self.colorwork.is_some(), egui::Button::new("Export PDF..."))
                    .on_hover_text("Colorwork patterns only - multi-page tiled printable chart, cross-stitch-pattern style.")
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("PDF", &["pdf"])
                        .set_file_name(&self.export_pdf_path)
                        .save_file()
                    {
                        self.export_pdf_path = path.display().to_string();
                        self.export_pdf();
                    }
                }
                ui.label("Print cell size:");
                ui.add(
                    egui::DragValue::new(&mut self.print_cell_size_in)
                        .clamp_range(0.1..=1.0)
                        .speed(0.01)
                        .suffix(" in"),
                );
                ui.label("Page:");
                ui.radio_value(&mut self.print_page_size, crate::print::PageSize::US_LETTER, "Letter");
                ui.radio_value(&mut self.print_page_size, crate::print::PageSize::A4, "A4");
                ui.label("Margin:");
                ui.add(
                    egui::DragValue::new(&mut self.print_margin_in)
                        .clamp_range(0.1..=2.0)
                        .speed(0.01)
                        .suffix(" in"),
                );
                if ui
                    .add_enabled(self.colorwork.is_some(), egui::Button::new("Print..."))
                    .on_hover_text("Opens the pattern as a PDF in your default viewer, ready to print from there.")
                    .clicked()
                {
                    self.print_chart();
                }
            });
        });

        egui::TopBottomPanel::top("view_tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.view_mode, ViewMode::Grid, "\u{1F9F6} Grid");
                ui.selectable_value(&mut self.view_mode, ViewMode::Viewport3D, "\u{1F9CA} 3D");
                ui.selectable_value(&mut self.view_mode, ViewMode::Dsl, "\u{1F4DD} DSL");
                ui.selectable_value(&mut self.view_mode, ViewMode::ImageImport, "\u{1F5BC} Image import");
                ui.selectable_value(&mut self.view_mode, ViewMode::TextImport, "\u{1F524} Text");
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                if let Some(g) = &self.graph {
                    let report = abyssal_thread_layout::summarize_tension(g);
                    ui.separator();
                    ui.label(format!(
                        "{} stitches, {} rounds - {} normal / {} loose / {} stretched",
                        g.stitch_count(),
                        g.round_count(),
                        report.normal,
                        report.loose,
                        report.stretched,
                    ));
                }
                if !self.compile_warnings.is_empty() {
                    ui.separator();
                    let n = self.compile_warnings.len();
                    ui.colored_label(
                        egui::Color32::from_rgb(200, 140, 0),
                        format!("\u{26a0} {n} warning{}", if n == 1 { "" } else { "s" }),
                    )
                    .on_hover_text(self.compile_warnings.join("\n"));
                }
            });
        });

        match self.view_mode {
            ViewMode::Dsl => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("DSL");
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut self.dsl_source)
                            .desired_rows(30)
                            .code_editor(),
                    );
                    if response.gained_focus() {
                        self.dsl_focus_snapshot = Some(self.dsl_source.clone());
                    }
                    if ui.button("Apply DSL \u{2192} graph").clicked() {
                        if let Some(prev) = self.dsl_focus_snapshot.take() {
                            if prev != self.dsl_source {
                                self.push_history(prev);
                            }
                        }
                        self.recompile_from_dsl();
                    }
                });
            }
            ViewMode::Viewport3D => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("3D inspection");
                        ui.checkbox(&mut self.viewport.shaded, "Shaded");
                        if ui.button("Reset view").clicked() {
                            self.viewport = viewport::ViewportState::default();
                        }
                    });
                    if let Some(clicked) =
                        viewport::show(ui, self.graph.as_ref(), &mut self.viewport, &self.selected_nodes)
                    {
                        self.selected_nodes = vec![clicked];
                    }
                });
            }
            ViewMode::ImageImport => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Convert a picture into a colorwork chart");
                    if let Some(payload) = image_import::show(ui, &mut self.image_import) {
                        self.load_color_grid(payload);
                    }
                });
            }
            ViewMode::TextImport => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Type words into a colorwork chart");
                    if let Some(payload) = text_import::show(ui, &mut self.text_import) {
                        self.load_color_grid(payload);
                    }
                });
            }
            ViewMode::Grid => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let mut colorwork_modified = false;
                    if self.colorwork.is_some() {
                        ui.heading("Colorwork grid editor");
                        if let Some(state) = &mut self.colorwork {
                            colorwork_modified = colorwork_grid::show(ui, state, &mut self.recent_colors);
                        }
                    } else {
                        ui.heading("Grid editor");
                        ui.label("Click a cell to edit it, right-click to delete, + to append.");
                        let def_names = self.grid.def_derived_names();
                        if !def_names.is_empty() {
                            egui::Frame::none()
                                .fill(egui::Color32::from_rgb(90, 70, 20))
                                .inner_margin(6.0)
                                .show(ui, |ui| {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(255, 210, 120),
                                        format!(
                                            "\u{26a0} This pattern uses custom stitch(es) [{}] \
                                             (italicized cells below). Editing anything in this \
                                             grid flattens them to raw stitches the moment you \
                                             save - their DEF: definitions stay intact, but no \
                                             round will invoke them by name anymore. Edit the DSL \
                                             tab directly instead if you want to keep using them.",
                                            def_names.join(", "),
                                        ),
                                    );
                                });
                        }
                        ui.horizontal(|ui| {
                            ui.label("Gauge (sts/rows per 4in):");
                            let sts_resp = ui.add(
                                egui::DragValue::new(&mut self.shaped_gauge_sts_per_4in)
                                    .clamp_range(1.0..=200.0)
                                    .speed(0.1),
                            );
                            let rows_resp = ui.add(
                                egui::DragValue::new(&mut self.shaped_gauge_rows_per_4in)
                                    .clamp_range(1.0..=200.0)
                                    .speed(0.1),
                            );
                            ui.separator();
                            ui.label("Relax iterations:");
                            let relax_resp = ui.add(
                                egui::DragValue::new(&mut self.shaped_relax_iterations)
                                    .clamp_range(0..=500),
                            );
                            // Recompiling means a full layout_ring -> relax
                            // -> analyze_tension pass - cheap for a quick
                            // click/keyboard edit, but a *drag* gesture
                            // fires `changed()` on every pixel of pointer
                            // movement, which used to mean one full pass
                            // per pixel. `commit_after_drag` only fires
                            // once, when the drag actually stops (a plain
                            // click/type-and-tab edit still applies right
                            // away, since it was never "dragged" at all).
                            if commit_after_drag(&sts_resp)
                                || commit_after_drag(&rows_resp)
                                || commit_after_drag(&relax_resp)
                            {
                                self.recompile_from_dsl();
                            }
                        });
                        if let Some(action) = grid::show(
                            ui,
                            &self.grid,
                            &self.selected_nodes,
                            &mut self.edit_target,
                            &mut self.recent_colors,
                        ) {
                            match action {
                                grid::GridAction::Select(nodes) => self.selected_nodes = nodes,
                                other => {
                                    self.grid.apply(other);
                                    self.sync_dsl_from_grid();
                                }
                            }
                        }
                    }
                    // Deliberately outside the borrow of `self.colorwork`
                    // above (which ends at that `if let` block's closing
                    // brace) rather than calling `self.sync_dsl_from_colorwork()`
                    // from inside it, so this doesn't depend on subtle
                    // borrow-checker/NLL reasoning to compile.
                    if colorwork_modified {
                        self.sync_dsl_from_colorwork();
                    }
                });
            }
        }
    }
}
