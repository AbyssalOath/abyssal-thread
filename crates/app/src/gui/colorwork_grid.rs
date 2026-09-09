//! A Stitch-Fiddle-style pixel paint grid for colorwork/image-derived
//! patterns: a palette of colors down the top, and a paintable grid below.
//!
//! IMPORTANT PERFORMANCE NOTE: the first version of this file rendered one
//! interactive egui widget *per cell* (`allocate_exact_size` +
//! `Sense::click_and_drag()` inside a nested per-row/per-column loop). For
//! a 200x150 image-import grid that's 30,000 individual widgets, every one
//! of them re-evaluated and hit-tested on *every single frame* at 60fps -
//! this pegged a CPU core and nearly locked up the reporting machine. The
//! fix below renders the entire grid as a single GPU texture (one upload,
//! only when a cell actually changes) displayed via one `Painter::image`
//! call, and handles click/drag painting through exactly one interactive
//! region, mapping the pointer position to a cell index arithmetically
//! instead of via per-cell widgets. This is O(1) widgets regardless of
//! grid size instead of O(width * height).

use abyssal_thread_core::ColorGrid;
use abyssal_thread_imageimport::ResizeFilter;
use eframe::egui::{self, Color32, ColorImage, TextureHandle, TextureOptions};
use image::DynamicImage;

const CELL_SIZE: f32 = 14.0;

/// Whether the canvas-size controls are driven by direct stitch counts or
/// by a desired finished size (converted through the gauge below). Mirrors
/// `image_import::SizeMode` - kept as a separate copy rather than a shared
/// type so this module stays self-contained, consistent with how
/// `image_import.rs` and this file are already independent GUI modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SizeMode {
    Stitches,
    Inches,
}

/// Whether clicking/dragging paints just the cell under the pointer, or
/// flood-fills the whole contiguous same-color region it belongs to
/// (classic "paint bucket" behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaintMode {
    Pixel,
    Bucket,
}

pub struct ColorworkGridState {
    pub grid: ColorGrid,
    pub palette: Vec<[u8; 3]>,
    pub selected: usize,
    /// Swatch currently being edited in the color picker, kept as
    /// persistent state so the picker doesn't reset every frame.
    pub new_color: [u8; 3],
    paint_mode: PaintMode,
    /// Cached render of the whole grid. Rebuilt only when `texture_dirty`
    /// is set (i.e. a cell actually changed color) - most frames do zero
    /// texture uploads.
    texture: Option<TextureHandle>,
    texture_dirty: bool,
    size_mode: SizeMode,
    gauge_sts_per_4in: f32,
    gauge_rows_per_4in: f32,
    desired_width_in: f32,
    desired_height_in: f32,
    /// Staged width/height (stitches) for the "Canvas size" controls -
    /// separate from `grid.width`/`grid.height` so typing in the field
    /// doesn't resize the canvas on every keystroke; `resize_canvas` is
    /// what actually applies these.
    pending_width: u32,
    pending_height: u32,
    /// The original photo, if this pattern came from Image Import - kept so
    /// `resize_canvas` can re-render from source at the new size (matching
    /// Image Import's own resize behavior) instead of just cropping/padding
    /// the already-quantized grid. `None` for a hand-painted/blank-started
    /// canvas, where crop/pad (the only thing possible without a source) is
    /// the correct and only option anyway.
    source_image: Option<DynamicImage>,
    colors: usize,
    filter: ResizeFilter,
}

impl ColorworkGridState {
    pub fn from_grid(grid: ColorGrid) -> Self {
        let mut palette = grid.palette();
        if palette.is_empty() {
            palette.push([255, 255, 255]);
        }
        let mut state = Self {
            grid,
            palette,
            selected: 0,
            new_color: [128, 128, 128],
            paint_mode: PaintMode::Pixel,
            texture: None,
            texture_dirty: true,
            size_mode: SizeMode::Stitches,
            gauge_sts_per_4in: 16.0,
            gauge_rows_per_4in: 16.0,
            desired_width_in: 0.0,
            desired_height_in: 0.0,
            pending_width: 0,
            pending_height: 0,
            source_image: None,
            colors: 4,
            filter: ResizeFilter::Nearest,
        };
        state.sync_pending_from_grid();
        state.sync_inches_from_stitches();
        state
    }

