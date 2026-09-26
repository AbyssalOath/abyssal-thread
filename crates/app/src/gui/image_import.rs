//! Image -> colorwork grid import panel. Wraps `abyssal_thread_imageimport`
//! with interactive size/color controls, addressing the specific complaint
//! that drove this module: being stuck with whatever size auto-detect
//! picked. Width, height, and color count are all live sliders/fields that
//! re-run the resize+quantize pipeline and redraw the preview immediately -
//! there's no "convert" step to commit to before you can see whether a size
//! is too big or too small.
//!
//! This is intentionally NOT wired into `GridState`/`StitchGraph`/the DSL -
//! see the doc comment on `ColorGrid` in `abyssal-thread-core` for why a flat
//! colorwork grid is a different data model from round/row shaping. This
//! panel's output (a `ColorGrid`) is exported directly to SVG/legend text,
//! independent of whatever's loaded in the Grid/3D/DSL tabs.

use abyssal_thread_core::ColorGrid;
use abyssal_thread_imageimport::{
    quantize, resize_exact, resize_preserving_aspect, threshold, ResizeFilter,
};
use eframe::egui::{self, Color32, ColorImage, TextureHandle, TextureOptions};
use image::DynamicImage;

use super::colorwork_grid::FiletColors;
use super::crossstitch_grid::{chart_from_image, convert_settings_ui, size_ui, ConvertSettings};
use abyssal_thread_core::Craft;
use abyssal_thread_crossstitch::{Chart, Fabric, GridCraft};

/// Whether the size controls are driven by direct stitch counts or by a
/// desired finished size (converted through the gauge below). Switching
/// modes doesn't lose your place - see the `clicked()` handlers in `show`
/// that sync one representation from the other when you switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SizeMode {
    Stitches,
    Inches,
}

pub struct ImageImportState {
    source_path: String,
    source: Option<DynamicImage>,
    width: u32,
    height: u32,
    lock_aspect: bool,
    colors: usize,
    filter: ResizeFilter,
    size_mode: SizeMode,
    /// Filet crochet mode: reduces to exactly two colors (block/space) by
    /// luminance threshold instead of the arbitrary-palette k-means
    /// `quantize` path - see the module doc on `crate::gui::colorwork_grid`
    /// for why this stays a `ColorGrid`-only feature rather than flowing
    /// through the same stitch-graph pipeline solid colorwork does.
    filet_mode: bool,
    fill_threshold: u8,
    invert_threshold: bool,
    block_color: [u8; 3],
    space_color: [u8; 3],
    /// Gauge as printed on a yarn label / measured from a swatch: stitches
    /// and rows per 4 inches (not per inch directly) - stitches and rows
    /// almost always need separate values since crochet stitches aren't
    /// square, which is exactly why width and height need independent
    /// inch->stitch conversions rather than one uniform scale factor.
    gauge_sts_per_4in: f32,
    gauge_rows_per_4in: f32,
    desired_width_in: f32,
    desired_height_in: f32,
    grid: Option<ColorGrid>,
    /// Craft the preview is for (the loaded pattern's craft, passed in to
    /// `show` every frame). For cross stitch, `grid` is the DMC-snapped
    /// rendering of `chart` rather than the raw k-means palette, so the
    /// preview shows the floss colors you'll actually stitch with.
    craft: Craft,
    fabric_count: f32,
    /// Row gauge for knitting charts (`None` = square cells).
    fabric_count_y: Option<f32>,
    convert: ConvertSettings,
    chart: Option<Chart>,
    preview_texture: Option<TextureHandle>,
    export_svg_path: String,
    export_legend_path: String,
    status: String,
}

impl Default for ImageImportState {
    fn default() -> Self {
        Self {
            source_path: String::new(),
            source: None,
            width: 60,
            height: 60,
            lock_aspect: true,
            colors: 4,
            filter: ResizeFilter::Nearest,
            size_mode: SizeMode::Stitches,
            filet_mode: false,
            fill_threshold: 128,
            invert_threshold: false,
            block_color: [0, 0, 0],
            space_color: [255, 255, 255],
            // Sensible starting point, not a substitute for your actual
            // swatch - typical worsted-weight single crochet runs somewhere
            // near this, but gauge varies by yarn, hook, and crocheter.
            gauge_sts_per_4in: 16.0,
            gauge_rows_per_4in: 16.0,
            desired_width_in: 15.0,
            desired_height_in: 15.0,
            grid: None,
            craft: Craft::Crochet,
            fabric_count: 14.0,
            fabric_count_y: None,
            convert: ConvertSettings::default(),
            chart: None,
            preview_texture: None,
            export_svg_path: "colorwork_chart.svg".to_string(),
            export_legend_path: "colorwork_legend.txt".to_string(),
            status: String::new(),
        }
    }
}

