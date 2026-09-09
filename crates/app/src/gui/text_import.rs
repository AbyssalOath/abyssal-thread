//! Type words, pick a font file, and convert them into a colorwork chart.
//!
//! Shares almost all of its machinery with `image_import.rs`: the rendered
//! text becomes a `DynamicImage`, which flows through the exact same
//! resize/quantize/gauge-sizing pipeline a loaded photo does. "Send to
//! Grid editor" produces the same `GridImportPayload` image_import.rs
//! does, so `colorwork_grid::resize_canvas`'s "re-render from source at
//! the new size" logic works identically whether the source was a photo
//! or rendered text - no changes needed there at all.
//!
//! Font choice is "pick any .ttf/.otf file from your system" via the file
//! dialog, the same pattern already used for "Choose picture...", rather
//! than a small bundled/hardcoded font list - this avoids needing to
//! source and redistribute font files ourselves, and gives access to
//! every font already installed on your machine.
//!
//! Each line of text is measured and centered independently (not the
//! whole block as one unit) - so "MERCI POUR LE" / "VENIN" centers each
//! line on its own, matching how the reference image was laid out.

use crate::gui::fonts::FONT_FAMILIES;
use crate::gui::image_import::GridImportPayload;
use ab_glyph::{FontArc, PxScale};
use abyssal_thread_core::ColorGrid;
use abyssal_thread_imageimport::{quantize, resize_exact, resize_preserving_aspect, ResizeFilter};
use eframe::egui::{self, Color32, ColorImage, TextureHandle, TextureOptions};
use image::{DynamicImage, Rgba, RgbaImage};
use imageproc::drawing::{draw_text_mut, text_size};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SizeMode {
    Stitches,
    Inches,
}

pub struct TextImportState {
    text: String,
    selected_family: usize,
    custom_font_path: Option<String>,
    system_font_names: Vec<String>,
    selected_system_font: Option<String>,
    bold: bool,
    italic: bool,
    font: Option<FontArc>,
    /// Resolution (in px) the text is rendered at before being resized
    /// down to the target stitch grid - this is source detail, not final
    /// stitch count, so it rarely needs adjusting once set reasonably high.
    render_font_size: f32,
    fg_color: [u8; 3],
    bg_color: [u8; 3],
    rendered: Option<DynamicImage>,
    width: u32,
    height: u32,
    lock_aspect: bool,
    colors: usize,
    filter: ResizeFilter,
    size_mode: SizeMode,
    gauge_sts_per_4in: f32,
    gauge_rows_per_4in: f32,
    desired_width_in: f32,
    desired_height_in: f32,
    grid: Option<ColorGrid>,
    preview_texture: Option<TextureHandle>,
    status: String,
}

impl Default for TextImportState {
    fn default() -> Self {
        let mut state = Self {
            text: String::new(),
            selected_family: 0,
            custom_font_path: None,
            system_font_names: crate::gui::fonts::enumerate_system_font_families(),
            selected_system_font: None,
            bold: false,
            italic: false,
            font: None,
            render_font_size: 200.0,
            fg_color: [200, 40, 40],
            bg_color: [10, 10, 10],
            rendered: None,
            width: 60,
            height: 60,
            lock_aspect: true,
            colors: 2,
            filter: ResizeFilter::Nearest,
            size_mode: SizeMode::Stitches,
            gauge_sts_per_4in: 16.0,
            gauge_rows_per_4in: 16.0,
            desired_width_in: 15.0,
            desired_height_in: 15.0,
            grid: None,
            preview_texture: None,
            status: String::new(),
        };
        state.reload_font();
        state.render_and_recompute();
        state
    }
}

/// Renders `text` (split on `\n` into lines) onto a canvas sized to fit,
/// each line independently horizontally centered.
fn render_text_image(
    text: &str,
    font: &FontArc,
    font_size: f32,
    fg: [u8; 3],
    bg: [u8; 3],
) -> DynamicImage {
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    if lines.is_empty() {
        return DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            1,
            1,
            Rgba([bg[0], bg[1], bg[2], 255]),
        ));
    }

    let scale = PxScale::from(font_size);
    let line_height = (font_size * 1.3).round() as i32;

    let mut line_widths: Vec<i32> = Vec::with_capacity(lines.len());
    let mut max_width = 0i32;
    for line in &lines {
        // ab_glyph-backed text_size returns (u32, u32) - the old
        // rusttype-backed one returned (i32, i32) - cast immediately so
        // the signed centering math below (which needs negative
        // intermediates before `.max(0)` clamps them) is unaffected.
        let (w, _h) = text_size(scale, font, line);
        let w = w as i32;
        line_widths.push(w);
        max_width = max_width.max(w);
    }
    let pad = (font_size * 0.2).round() as i32;
    let canvas_w = (max_width + pad * 2).max(1) as u32;
    let canvas_h = (line_height * lines.len() as i32 + pad * 2).max(1) as u32;

    let bg_rgba = Rgba([bg[0], bg[1], bg[2], 255]);
    let fg_rgba = Rgba([fg[0], fg[1], fg[2], 255]);
    let mut img = RgbaImage::from_pixel(canvas_w, canvas_h, bg_rgba);
    for (i, line) in lines.iter().enumerate() {
        let line_w = line_widths[i];
        let x = ((canvas_w as i32 - line_w) / 2).max(0);
        let y = pad + i as i32 * line_height;
        draw_text_mut(&mut img, fg_rgba, x, y, scale, font, line);
    }
    DynamicImage::ImageRgba8(img)
}