    /// Like `from_grid`, but also keeps the original source image (and the
    /// color count/resize filter Image Import used) so `resize_canvas` can
    /// re-render from source instead of cropping/padding. Used by
    /// `GoblinApp::load_color_grid` when a grid arrives via "Send to Grid
    /// editor" from the Image Import tab.
    pub fn from_image_import(
        grid: ColorGrid,
        source: DynamicImage,
        colors: usize,
        filter: ResizeFilter,
    ) -> Self {
        let mut state = Self::from_grid(grid);
        state.source_image = Some(source);
        state.colors = colors;
        state.filter = filter;
        state
    }

    fn rebuild_texture_if_needed(&mut self, ctx: &egui::Context) {
        if self.texture.is_some() && !self.texture_dirty {
            return;
        }
        let mut pixels = Vec::with_capacity(self.grid.width * self.grid.height);
        for y in 0..self.grid.height {
            for x in 0..self.grid.width {
                let [r, g, b] = self.grid.get(x, y);
                pixels.push(Color32::from_rgb(r, g, b));
            }
        }
        let image = ColorImage {
            size: [self.grid.width, self.grid.height],
            pixels,
        };
        self.texture =
            Some(ctx.load_texture("colorwork_paint_grid", image, TextureOptions::NEAREST));
        self.texture_dirty = false;
    }

    /// Forces the cached texture to be rebuilt next frame. Call this after
    /// replacing `grid`'s contents from outside `show` (e.g. when a DSL
    /// recompile updates the grid in place - see `GoblinApp::recompile_from_dsl`).
    pub fn mark_texture_dirty(&mut self) {
        self.texture_dirty = true;
    }

    /// Re-reads `pending_width`/`pending_height` from the current
    /// `grid` dimensions. Call this after `grid` changes from outside
    /// `resize_canvas` (e.g. `GoblinApp::recompile_from_dsl` replacing
    /// `grid` wholesale) so the size controls don't show stale numbers.
    pub fn sync_pending_from_grid(&mut self) {
        self.pending_width = self.grid.width as u32;
        self.pending_height = self.grid.height as u32;
    }

    fn sts_per_in(&self) -> f32 {
        (self.gauge_sts_per_4in / 4.0).max(0.01)
    }

    fn rows_per_in(&self) -> f32 {
        (self.gauge_rows_per_4in / 4.0).max(0.01)
    }

    fn apply_gauge_size(&mut self) {
        self.pending_width = ((self.desired_width_in * self.sts_per_in()).round() as u32).max(1);
        self.pending_height = ((self.desired_height_in * self.rows_per_in()).round() as u32).max(1);
    }

    fn sync_inches_from_stitches(&mut self) {
        self.desired_width_in = self.pending_width as f32 / self.sts_per_in();
        self.desired_height_in = self.pending_height as f32 / self.rows_per_in();
    }

    /// Resizes the live canvas to `new_width` x `new_height`.
    ///
    /// When this pattern came from Image Import (`source_image` is
    /// `Some`), re-renders straight from the original photo at the new
    /// size/color count - the exact same resize+quantize pipeline Image
    /// Import itself uses. This is the fix for a real bug: adjusting size
    /// here used to just crop/pad the already-quantized grid, leaving the
    /// old content pinned in the top-left corner with blank space around
    /// it instead of actually rescaling the picture.
    ///
    /// For a hand-painted/blank-started canvas (`source_image` is `None`),
    /// there's no source to re-render from, so this falls back to
    /// crop/pad anchored top-left: existing painted cells within the
    /// overlapping region are kept, new area is filled white, and area
    /// outside the new bounds is cropped away.
    /// Sets every cell in the canvas to `color` - the "splash"/background-
    /// fill request: paint the whole background in one click instead of
    /// clicking every cell individually.
    fn fill_all(&mut self, color: [u8; 3]) {
        for cell in self.grid.cells.iter_mut() {
            *cell = color;
        }
        self.texture_dirty = true;
    }