impl ImageImportState {
    fn load(&mut self, path: std::path::PathBuf) {
        match abyssal_thread_imageimport::load_image(&path) {
            Ok(img) => {
                // Default to the source's own size (capped so a huge photo
                // doesn't immediately produce an unreadable/enormous grid),
                // preserving aspect - a sane starting point the user then
                // adjusts, rather than an arbitrary fixed default.
                let (w, h) = (img.width(), img.height());
                let cap = 150u32;
                let scale = (cap as f32 / w.max(h) as f32).min(1.0);
                self.width = ((w as f32 * scale).round() as u32).max(1);
                self.height = ((h as f32 * scale).round() as u32).max(1);
                self.source_path = path.display().to_string();
                self.source = Some(img);
                self.status = "Loaded. Adjust width/height/colors below.".to_string();
                self.recompute();
            }
            Err(e) => self.status = format!("couldn't load {}: {e}", path.display()),
        }
    }

    fn recompute(&mut self) {
        let Some(img) = &self.source else { return };
        // Knit stitches are shorter than wide, so keeping a picture's
        // shape takes proportionally more rows than a square grid would.
        let row_factor = self
            .fabric_count_y
            .map_or(1.0, |y| y / self.fabric_count.max(0.01));
        let resized = if self.lock_aspect && (row_factor - 1.0).abs() > 1e-3 {
            let (sw, sh) = (img.width().max(1) as f32, img.height().max(1) as f32);
            let h = ((self.width as f32 * sh / sw * row_factor).round() as u32).max(1);
            resize_exact(img, self.width, h, self.filter)
        } else if self.lock_aspect {
            resize_preserving_aspect(img, Some(self.width), None, self.filter)
        } else {
            resize_exact(img, self.width, self.height, self.filter)
        };
        // `resize_preserving_aspect` may have derived a different height
        // than what's in the field - reflect that back so the displayed
        // number always matches what was actually produced.
        self.height = resized.height();
        self.width = resized.width();
        if let Some(grid_craft) = GridCraft::from_craft(self.craft) {
            let chart = chart_from_image(
                &resized,
                self.colors,
                Fabric {
                    count: self.fabric_count,
                    count_y: self.fabric_count_y,
                    ..Fabric::default()
                },
                self.convert,
                grid_craft,
            );
            self.grid = Some(chart.to_color_grid());
            self.chart = Some(chart);
            self.preview_texture = None;
            return;
        }
        self.chart = None;
        self.grid = Some(if self.filet_mode {
            threshold(
                &resized,
                self.fill_threshold,
                self.invert_threshold,
                self.block_color,
                self.space_color,
            )
        } else {
            quantize(&resized, self.colors)
        });
        self.preview_texture = None; // rebuilt lazily in `show` (needs `ctx`)
    }

    fn filet_colors(&self) -> Option<FiletColors> {
        self.filet_mode.then_some(FiletColors {
            block: self.block_color,
            space: self.space_color,
        })
    }

    fn sts_per_in(&self) -> f32 {
        (self.gauge_sts_per_4in / 4.0).max(0.01)
    }

    fn rows_per_in(&self) -> f32 {
        (self.gauge_rows_per_4in / 4.0).max(0.01)
    }

    /// Converts `desired_width_in`/`desired_height_in` through the gauge
    /// into `width`/`height` (stitch counts), which then feed the exact
    /// same `recompute()` resize/quantize pipeline stitch-count mode uses -
    /// this function is the entire "new way to set size", nothing else in
    /// the resize/quantize path needs to know inches exist at all.
    fn apply_gauge_size(&mut self) {
        self.width = ((self.desired_width_in * self.sts_per_in()).round() as u32).max(1);
        if !self.lock_aspect {
            self.height = ((self.desired_height_in * self.rows_per_in()).round() as u32).max(1);
        }
    }