impl TextImportState {
    fn reload_font(&mut self) {
        if let Some(path) = self.custom_font_path.clone() {
            match std::fs::read(&path)
                .ok()
                .and_then(|bytes| FontArc::try_from_vec(bytes).ok())
            {
                Some(font) => {
                    self.font = Some(font);
                    self.status.clear();
                    return;
                }
                None => self.status = format!("couldn't load font file: {path}"),
            }
        }
        let family = &FONT_FAMILIES[self.selected_family];
        let bytes = family.bytes_for(self.bold, self.italic);
        match FontArc::try_from_slice(bytes) {
            Ok(font) => {
                self.font = Some(font);
                self.status.clear();
            }
            Err(_) => self.status = format!("couldn't parse bundled font '{}'", family.name),
        }
    }

    fn render_and_recompute(&mut self) {
        let Some(font) = &self.font else { return };
        self.rendered = Some(render_text_image(
            &self.text,
            font,
            self.render_font_size,
            self.fg_color,
            self.bg_color,
        ));
        self.recompute();
    }

    fn recompute(&mut self) {
        let Some(source) = &self.rendered else { return };
        let resized = if self.lock_aspect {
            resize_preserving_aspect(source, Some(self.width), None, self.filter)
        } else {
            resize_exact(source, self.width, self.height, self.filter)
        };
        self.height = resized.height();
        self.width = resized.width();
        self.grid = Some(quantize(&resized, self.colors));
        self.preview_texture = None;
    }

    fn sts_per_in(&self) -> f32 {
        (self.gauge_sts_per_4in / 4.0).max(0.01)
    }

    fn rows_per_in(&self) -> f32 {
        (self.gauge_rows_per_4in / 4.0).max(0.01)
    }

    fn apply_gauge_size(&mut self) {
        self.width = ((self.desired_width_in * self.sts_per_in()).round() as u32).max(1);
        if !self.lock_aspect {
            self.height = ((self.desired_height_in * self.rows_per_in()).round() as u32).max(1);
        }
    }

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
        self.preview_texture =
            Some(ctx.load_texture("text_import_preview", color_image, TextureOptions::NEAREST));
    }
}