    /// Classic paint-bucket: flood-fills the contiguous region of cells
    /// matching the color at `(start_x, start_y)` with `new_color`, using
    /// 4-connectivity (up/down/left/right, not diagonals - the standard
    /// choice for pixel-art bucket tools). No-ops if the target color
    /// already matches (avoids an infinite/wasted fill).
    fn flood_fill(&mut self, start_x: usize, start_y: usize, new_color: [u8; 3]) {
        let target = self.grid.get(start_x, start_y);
        if target == new_color {
            return;
        }
        let (w, h) = (self.grid.width, self.grid.height);
        let mut stack = vec![(start_x, start_y)];
        while let Some((x, y)) = stack.pop() {
            if self.grid.get(x, y) != target {
                continue;
            }
            self.grid.set(x, y, new_color);
            if x + 1 < w {
                stack.push((x + 1, y));
            }
            if x > 0 {
                stack.push((x - 1, y));
            }
            if y + 1 < h {
                stack.push((x, y + 1));
            }
            if y > 0 {
                stack.push((x, y - 1));
            }
        }
        self.texture_dirty = true;
    }

    fn resize_canvas(&mut self, new_width: usize, new_height: usize) {
        let new_width = new_width.max(1);
        let new_height = new_height.max(1);

        if let Some(source) = &self.source_image {
            let resized = abyssal_thread_imageimport::resize_exact(
                source,
                new_width as u32,
                new_height as u32,
                self.filter,
            );
            self.grid = abyssal_thread_imageimport::quantize(&resized, self.colors);
            // Refresh the palette to match what's actually achievable at
            // the new size/color count - safe to do here (unlike on every
            // paint stroke, which was the earlier "selected color resets"
            // bug) since this only runs on an explicit size/color change.
            self.palette = self.grid.palette();
            if self.palette.is_empty() {
                self.palette.push([255, 255, 255]);
            }
            self.selected = self.selected.min(self.palette.len() - 1);
            self.texture_dirty = true;
            return;
        }

        if new_width == self.grid.width && new_height == self.grid.height {
            return;
        }
        let mut new_grid = ColorGrid::new(new_width, new_height, [255, 255, 255]);
        let copy_w = new_width.min(self.grid.width);
        let copy_h = new_height.min(self.grid.height);
        for y in 0..copy_h {
            for x in 0..copy_w {
                new_grid.set(x, y, self.grid.get(x, y));
            }
        }
        self.grid = new_grid;
        self.texture_dirty = true;
    }
}