    /// Reverse of `apply_gauge_size` - populates the inches fields from the
    /// current stitch dimensions. Called when switching into Inches mode so
    /// the fields start from where the stitch-count controls left off,
    /// rather than some stale unrelated value.
    fn sync_inches_from_stitches(&mut self) {
        self.desired_width_in = self.width as f32 / self.sts_per_in();
        self.desired_height_in = self.height as f32 / self.rows_per_in();
    }

    fn ensure_preview_texture(&mut self, ctx: &egui::Context) {
        if self.preview_texture.is_some() {
            return;
        }
        let Some(grid) = &self.grid else { return };
        let mut pixels = Vec::with_capacity(grid.width * grid.height);
        for y in 0..grid.height {
            for x in 0..grid.width {
                let [r, g, b] = grid.get(x, y);
                pixels.push(Color32::from_rgb(r, g, b));
            }
        }
        let color_image = ColorImage {
            size: [grid.width, grid.height],
            pixels,
        };
        let texture = ctx.load_texture("colorwork_preview", color_image, TextureOptions::NEAREST);
        self.preview_texture = Some(texture);
    }

    fn export_svg(&mut self) {
        let Some(grid) = &self.grid else {
            self.status = "load an image first".to_string();
            return;
        };
        let svg = abyssal_thread_export::export_color_chart_svg(grid);
        match std::fs::write(&self.export_svg_path, svg) {
            Ok(()) => self.status = format!("wrote chart to {}", self.export_svg_path),
            Err(e) => self.status = format!("couldn't write {}: {e}", self.export_svg_path),
        }
    }

    fn export_legend(&mut self) {
        let Some(grid) = &self.grid else {
            self.status = "load an image first".to_string();
            return;
        };
        let text = abyssal_thread_export::export_color_grid_legend(grid);
        match std::fs::write(&self.export_legend_path, text) {
            Ok(()) => self.status = format!("wrote legend to {}", self.export_legend_path),
            Err(e) => self.status = format!("couldn't write {}: {e}", self.export_legend_path),
        }
    }
}

/// Bundles everything needed to seed the Grid tab's colorwork editor with
/// a *resizable* pattern - not just the already-quantized `ColorGrid`, but
/// the original source image and the color count/filter used, so resizing
/// there can re-render from source (see `colorwork_grid::ColorworkGridState::from_image_import`)
/// instead of just cropping/padding a frozen snapshot.
pub struct GridImportPayload {
    pub grid: ColorGrid,
    pub source: DynamicImage,
    pub colors: usize,
    pub filter: ResizeFilter,
    /// `Some` when this came from Filet mode - see `FiletColors`.
    pub filet: Option<FiletColors>,
    pub fill_threshold: u8,
    pub invert_threshold: bool,
    /// `Some` for a cross-stitch import - the floss-matched chart and the
    /// settings that produced it (kept for re-rendering from source).
    pub chart: Option<(Chart, ConvertSettings)>,
}