/// Returns `Some(payload)` when "Send to Grid editor" was clicked this frame.
pub fn show(ui: &mut egui::Ui, state: &mut TextImportState) -> Option<GridImportPayload> {
    let mut send_to_grid = None;
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label("Font:");
        let current_name = FONT_FAMILIES[state.selected_family].name;
        egui::ComboBox::from_id_source("text_import_font_family")
            .selected_text(current_name)
            .show_ui(ui, |ui| {
                for (i, family) in FONT_FAMILIES.iter().enumerate() {
                    if ui
                        .selectable_value(&mut state.selected_family, i, family.name)
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
        changed |= ui.checkbox(&mut state.bold, "Bold").changed();
        changed |= ui.checkbox(&mut state.italic, "Italic").changed();
    });

    ui.horizontal(|ui| {
        if ui.button("Use a custom font file...").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Font", &["ttf", "otf"])
                .pick_file()
            {
                state.custom_font_path = Some(path.display().to_string());
                changed = true;
            }
        }
        if let Some(path) = &state.custom_font_path {
            ui.label(path);
            if ui.button("Clear (use dropdown font)").clicked() {
                state.custom_font_path = None;
                changed = true;
            }
        }
    });

    ui.horizontal(|ui| {
        ui.label("Or an installed system font:");
        let current = state.selected_system_font.clone().unwrap_or_else(|| "(none)".to_string());
        egui::ComboBox::from_id_source("text_import_system_font")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for name in state.system_font_names.clone() {
                    let is_selected = state.selected_system_font.as_deref() == Some(name.as_str());
                    if ui.selectable_label(is_selected, &name).clicked() {
                        state.selected_system_font = Some(name.clone());
                        match crate::gui::fonts::resolve_system_font_path(&name) {
                            Some(path) => {
                                state.custom_font_path = Some(path);
                                changed = true;
                            }
                            None => {
                                state.status = format!("'{name}' has no loadable font file (likely a memory-only system font)");
                            }
                        }
                    }
                }
            });
    });

    if changed {
        state.reload_font();
    }

    if state.font.is_none() {
        ui.label(&state.status);
        return None;
    }

    ui.separator();
    ui.label("Text (one line per row of the pattern - each line is centered independently):");
    changed |= ui
        .add(egui::TextEdit::multiline(&mut state.text).desired_rows(3))
        .changed();

    ui.horizontal(|ui| {
        ui.label("Text color:");
        changed |= egui::color_picker::color_edit_button_srgb(ui, &mut state.fg_color).changed();
        ui.label("Background color:");
        changed |= egui::color_picker::color_edit_button_srgb(ui, &mut state.bg_color).changed();
    });
    ui.horizontal(|ui| {
        ui.label("Render detail (px):");
        changed |= ui
            .add(egui::DragValue::new(&mut state.render_font_size).clamp_range(20.0..=800.0).speed(2.0))
            .on_hover_text("Higher = more detail available when resizing to the stitch grid below. Doesn't change the final stitch count.")
            .changed();
    });

    if changed {
        state.render_and_recompute();
    }

    ui.separator();
    ui.heading("Size");
    let mut size_changed = false;

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
        size_changed |= ui
            .add(
                egui::DragValue::new(&mut state.gauge_sts_per_4in)
                    .clamp_range(1.0..=200.0)
                    .speed(0.1),
            )
            .changed();
        ui.label("sts,");
        size_changed |= ui
            .add(
                egui::DragValue::new(&mut state.gauge_rows_per_4in)
                    .clamp_range(1.0..=200.0)
                    .speed(0.1),
            )
            .changed();
        ui.label("rows, per 4 inches");
    });

    match state.size_mode {
        SizeMode::Stitches => {
            ui.horizontal(|ui| {
                ui.label("Width (stitches):");
                size_changed |= ui
                    .add(egui::Slider::new(&mut state.width, 1..=400))
                    .changed();
                size_changed |= ui.add(egui::DragValue::new(&mut state.width)).changed();
            });
            ui.horizontal(|ui| {
                ui.label("Height (rows):");
                let enabled = !state.lock_aspect;
                size_changed |= ui
                    .add_enabled(enabled, egui::Slider::new(&mut state.height, 1..=400))
                    .changed();
                size_changed |= ui
                    .add_enabled(enabled, egui::DragValue::new(&mut state.height))
                    .changed();
            });
        }
        SizeMode::Inches => {
            ui.horizontal(|ui| {
                ui.label("Width (inches):");
                size_changed |= ui
                    .add(
                        egui::DragValue::new(&mut state.desired_width_in)
                            .clamp_range(0.5..=200.0)
                            .speed(0.1),
                    )
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Height (inches):");
                let enabled = !state.lock_aspect;
                size_changed |= ui
                    .add_enabled(
                        enabled,
                        egui::DragValue::new(&mut state.desired_height_in)
                            .clamp_range(0.5..=200.0)
                            .speed(0.1),
                    )
                    .changed();
            });
            if size_changed {
                state.apply_gauge_size();
            }
        }
    }
    size_changed |= ui
        .checkbox(&mut state.lock_aspect, "Lock aspect ratio")
        .changed();

    ui.horizontal(|ui| {
        ui.label("Number of colors:");
        size_changed |= ui
            .add(egui::Slider::new(&mut state.colors, 1..=16))
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label("Resize style:");
        size_changed |= ui
            .radio_value(&mut state.filter, ResizeFilter::Nearest, "Crisp")
            .changed();
        size_changed |= ui
            .radio_value(&mut state.filter, ResizeFilter::Smooth, "Smooth")
            .changed();
    });

    if size_changed {
        state.recompute();
    }

    if let Some(grid) = &state.grid {
        let est_w_in = grid.width as f32 / state.sts_per_in();
        let est_h_in = grid.height as f32 / state.rows_per_in();
        ui.label(format!(
            "Stitch count: {} ({} x {}) \u{2192} approx finished size at this gauge: {:.1} in x {:.1} in",
            grid.width * grid.height,
            grid.width,
            grid.height,
            est_w_in,
            est_h_in,
        ));
    }

    ui.separator();
    ui.heading("Preview");
    state.ensure_preview_texture(ui.ctx());
    if let Some(texture) = &state.preview_texture {
        let src_size = texture.size_vec2();
        let scale = (500.0 / src_size.x.max(src_size.y)).clamp(1.0, 16.0);
        ui.image((texture.id(), src_size * scale));
    }

    ui.separator();
    if ui
        .button("Send to Grid editor \u{2192}")
        .on_hover_text("Load this into the paint-grid editor for fine manual touch-ups (also updates the DSL and 3D tabs).")
        .clicked()
    {
        if let (Some(grid), Some(rendered)) = (&state.grid, &state.rendered) {
            send_to_grid = Some(GridImportPayload {
                grid: grid.clone(),
                source: rendered.clone(),
                colors: state.colors,
                filter: state.filter,
            });
        }
    }
    if !state.status.is_empty() {
        ui.label(&state.status);
    }

    send_to_grid
}