/// Renders the palette + paint grid. Returns `true` if any cell's color
/// changed this frame - the caller should re-derive the `StitchGraph`/DSL
/// text from `state.grid` when it does (see `GoblinApp::sync_dsl_from_colorwork`).
/// `recent` is the app-wide recently-used-colors strip shared with the
/// shaped-pattern grid editor's per-stitch color picker (see
/// `recent_colors` module doc) - colors added to this palette get recorded
/// there too, and clicking a recent swatch here quick-adds/-selects it.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut ColorworkGridState,
    recent: &mut crate::gui::recent_colors::RecentColors,
) -> bool {
    let mut modified = false;

    ui.heading("Canvas size");
    let mut gauge_changed = false;
    ui.horizontal(|ui| {
        ui.label("Set size by:");
        ui.radio_value(&mut state.size_mode, SizeMode::Stitches, "Stitch count");
        if ui
            .radio_value(
                &mut state.size_mode,
                SizeMode::Inches,
                "Finished size (inches)",
            )
            .clicked()
        {
            state.sync_inches_from_stitches();
        }
    });
    ui.horizontal(|ui| {
        ui.label("Gauge:");
        gauge_changed |= ui
            .add(
                egui::DragValue::new(&mut state.gauge_sts_per_4in)
                    .clamp_range(1.0..=200.0)
                    .speed(0.1),
            )
            .changed();
        ui.label("sts,");
        gauge_changed |= ui
            .add(
                egui::DragValue::new(&mut state.gauge_rows_per_4in)
                    .clamp_range(1.0..=200.0)
                    .speed(0.1),
            )
            .changed();
        ui.label("rows, per 4 inches");
    });

    let mut dims_changed = false;
    match state.size_mode {
        SizeMode::Stitches => {
            ui.horizontal(|ui| {
                ui.label("Width (stitches):");
                dims_changed |= ui
                    .add(egui::Slider::new(&mut state.pending_width, 1..=400))
                    .changed();
                dims_changed |= ui
                    .add(egui::DragValue::new(&mut state.pending_width))
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Height (rows):");
                dims_changed |= ui
                    .add(egui::Slider::new(&mut state.pending_height, 1..=400))
                    .changed();
                dims_changed |= ui
                    .add(egui::DragValue::new(&mut state.pending_height))
                    .changed();
            });
        }
        SizeMode::Inches => {
            let mut inches_changed = false;
            ui.horizontal(|ui| {
                ui.label("Width (inches):");
                inches_changed |= ui
                    .add(
                        egui::DragValue::new(&mut state.desired_width_in)
                            .clamp_range(0.5..=200.0)
                            .speed(0.1),
                    )
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Height (inches):");
                inches_changed |= ui
                    .add(
                        egui::DragValue::new(&mut state.desired_height_in)
                            .clamp_range(0.5..=200.0)
                            .speed(0.1),
                    )
                    .changed();
            });
            if inches_changed || gauge_changed {
                state.apply_gauge_size();
                dims_changed = true;
            }
        }
    }

    if dims_changed {
        let (w, h) = (state.pending_width as usize, state.pending_height as usize);
        state.resize_canvas(w, h);
        state.sync_pending_from_grid();
        modified = true;
    }

    let mut colors_changed = false;
    if state.source_image.is_some() {
        ui.horizontal(|ui| {
            ui.label("Colors:");
            colors_changed |= ui
                .add(egui::Slider::new(&mut state.colors, 1..=16))
                .changed();
        });
        if colors_changed {
            let (w, h) = (state.pending_width as usize, state.pending_height as usize);
            state.resize_canvas(w, h);
            modified = true;
        }
    }

    let est_w_in = state.grid.width as f32 / state.sts_per_in();
    let est_h_in = state.grid.height as f32 / state.rows_per_in();
    ui.label(format!(
        "Stitch count: {} ({} x {}) \u{2192} approx finished size at this gauge: {:.1} in x {:.1} in",
        state.grid.width * state.grid.height,
        state.grid.width,
        state.grid.height,
        est_w_in,
        est_h_in,
    ));

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Palette:");
        let mut to_remove = None;
        for (i, &[r, g, b]) in state.palette.clone().iter().enumerate() {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
            let stroke = if i == state.selected {
                egui::Stroke::new(2.0_f32, Color32::WHITE)
            } else {
                egui::Stroke::new(1.0_f32, Color32::from_gray(90))
            };
            ui.painter().rect_stroke(rect, 2.0, stroke);
            if response.clicked() {
                state.selected = i;
            }
            // Must always keep at least one color to paint with.
            if response.secondary_clicked() && state.palette.len() > 1 {
                to_remove = Some(i);
            }
        }
        if let Some(i) = to_remove {
            state.palette.remove(i);
            if state.selected >= state.palette.len() {
                state.selected = state.palette.len() - 1;
            } else if state.selected > i {
                state.selected -= 1;
            }
        }

        ui.separator();
        egui::color_picker::color_edit_button_srgb(ui, &mut state.new_color);
        if ui.button("+ Add color").clicked() {
            state.palette.push(state.new_color);
            state.selected = state.palette.len() - 1;
            recent.record(state.new_color);
        }
    });
    ui.small("Left-click a swatch to select it, right-click to remove it from the palette.");
    if let Some(picked) = recent.show(ui) {
        // Quick-add/-select rather than blindly pushing a duplicate -
        // reusing a color someone already picked here or in the shaped
        // grid editor should feel like picking it, not cluttering the
        // palette with two swatches of the same color.
        match state.palette.iter().position(|&c| c == picked) {
            Some(i) => state.selected = i,
            None => {
                state.palette.push(picked);
                state.selected = state.palette.len() - 1;
            }
        }
    }

    ui.horizontal(|ui| {
        if ui
            .button("Fill All")
            .on_hover_text("Fills the whole canvas with the selected color - handy for setting a background before painting a design on top.")
            .clicked()
        {
            let color = state.palette[state.selected];
            state.fill_all(color);
            modified = true;
        }
        ui.separator();
        ui.label("Paint mode:");
        ui.radio_value(&mut state.paint_mode, PaintMode::Pixel, "Pixel");
        ui.radio_value(&mut state.paint_mode, PaintMode::Bucket, "Bucket fill")
            .on_hover_text("Click a cell to flood-fill its whole connected same-color region, like a paint-bucket tool.");
    });

    ui.separator();
    ui.label("Click, or click-and-drag, to paint. Scroll to pan around large grids.");

    state.rebuild_texture_if_needed(ui.ctx());
    let Some(texture) = state.texture.clone() else {
        return modified;
    };

    let display_size = egui::vec2(
        state.grid.width as f32 * CELL_SIZE,
        state.grid.height as f32 * CELL_SIZE,
    );

    egui::ScrollArea::both().show(ui, |ui| {
        let (rect, response) = ui.allocate_exact_size(display_size, egui::Sense::click_and_drag());
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );

        // Cell-boundary overlay, drawn as plain line segments rather than
        // per-cell widgets - a few hundred cheap paint calls at most, not
        // the O(width*height) widget cost this file used to have. Purely
        // visual: lets you see individual cells while painting, matching
        // Stitch Fiddle's grid lines.
        let line_stroke = egui::Stroke::new(1.0_f32, Color32::from_black_alpha(60));
        for col in 0..=state.grid.width {
            let x = rect.min.x + col as f32 * CELL_SIZE;
            ui.painter().line_segment(
                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                line_stroke,
            );
        }
        for row in 0..=state.grid.height {
            let y = rect.min.y + row as f32 * CELL_SIZE;
            ui.painter().line_segment(
                [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                line_stroke,
            );
        }

        let pointer_down = ui.input(|i| i.pointer.primary_down());
        // Bucket fill only triggers on a discrete click, never while
        // dragging - continuously re-flood-filling on every drag frame
        // would be surprising (and wasteful) compared to pixel mode's
        // paint-while-dragging behavior.
        let painting = match state.paint_mode {
            PaintMode::Pixel => response.hovered() && (pointer_down || response.clicked()),
            PaintMode::Bucket => response.clicked(),
        };
        if painting {
            // `hover_pos()` (rather than `interact_pointer_pos()`) so this
            // doesn't depend on exactly when egui considers a widget
            // "interacted with" - just: where is the pointer right now,
            // is it inside our rect, which cell does that map to.
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                if rect.contains(pos) {
                    let local = pos - rect.min;
                    let cx = (local.x / CELL_SIZE) as usize;
                    let cy = (local.y / CELL_SIZE) as usize;
                    if cx < state.grid.width && cy < state.grid.height {
                        let new_color = state.palette[state.selected];
                        match state.paint_mode {
                            PaintMode::Pixel => {
                                if state.grid.get(cx, cy) != new_color {
                                    state.grid.set(cx, cy, new_color);
                                    state.texture_dirty = true;
                                    modified = true;
                                }
                            }
                            PaintMode::Bucket => {
                                if state.grid.get(cx, cy) != new_color {
                                    state.flood_fill(cx, cy, new_color);
                                    modified = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    modified
}