/// Returns `Some(payload)` when the user clicked "Send to Grid editor" this
/// frame - `GoblinApp` uses this to populate the colorwork paint grid (and,
/// via that, the DSL/3D views too) with the current image-import result.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut ImageImportState,
    craft: Craft,
) -> Option<GridImportPayload> {
    let mut send_to_grid = None;
    let grid_craft = GridCraft::from_craft(craft);
    let is_chart = grid_craft.is_some();
    if state.craft != craft {
        state.craft = craft;
        if let Some(g) = grid_craft {
            state.filet_mode = false;
            state.fabric_count = g.default_per_inch();
            state.fabric_count_y = g.default_per_inch_y();
            state.convert = ConvertSettings::for_craft(g);
            state.colors = state.colors.max(8);
            state.gauge_sts_per_4in = state.fabric_count * 4.0;
            state.gauge_rows_per_4in = state.fabric_count_y.unwrap_or(state.fabric_count) * 4.0;
        }
        state.recompute();
    }
    ui.horizontal(|ui| {
        if ui.button("Choose picture...").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Image", &["png", "jpg", "jpeg", "bmp", "gif"])
                .pick_file()
            {
                state.load(path);
            }
        }
        if !state.source_path.is_empty() {
            ui.label(&state.source_path);
        }
    });

    if state.source.is_none() {
        ui.label(if is_chart {
            "Choose a picture to convert it into a chart."
        } else {
            "Choose a picture to convert it into a colorwork chart."
        });
        return None;
    }

    ui.separator();
    ui.heading("Size");
    let mut changed = false;

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

    if is_chart {
        ui.horizontal(|ui| {
            if size_ui(
                ui,
                "image_import_fabric_count",
                grid_craft.unwrap_or(GridCraft::CrossStitch),
                &mut state.fabric_count,
                &mut state.fabric_count_y,
            ) {
                state.gauge_sts_per_4in = state.fabric_count * 4.0;
                state.gauge_rows_per_4in = state.fabric_count_y.unwrap_or(state.fabric_count) * 4.0;
                if state.size_mode == SizeMode::Inches {
                    state.apply_gauge_size();
                }
                changed = true;
            }
        });
    } else {
        ui.horizontal(|ui| {
            ui.label("Gauge:");
            changed |= ui
                .add(egui::DragValue::new(&mut state.gauge_sts_per_4in).range(1.0..=200.0).speed(0.1))
                .changed();
            ui.label("sts,");
            changed |= ui
                .add(egui::DragValue::new(&mut state.gauge_rows_per_4in).range(1.0..=200.0).speed(0.1))
                .changed();
            ui.label("rows, per 4 inches");
        })
        .response
        .on_hover_text("From your yarn label or a swatch you crocheted and measured - not a substitute for either.");
    }

    match state.size_mode {
        SizeMode::Stitches => {
            ui.horizontal(|ui| {
                ui.label("Width (stitches):");
                changed |= ui
                    .add(egui::Slider::new(&mut state.width, 1..=400))
                    .changed();
                changed |= ui.add(egui::DragValue::new(&mut state.width)).changed();
            });
            ui.horizontal(|ui| {
                ui.label("Height (rows):");
                let enabled = !state.lock_aspect;
                changed |= ui
                    .add_enabled(enabled, egui::Slider::new(&mut state.height, 1..=400))
                    .changed();
                changed |= ui
                    .add_enabled(enabled, egui::DragValue::new(&mut state.height))
                    .changed();
            });
        }
        SizeMode::Inches => {
            ui.horizontal(|ui| {
                ui.label("Width (inches):");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut state.desired_width_in)
                            .range(0.5..=200.0)
                            .speed(0.1),
                    )
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Height (inches):");
                let enabled = !state.lock_aspect;
                changed |= ui
                    .add_enabled(
                        enabled,
                        egui::DragValue::new(&mut state.desired_height_in)
                            .range(0.5..=200.0)
                            .speed(0.1),
                    )
                    .changed();
            });
            if changed {
                state.apply_gauge_size();
            }
        }
    }

    changed |= ui
        .checkbox(&mut state.lock_aspect, "Lock aspect ratio to source image")
        .changed();

    if let Some(grid) = &state.grid {
        let est_w_in = grid.width as f32 / state.sts_per_in();
        let est_h_in = grid.height as f32 / state.rows_per_in();
        let unit = if state.filet_mode {
            "Block count"
        } else {
            "Stitch count"
        };
        ui.label(format!(
            "{unit}: {} ({} x {}) \u{2192} approx finished size at this gauge: {:.1} in x {:.1} in",
            grid.width * grid.height,
            grid.width,
            grid.height,
            est_w_in,
            est_h_in,
        ));
        if state.filet_mode {
            ui.label(format!(
                "Foundation chain: {}",
                abyssal_thread_export::filet_starting_chain(grid.width)
            ));
        }
    }

    ui.separator();
    if !is_chart {
        changed |= ui
            .checkbox(&mut state.filet_mode, "Filet crochet mode")
            .on_hover_text("Reduces the image to two colors (block/space) by brightness threshold instead of a full palette, and enables filet-specific chain count and written block/space instructions below.")
            .changed();
    }

    if state.filet_mode {
        ui.heading("Fill");
        ui.horizontal(|ui| {
            ui.label("Fill threshold:");
            changed |= ui
                .add(egui::Slider::new(&mut state.fill_threshold, 0..=255))
                .on_hover_text("Adjust until the motif reads clearly - pixels darker than this become blocks (or lighter, with Invert on).")
                .changed();
        });
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut state.invert_threshold, "Invert").changed();
            ui.label("Block color:");
            changed |=
                egui::color_picker::color_edit_button_srgb(ui, &mut state.block_color).changed();
            ui.label("Space color:");
            changed |=
                egui::color_picker::color_edit_button_srgb(ui, &mut state.space_color).changed();
        });
    } else {
        ui.heading("Colors");
        ui.horizontal(|ui| {
            ui.label(if is_chart {
                "Max colors:"
            } else {
                "Number of colors:"
            });
            let max = grid_craft.map_or(16, |g| g.max_colors());
            changed |= ui
                .add(egui::Slider::new(&mut state.colors, 1..=max))
                .changed();
        });
        if is_chart {
            changed |= convert_settings_ui(
                ui,
                "image_import_convert",
                grid_craft.unwrap_or(GridCraft::CrossStitch),
                &mut state.convert,
            );
            ui.small("Colors are matched to the nearest color in the chosen catalog, so similar colors may merge.");
        }
    }
    ui.horizontal(|ui| {
        ui.label("Resize style:");
        changed |= ui
            .radio_value(
                &mut state.filter,
                ResizeFilter::Nearest,
                "Crisp (logos/text)",
            )
            .changed();
        changed |= ui
            .radio_value(&mut state.filter, ResizeFilter::Smooth, "Smooth (photos)")
            .changed();
    });
    if let Some(chart) = &state.chart {
        ui.horizontal_wrapped(|ui| {
            ui.label("Floss:");
            for f in &chart.palette {
                let (rect, resp) =
                    ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                let [r, g, b] = f.rgb;
                ui.painter()
                    .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
                resp.on_hover_text(f.label());
            }
        });
    } else if let Some(grid) = &state.grid {
        ui.horizontal_wrapped(|ui| {
            ui.label("Palette:");
            for [r, g, b] in grid.palette() {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
            }
        });
    }

    if changed {
        state.recompute();
    }

    ui.separator();
    ui.heading("Preview");
    state.ensure_preview_texture(ui.ctx());
    if let Some(texture) = &state.preview_texture {
        // Scale the (small) pixel grid up for visibility, capped so huge
        // grids don't blow out the panel.
        let src_size = texture.size_vec2();
        let scale = (500.0 / src_size.x.max(src_size.y)).clamp(1.0, 16.0);
        // NOTE: this is the one call in this patch I couldn't compile-check
        // against your exact egui 0.27 (sandbox toolchain here is too old to
        // resolve eframe's dependency tree - see the accompanying message).
        // `ui.image((TextureId, Vec2))` has been a stable convenience
        // conversion for a long time; if it doesn't compile for you, the
        // fix is almost certainly swapping this one line for whatever
        // `egui::Image::new(...)`-builder form egui 0.27 wants.
        ui.image((texture.id(), src_size * scale));
    }

    ui.separator();
    ui.horizontal(|ui| {
        if !is_chart && ui.button("Export SVG chart...").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("SVG", &["svg"])
                .set_file_name(&state.export_svg_path)
                .save_file()
            {
                state.export_svg_path = path.display().to_string();
                state.export_svg();
            }
        }
        if !is_chart && ui.button("Export text legend...").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Text", &["txt"])
                .set_file_name(&state.export_legend_path)
                .save_file()
            {
                state.export_legend_path = path.display().to_string();
                state.export_legend();
            }
        }
        ui.separator();
        let (label, hover) = if is_chart {
            ("Send to Chart editor \u{2192}", "Open this in the chart editor for touch-ups, color changes, sizing and printing. Keeps the source photo so resizing there re-renders instead of cropping.")
        } else {
            ("Send to Grid editor \u{2192}", "Load this into the paint-grid editor for fine manual touch-ups (also updates the DSL and 3D tabs). Keeps the source photo so resizing there rescales properly instead of cropping.")
        };
        if ui
            .button(label)
            .on_hover_text(hover)
            .clicked()
        {
            if let (Some(grid), Some(source)) = (&state.grid, &state.source) {
                send_to_grid = Some(GridImportPayload {
                    grid: grid.clone(),
                    source: source.clone(),
                    colors: state.colors,
                    filter: state.filter,
                    filet: state.filet_colors(),
                    fill_threshold: state.fill_threshold,
                    invert_threshold: state.invert_threshold,
                    chart: state.chart.clone().map(|c| (c, state.convert)),
                });
            }
        }
    });
    if !state.status.is_empty() {
        ui.label(&state.status);
    }

    send_to_grid
}
