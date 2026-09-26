//! Cross-stitch chart editor (the Grid tab when a cross-stitch pattern is
//! loaded) and the Materials tab.
//!
//! Rendering follows `colorwork_grid.rs`'s performance rule - full crosses
//! are one GPU texture plus one interactive region, never a widget per
//! square. Everything else is painted on top each frame, but only for the
//! visible part of the scroll area: symbols (once squares are big enough
//! to read), part stitches, then backstitch lines and knots, so a 200x200
//! chart still costs a few hundred shapes per frame, not 40,000.

use abyssal_thread_core::ColorGrid;
use abyssal_thread_crossstitch::chart::{COMMON_HOOPS_IN, HOOP_MARGIN_IN};
use abyssal_thread_crossstitch::export::{
    contrasting_ink, half_stitch_ends, quarter_origin, split_triangle, usage_summary,
};
use abyssal_thread_crossstitch::threads::find_thread;
use abyssal_thread_crossstitch::{
    Backstitch, BlendThread, Catalog, Chart, Corner, Diagonal, Fabric, Floss, GridCraft, Knot,
    Partial,
};
use abyssal_thread_imageimport::{quantize, resize_exact, ResizeFilter};
use eframe::egui::{self, Color32, ColorImage, TextureHandle, TextureOptions};
use image::DynamicImage;

/// Squares smaller than this (in screen px) don't get symbols drawn -
/// they'd be unreadable smudges.
const MIN_SYMBOL_CELL_PX: f32 = 11.0;
/// How close (in squares) the pointer must be to a backstitch line or
/// knot for the eraser / right-click to remove it.
const PICK_RADIUS: f32 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Full,
    Half(Diagonal),
    ThreeQuarter,
    Quarter,
    Backstitch,
    Knot,
    Fill,
    Erase,
}

fn c32(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// How a picture becomes a chart (besides size and color count) - shared
/// by the Image/Text tabs and this editor's re-render-from-source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConvertSettings {
    pub skip_background: bool,
    /// `None` = keep free colors (crafts without a catalog).
    pub catalog: Option<Catalog>,
    /// Patches smaller than this many squares are merged into their
    /// surroundings (`Chart::remove_confetti`); 1 = off.
    pub min_speck: usize,
}

impl ConvertSettings {
    pub fn for_craft(craft: GridCraft) -> Self {
        Self {
            skip_background: craft.default_skip_background(),
            catalog: craft.default_catalog(),
            // Removes lone single cells only - the worst offenders in
            // photo imports - without eating deliberate 2-cell details.
            // Pixel art is usually hand-placed, so leave it alone.
            min_speck: if craft == GridCraft::PixelArt { 1 } else { 2 },
        }
    }
}

impl Default for ConvertSettings {
    fn default() -> Self {
        Self::for_craft(GridCraft::CrossStitch)
    }
}

/// Quantize `img` to at most `colors` colors, snap to the chosen color
/// catalog, then clear out confetti. The result is a `craft` chart at
/// `fabric`'s cell size, with the craft's default board.
pub fn chart_from_image(
    img: &image::RgbImage,
    colors: usize,
    fabric: Fabric,
    settings: ConvertSettings,
    craft: GridCraft,
) -> Chart {
    let grid = quantize(img, colors.clamp(1, craft.max_colors()));
    let mut chart =
        Chart::from_color_grid(&grid, fabric, settings.skip_background, settings.catalog);
    chart.craft = craft;
    chart.board = craft.default_board(chart.fabric.count);
    if chart.remove_confetti(settings.min_speck) > 0 {
        chart.remove_unused_floss();
    }
    chart
}

/// Brand, background and speck controls shared with the import tabs.
/// Returns whether anything changed.
pub fn convert_settings_ui(
    ui: &mut egui::Ui,
    id: &str,
    craft: GridCraft,
    s: &mut ConvertSettings,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= catalog_ui(ui, id, craft, &mut s.catalog);
    });
    changed |= ui
        .checkbox(
            &mut s.skip_background,
            format!("Leave background {}", craft.background_word()),
        )
        .on_hover_text("Treats the most common color around the picture's edge as background and leaves those cells empty.")
        .changed();
    let (_, cells) = craft.cell_word();
    ui.horizontal(|ui| {
        ui.label("Remove specks smaller than");
        changed |= ui
            .add(egui::DragValue::new(&mut s.min_speck).range(1..=20))
            .on_hover_text(format!("Lone {cells} of an in-between color (\"confetti\") are merged into whatever surrounds them. 1 = keep everything; 2 = remove single {cells}."))
            .changed();
        ui.label(cells);
    });
    changed
}

/// Color-catalog picker for `craft`; crafts without catalogs just say
/// colors are free.
fn catalog_ui(
    ui: &mut egui::Ui,
    id: &str,
    craft: GridCraft,
    catalog: &mut Option<Catalog>,
) -> bool {
    let choices = craft.catalogs();
    if choices.is_empty() {
        *catalog = None;
        ui.label("Colors: any (named by nearest common color)");
        return false;
    }
    if catalog.is_none_or(|c| !choices.contains(&c)) {
        *catalog = craft.default_catalog();
    }
    let mut changed = false;
    ui.label(if craft == GridCraft::CrossStitch {
        "Floss brand:"
    } else {
        "Colors from:"
    });
    egui::ComboBox::from_id_salt(id)
        .selected_text(catalog.map_or("", |c| c.display_name()))
        .show_ui(ui, |ui| {
            for &c in choices {
                changed |= ui
                    .selectable_value(catalog, Some(c), c.display_name())
                    .changed();
            }
        });
    changed
}

/// Cell-size dropdown (Aida count, bead size, knitting gauge, quilt
/// square...) for `craft`, plus custom values in the craft's own terms:
/// stitches and rows per 4 in for knitting, finished square inches for
/// quilts, cells per inch otherwise. `per_inch_y` is the row gauge
/// (`None` = square cells). Shared with the import tabs. Returns whether
/// anything changed.
pub fn size_ui(
    ui: &mut egui::Ui,
    id: &str,
    craft: GridCraft,
    per_inch: &mut f32,
    per_inch_y: &mut Option<f32>,
) -> bool {
    let mut changed = false;
    ui.label(match craft {
        GridCraft::CrossStitch => "Fabric:",
        GridCraft::Knitting => "Gauge:",
        _ => "Size:",
    });
    let current = craft
        .size_label_xy(*per_inch, *per_inch_y)
        .map_or_else(|| "Custom".to_string(), str::to_string);
    egui::ComboBox::from_id_salt(id)
        .selected_text(current)
        .show_ui(ui, |ui| {
            for preset in craft.sizes() {
                let selected = craft.size_label_xy(*per_inch, *per_inch_y) == Some(preset.label);
                if ui.selectable_label(selected, preset.label).clicked() {
                    *per_inch = preset.per_inch;
                    *per_inch_y = preset.per_inch_y;
                    changed = true;
                }
            }
        });
    match craft {
        GridCraft::Knitting => {
            let mut sts = *per_inch * 4.0;
            let mut rows = per_inch_y.unwrap_or(*per_inch) * 4.0;
            let a = ui.add(
                egui::DragValue::new(&mut sts)
                    .range(4.0..=60.0)
                    .speed(0.1)
                    .suffix(" sts"),
            );
            let b = ui.add(
                egui::DragValue::new(&mut rows)
                    .range(4.0..=80.0)
                    .speed(0.1)
                    .suffix(" rows"),
            );
            ui.label("/ 4 in");
            if a.changed() || b.changed() {
                *per_inch = sts / 4.0;
                *per_inch_y = Some(rows / 4.0);
                changed = true;
            }
        }
        GridCraft::Quilt => {
            let mut inches = 1.0 / per_inch.max(0.01);
            if ui
                .add(
                    egui::DragValue::new(&mut inches)
                        .range(0.5..=12.0)
                        .speed(0.125)
                        .suffix(" in finished"),
                )
                .changed()
            {
                *per_inch = 1.0 / inches.max(0.125);
                changed = true;
            }
        }
        _ => {
            let (_, cells) = craft.cell_word();
            changed |= ui
                .add(
                    egui::DragValue::new(per_inch)
                        .range(0.5..=60.0)
                        .speed(0.05)
                        .suffix(format!(" {cells}/in")),
                )
                .changed();
        }
    }
    changed
}

/// Floss palette swatch with its symbol on it (a blend shows both
/// threads, split diagonally).
fn swatch(ui: &mut egui::Ui, f: &Floss, size: f32, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, c32(f.rgb));
    if let Some(b) = &f.blend {
        painter.add(egui::Shape::convex_polygon(
            vec![rect.right_top(), rect.right_bottom(), rect.left_bottom()],
            c32(b.rgb),
            egui::Stroke::NONE,
        ));
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        f.symbol,
        egui::FontId::monospace(size * 0.7),
        c32(contrasting_ink(f.display_rgb())),
    );
    let stroke = if selected {
        egui::Stroke::new(2.5_f32, Color32::from_rgb(255, 200, 0))
    } else {
        egui::Stroke::new(1.0_f32, Color32::from_gray(90))
    };
    painter.rect_stroke(rect, 2.0, stroke);
    response
}

/// The catalog a chart's colors come from: the first palette entry's
/// brand when it's one of the craft's catalogs, else the craft default.
fn palette_catalog(chart: &Chart) -> Option<Catalog> {
    let choices = chart.craft.catalogs();
    chart
        .palette
        .iter()
        .find_map(|f| Catalog::for_brand(&f.brand))
        .filter(|c| choices.contains(c))
        .or(chart.craft.default_catalog())
}

/// Whether every palette color fits `craft` (from one of its catalogs,
/// or free colors for a craft without catalogs).
fn palette_fits_craft(chart: &Chart, craft: GridCraft) -> bool {
    let choices = craft.catalogs();
    chart.palette.iter().all(|f| {
        if choices.is_empty() {
            f.is_custom()
        } else {
            Catalog::for_brand(&f.brand).is_some_and(|c| choices.contains(&c))
        }
    })
}

/// Renders the chart's full cells to an image, `scale` pixels per cell,
/// optionally with 1px grid lines. Empty cells are the background color,
/// or transparent for pixel art.
pub fn render_image(chart: &Chart, scale: u32, grid: bool) -> image::RgbaImage {
    let scale = scale.max(1);
    // Knit stitches are shorter than wide; square for everything else.
    let scale_y = ((scale as f32 * chart.fabric.cell_aspect()).round() as u32).max(1);
    let (w, h) = (chart.width as u32 * scale, chart.height as u32 * scale_y);
    let transparent = chart.craft == GridCraft::PixelArt;
    let mut img = image::RgbaImage::new(w, h);
    for (i, cell) in chart.cells.iter().enumerate() {
        let (cx, cy) = ((i % chart.width) as u32, (i / chart.width) as u32);
        let px = match cell.and_then(|c| chart.palette.get(c as usize)) {
            Some(f) => {
                let [r, g, b] = f.display_rgb();
                image::Rgba([r, g, b, 255])
            }
            None if transparent => image::Rgba([0, 0, 0, 0]),
            None => {
                let [r, g, b] = chart.fabric.rgb;
                image::Rgba([r, g, b, 255])
            }
        };
        for dy in 0..scale_y {
            for dx in 0..scale {
                let on_line = grid && scale > 2 && (dx == 0 || dy == 0);
                let p = if on_line {
                    image::Rgba([0, 0, 0, 90])
                } else {
                    px
                };
                img.put_pixel(cx * scale + dx, cy * scale_y + dy, p);
            }
        }
    }
    img
}

/// Distance (in squares) from point `p` to segment `a`-`b`.
fn dist_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

fn half_to_squares((x, y): (u32, u32)) -> (f32, f32) {
    (x as f32 / 2.0, y as f32 / 2.0)
}

pub struct CrossStitchState {
    pub chart: Chart,
    pub selected: usize,
    tool: Tool,
    cell_px: f32,
    show_symbols: bool,
    texture: Option<TextureHandle>,
    texture_dirty: bool,
    hoop_in: f32,
    hover: String,
    /// Grid point where an in-progress backstitch drag started.
    bs_start: Option<(u32, u32)>,
    /// Kept (with the settings that produced the chart) when the chart
    /// came from Image/Text import, so resizing re-renders from the
    /// original instead of cropping - same reasoning as
    /// `ColorworkGridState::source_image`.
    source_image: Option<DynamicImage>,
    colors: usize,
    filter: ResizeFilter,
    convert: ConvertSettings,
    /// Catalog for "Add floss" / blends / re-matching; `None` = free
    /// colors.
    catalog: Option<Catalog>,
    /// Color for "Add color" when the craft has no catalog.
    free_color: [u8; 3],
    craft_note: String,
    png_scale: u32,
    png_grid: bool,
    /// Knitting: floats longer than this are flagged.
    max_float: usize,
    png_result: String,
    cleanup_min: usize,
    cleanup_result: String,
    floss_query: String,
    suggest_color: [u8; 3],
    blend_code: String,
}

impl CrossStitchState {
    pub fn new(chart: Chart) -> Self {
        let convert = ConvertSettings::for_craft(chart.craft);
        let catalog = palette_catalog(&chart);
        Self {
            chart,
            selected: 0,
            tool: Tool::Full,
            cell_px: 16.0,
            show_symbols: true,
            texture: None,
            texture_dirty: true,
            hoop_in: 8.0,
            hover: String::new(),
            bs_start: None,
            source_image: None,
            colors: 8,
            filter: ResizeFilter::Nearest,
            convert,
            catalog,
            free_color: [200, 60, 60],
            craft_note: String::new(),
            png_scale: 10,
            png_grid: false,
            max_float: abyssal_thread_crossstitch::knit::DEFAULT_MAX_FLOAT,
            png_result: String::new(),
            cleanup_min: 2,
            cleanup_result: String::new(),
            floss_query: String::new(),
            suggest_color: [120, 170, 90],
            blend_code: String::new(),
        }
    }

    pub fn from_import(
        chart: Chart,
        source: DynamicImage,
        colors: usize,
        filter: ResizeFilter,
        convert: ConvertSettings,
    ) -> Self {
        let mut s = Self::new(chart);
        s.source_image = Some(source);
        s.colors = colors;
        s.filter = filter;
        s.convert = convert;
        s.catalog = convert.catalog;
        s
    }

    /// Swap in a freshly parsed chart (after undo/redo or a text edit)
    /// while keeping editor state like the selected floss and zoom.
    pub fn replace_chart(&mut self, chart: Chart) {
        if chart.craft != self.chart.craft {
            self.catalog = palette_catalog(&chart);
            self.convert = ConvertSettings::for_craft(chart.craft);
            if !chart.craft.has_part_stitches()
                && !(chart.craft.has_triangles() && self.tool == Tool::ThreeQuarter)
            {
                self.tool = Tool::Full;
            }
        }
        self.chart = chart;
        self.selected = self
            .selected
            .min(self.chart.palette.len().saturating_sub(1));
        self.texture_dirty = true;
    }

    fn rebuild_texture_if_needed(&mut self, ctx: &egui::Context) {
        if self.texture.is_some() && !self.texture_dirty {
            return;
        }
        let grid: ColorGrid = self.chart.to_color_grid();
        let image = ColorImage {
            size: [grid.width, grid.height],
            pixels: grid.cells.iter().map(|&c| c32(c)).collect(),
        };
        self.texture = Some(ctx.load_texture("xstitch_chart", image, TextureOptions::NEAREST));
        self.texture_dirty = false;
    }

    fn rerender(&mut self, width: usize, height: usize) {
        let Some(src) = &self.source_image else {
            return;
        };
        let resized = resize_exact(src, width.max(1) as u32, height.max(1) as u32, self.filter);
        let name = self.chart.name.clone();
        let board = self.chart.board;
        let mut chart = chart_from_image(
            &resized,
            self.colors,
            self.chart.fabric.clone(),
            self.convert,
            self.chart.craft,
        );
        chart.name = name;
        chart.board = board;
        self.replace_chart(chart);
    }

    /// Chart rows per chart column that keep the source picture's
    /// proportions - the picture's height/width, adjusted for cells that
    /// aren't square (a knit chart needs more rows than columns for the
    /// same shape, since each row is shorter than a stitch is wide).
    fn source_aspect(&self) -> Option<f32> {
        self.source_image.as_ref().map(|s| {
            s.height().max(1) as f32 / s.width().max(1) as f32 / self.chart.fabric.cell_aspect()
        })
    }

    fn resize(&mut self, width: usize, height: usize) {
        if self.source_image.is_some() {
            self.rerender(width, height);
        } else {
            self.chart.crop_or_pad(width, height);
            self.texture_dirty = true;
        }
    }

    /// Largest re-render (keeping the source's aspect) whose stitched
    /// area fits `hoop_in`. Binary search on width - `fits_in_hoop` is
    /// monotonic in size for a fixed source image, near enough.
    fn fit_to_hoop(&mut self) {
        let Some(aspect) = self.source_aspect() else {
            return;
        };
        let count = self.chart.fabric.count;
        let (mut lo, mut hi) = (1usize, ((self.hoop_in * count).ceil() as usize).max(2));
        let height_for = |w: usize| ((w as f32 * aspect).round() as usize).max(1);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            self.rerender(mid, height_for(mid));
            if self.chart.fits_in_hoop(self.hoop_in) {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        self.rerender(lo, height_for(lo));
    }

    /// Removes backstitch lines and knots within `PICK_RADIUS` of `p`
    /// (in squares). Returns whether anything was removed.
    fn erase_lines_near(&mut self, p: (f32, f32)) -> bool {
        let (nb, nk) = (self.chart.backstitches.len(), self.chart.knots.len());
        self.chart.backstitches.retain(|b| {
            dist_to_segment(p, half_to_squares(b.from), half_to_squares(b.to)) > PICK_RADIUS
        });
        self.chart.knots.retain(|k| {
            let (kx, ky) = half_to_squares(k.at);
            ((kx - p.0).powi(2) + (ky - p.1).powi(2)).sqrt() > PICK_RADIUS
        });
        nb != self.chart.backstitches.len() || nk != self.chart.knots.len()
    }
}

/// Where the pointer is on the chart.
struct PointerAt {
    /// Position in squares.
    p: (f32, f32),
    /// Square under the pointer.
    cell: (usize, usize),
    /// Nearest grid point, half-square units.
    snap: (u32, u32),
}

/// Applies the current tool at `at`. Returns whether the chart changed.
fn apply_tool(state: &mut CrossStitchState, at: &PointerAt, input: &ToolInput) -> bool {
    let (cx, cy) = at.cell;
    let sel = (!state.chart.palette.is_empty()).then_some(state.selected as u16);
    let chart = &mut state.chart;
    let before_cell = (chart.get(cx, cy), chart.partial(cx, cy).cloned());
    let square_changed = |c: &Chart| (c.get(cx, cy), c.partial(cx, cy).cloned()) != before_cell;

    match state.tool {
        Tool::Full | Tool::Half(_) | Tool::ThreeQuarter | Tool::Quarter if input.secondary_down => {
            chart.clear_square(cx, cy);
            square_changed(chart)
        }
        Tool::Full => {
            let (Some(sel), true) = (sel, input.primary_down) else {
                return false;
            };
            chart.set(cx, cy, Some(sel));
            square_changed(chart)
        }
        Tool::Half(diagonal) => {
            let (Some(floss), true) = (sel, input.primary_down) else {
                return false;
            };
            chart.set_partial(cx, cy, Partial::Half { diagonal, floss });
            square_changed(chart)
        }
        Tool::ThreeQuarter => {
            let (Some(sel), true) = (sel, input.primary_down) else {
                return false;
            };
            let (fx, fy) = (at.p.0 - cx as f32, at.p.1 - cy as f32);
            let (diagonal, is_first) = Partial::split_side(Corner::nearest(fx, fy));
            let (mut first, mut second) = match chart.partial(cx, cy) {
                Some(Partial::Split {
                    diagonal: d,
                    first,
                    second,
                }) if *d == diagonal => (*first, *second),
                _ => (None, None),
            };
            if is_first {
                first = Some(sel);
            } else {
                second = Some(sel);
            }
            chart.set_partial(
                cx,
                cy,
                Partial::Split {
                    diagonal,
                    first,
                    second,
                },
            );
            square_changed(chart)
        }
        Tool::Quarter => {
            let (Some(sel), true) = (sel, input.primary_down) else {
                return false;
            };
            let (fx, fy) = (at.p.0 - cx as f32, at.p.1 - cy as f32);
            let mut q = match chart.partial(cx, cy) {
                Some(Partial::Quarters(q)) => *q,
                _ => [None; 4],
            };
            q[Corner::nearest(fx, fy) as usize] = Some(sel);
            chart.set_partial(cx, cy, Partial::Quarters(q));
            square_changed(chart)
        }
        Tool::Fill => {
            if input.clicked {
                if let Some(sel) = sel {
                    chart.flood_fill(cx, cy, Some(sel));
                }
            } else if input.secondary_clicked {
                chart.flood_fill(cx, cy, None);
            }
            square_changed(chart)
        }
        Tool::Knot => {
            if input.secondary_clicked {
                return state.erase_lines_near(at.p);
            }
            let (Some(floss), true) = (sel, input.clicked) else {
                return false;
            };
            let knot = Knot { at: at.snap, floss };
            match chart.knots.iter_mut().find(|k| k.at == at.snap) {
                Some(k) if *k == knot => false,
                Some(k) => {
                    *k = knot;
                    true
                }
                None => {
                    chart.knots.push(knot);
                    true
                }
            }
        }
        Tool::Backstitch => {
            if input.secondary_clicked {
                return state.erase_lines_near(at.p);
            }
            if input.primary_pressed {
                state.bs_start = Some(at.snap);
            }
            if input.primary_released {
                if let (Some(from), Some(floss)) = (state.bs_start.take(), sel) {
                    let b = Backstitch {
                        from,
                        to: at.snap,
                        floss,
                    };
                    let dup = state.chart.backstitches.iter().any(|o| {
                        o.floss == floss
                            && ((o.from, o.to) == (b.from, b.to)
                                || (o.from, o.to) == (b.to, b.from))
                    });
                    if from != at.snap && !dup {
                        state.chart.backstitches.push(b);
                        return true;
                    }
                }
            }
            false
        }
        Tool::Erase => {
            if !(input.primary_down || input.secondary_down) {
                return false;
            }
            chart.clear_square(cx, cy);
            let squares = square_changed(chart);
            state.erase_lines_near(at.p) || squares
        }
    }
}

struct ToolInput {
    primary_down: bool,
    secondary_down: bool,
    primary_pressed: bool,
    primary_released: bool,
    clicked: bool,
    secondary_clicked: bool,
}

/// Returns `true` when the chart changed (the caller re-serializes it -
/// see `GoblinApp::sync_dsl_from_xstitch`).
pub fn show(ui: &mut egui::Ui, state: &mut CrossStitchState) -> bool {
    let mut modified = false;

    egui::SidePanel::left("xstitch_side")
        .resizable(true)
        .default_width(310.0)
        .show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                modified |= side_panel(ui, state);
            });
        });

    ui.horizontal_wrapped(|ui| {
        let craft = state.chart.craft;
        let (cell_one, cells) = craft.cell_word();
        let full_tip = format!("Click or drag to place {cells}. Right-click/drag removes them.");
        let fill_tip = format!("Fill the connected area of the same color (or empty). Right-click empties it. Places one {cell_one} per cell.");
        ui.label("Tool:");
        let tools = [
            (Tool::Full, if craft.has_part_stitches() { "Full \u{2716}" } else { "Draw" }, full_tip.as_str()),
            (Tool::ThreeQuarter, "\u{be}", "Three-quarter stitch toward the corner you click nearest."),
            (Tool::Half(Diagonal::Slash), "Half /", "Half stitch, bottom-left to top-right."),
            (Tool::Half(Diagonal::Backslash), "Half \\", "Half stitch, top-left to bottom-right."),
            (Tool::Quarter, "\u{bc}", "Quarter stitch in the corner you click nearest."),
            (Tool::Backstitch, "Backstitch", "Drag between grid points (corners, edge midpoints or square centers). Right-click a line to remove it."),
            (Tool::Knot, "Knot \u{2022}", "Click a grid point to place a French knot. Right-click removes."),
            (Tool::Fill, "Fill", fill_tip.as_str()),
            (Tool::Erase, "Erase", "Removes everything under the pointer."),
        ];
        for (tool, label, tip) in tools {
            let allowed = match tool {
                Tool::Full | Tool::Fill | Tool::Erase => true,
                Tool::ThreeQuarter => craft.has_triangles(),
                _ => craft.has_part_stitches(),
            };
            if !allowed {
                continue;
            }
            let (label, tip) = if tool == Tool::ThreeQuarter && craft == GridCraft::Quilt {
                ("Triangle (HST)", "Half-square triangle: click near a corner to fill that corner's triangle with the selected fabric (the other half keeps its fabric, or the background).")
            } else {
                (label, tip)
            };
            if ui.selectable_label(state.tool == tool, label).on_hover_text(tip).clicked() {
                state.tool = tool;
                state.bs_start = None;
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Zoom:");
        ui.add(egui::Slider::new(&mut state.cell_px, 4.0..=40.0).show_value(false));
        ui.checkbox(&mut state.show_symbols, "Symbols");
        ui.separator();
        ui.label(&state.hover);
    });
    ui.separator();

    state.rebuild_texture_if_needed(ui.ctx());
    let Some(texture) = state.texture.clone() else {
        return modified;
    };
    // Cells are `cw` x `ch` on screen - knit stitches are wider than
    // tall; every other craft has square cells. `cmin` sizes symbols and
    // strokes so they fit either way.
    let (cw, ch) = (
        state.cell_px,
        state.cell_px * state.chart.fabric.cell_aspect(),
    );
    let cmin = cw.min(ch);
    let (w, h) = (state.chart.width, state.chart.height);
    let size = egui::vec2(w as f32 * cw, h as f32 * ch);

    egui::ScrollArea::both().show(ui, |ui| {
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let painter = ui.painter_at(ui.clip_rect());
        painter.image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        let to_screen = |(x, y): (f32, f32)| rect.min + egui::vec2(x * cw, y * ch);

        // Only the visible part of the chart gets lines/symbols/parts.
        let visible = ui.clip_rect().intersect(rect);
        let col0 = (((visible.min.x - rect.min.x) / cw).floor().max(0.0)) as usize;
        let col1 = ((((visible.max.x - rect.min.x) / cw).ceil()) as usize).min(w);
        let row0 = (((visible.min.y - rect.min.y) / ch).floor().max(0.0)) as usize;
        let row1 = ((((visible.max.y - rect.min.y) / ch).ceil()) as usize).min(h);
        let symbols = state.show_symbols && cmin >= MIN_SYMBOL_CELL_PX;
        let palette = &state.chart.palette;
        let floss = |i: u16| palette.get(i as usize);

        if symbols {
            let font = egui::FontId::monospace(cmin * 0.72);
            for y in row0..row1 {
                for x in col0..col1 {
                    if let Some(f) = state.chart.get(x, y).and_then(floss) {
                        painter.text(
                            to_screen((x as f32 + 0.5, y as f32 + 0.5)),
                            egui::Align2::CENTER_CENTER,
                            f.symbol,
                            font.clone(),
                            c32(contrasting_ink(f.display_rgb())),
                        );
                    }
                }
            }
        }

        for (&(y, x), p) in state
            .chart
            .partials
            .range((row0, 0)..(row1, 0))
            .filter(|(&(_, x), _)| (col0..col1).contains(&x))
        {
            let at = |(dx, dy): (f32, f32)| to_screen((x as f32 + dx, y as f32 + dy));
            match p {
                Partial::Half { diagonal, floss: i } => {
                    let Some(f) = floss(*i) else { continue };
                    let [a, b] = half_stitch_ends(*diagonal).map(at);
                    painter
                        .line_segment([a, b], egui::Stroke::new(cmin * 0.3, c32(f.display_rgb())));
                }
                Partial::Split {
                    diagonal,
                    first,
                    second,
                } => {
                    for (side, v) in [(true, first), (false, second)] {
                        let Some(f) = v.and_then(floss) else { continue };
                        let pts = split_triangle(*diagonal, side);
                        painter.add(egui::Shape::convex_polygon(
                            pts.iter().map(|&p| at(p)).collect(),
                            c32(f.display_rgb()),
                            egui::Stroke::NONE,
                        ));
                        if symbols {
                            let centroid = (
                                pts.iter().map(|p| p.0).sum::<f32>() / 3.0,
                                pts.iter().map(|p| p.1).sum::<f32>() / 3.0,
                            );
                            painter.text(
                                at(centroid),
                                egui::Align2::CENTER_CENTER,
                                f.symbol,
                                egui::FontId::monospace(cmin * 0.4),
                                c32(contrasting_ink(f.display_rgb())),
                            );
                        }
                    }
                }
                Partial::Quarters(q) => {
                    for corner in Corner::ALL {
                        let Some(f) = q[corner as usize].and_then(floss) else {
                            continue;
                        };
                        let (ox, oy) = quarter_origin(corner);
                        painter.rect_filled(
                            egui::Rect::from_min_size(at((ox, oy)), egui::vec2(cw / 2.0, ch / 2.0)),
                            0.0,
                            c32(f.display_rgb()),
                        );
                    }
                }
            }
        }

        if cmin >= 5.0 {
            let thin = egui::Stroke::new(1.0_f32, Color32::from_black_alpha(50));
            let heavy = egui::Stroke::new(1.5_f32, Color32::from_black_alpha(170));
            for c in col0..=col1 {
                let x = rect.min.x + c as f32 * cw;
                let s = if state.chart.is_heavy_col_line(c) {
                    heavy
                } else {
                    thin
                };
                painter.line_segment(
                    [egui::pos2(x, visible.min.y), egui::pos2(x, visible.max.y)],
                    s,
                );
            }
            for r in row0..=row1 {
                let y = rect.min.y + r as f32 * ch;
                let s = if state.chart.is_heavy_row_line(r) {
                    heavy
                } else {
                    thin
                };
                painter.line_segment(
                    [egui::pos2(visible.min.x, y), egui::pos2(visible.max.x, y)],
                    s,
                );
            }
        }

        // Pegboard/baseplate boundaries, so the chart can be built one
        // board at a time.
        if let Some((bw, bh)) = state.chart.board {
            let board_stroke = egui::Stroke::new(2.5_f32, Color32::from_rgb(220, 40, 40));
            for c in (0..=w).step_by(bw.max(1)) {
                let x = rect.min.x + c as f32 * cw;
                painter.line_segment(
                    [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                    board_stroke,
                );
            }
            for r in (0..=h).step_by(bh.max(1)) {
                let y = rect.min.y + r as f32 * ch;
                painter.line_segment(
                    [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                    board_stroke,
                );
            }
        }

        let bs_width = (cmin * 0.16).max(2.0);
        let outline = egui::Stroke::new(bs_width + 2.0, Color32::from_black_alpha(120));
        for b in &state.chart.backstitches {
            let Some(f) = floss(b.floss) else { continue };
            let seg = [
                to_screen(half_to_squares(b.from)),
                to_screen(half_to_squares(b.to)),
            ];
            painter.line_segment(seg, outline);
            painter.line_segment(seg, egui::Stroke::new(bs_width, c32(f.display_rgb())));
        }
        let knot_r = (cmin * 0.25).max(2.5);
        for k in &state.chart.knots {
            let Some(f) = floss(k.floss) else { continue };
            let c = to_screen(half_to_squares(k.at));
            painter.circle(
                c,
                knot_r,
                c32(f.display_rgb()),
                egui::Stroke::new(1.0_f32, Color32::BLACK),
            );
        }

        let pointer = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|p| rect.contains(*p));
        let released = ui.input(|i| i.pointer.primary_released());
        let Some(pos) = pointer else {
            state.hover.clear();
            if released {
                state.bs_start = None;
            }
            return;
        };
        let local = pos - rect.min;
        let p = (local.x / cw, local.y / ch);
        let at = PointerAt {
            p,
            cell: ((p.0 as usize).min(w - 1), (p.1 as usize).min(h - 1)),
            snap: (
                ((p.0 * 2.0).round().max(0.0) as u32).min(2 * w as u32),
                ((p.1 * 2.0).round().max(0.0) as u32).min(2 * h as u32),
            ),
        };
        let (cx, cy) = at.cell;
        let describe =
            |i: u16| floss(i).map_or("?".to_string(), |f| format!("{} {}", f.symbol, f.label()));
        let chart = &state.chart;
        let (col, row) = (chart.col_number(cx), chart.row_number(cy));
        let place = match chart.craft {
            GridCraft::Knitting => format!("row {row}, st {col}"),
            _ => format!("col {col}, row {row}"),
        };
        let empty = match chart.craft {
            GridCraft::Knitting => "main color",
            GridCraft::Quilt => "background fabric",
            GridCraft::CrossStitch => "unstitched",
            _ => "empty",
        };
        state.hover = match (chart.get(cx, cy), chart.partial(cx, cy)) {
            (Some(i), _) => format!("{place}: {}", describe(i)),
            (None, Some(p)) => {
                let names: Vec<String> = p.flosses().into_iter().map(describe).collect();
                let what = if chart.craft == GridCraft::Quilt {
                    "half-square triangle"
                } else {
                    "part stitches"
                };
                format!("{place}: {what} - {}", names.join(", "))
            }
            (None, None) => format!("{place}: {empty}"),
        };

        // Grid-point cursor and live preview for the line/point tools.
        if matches!(state.tool, Tool::Backstitch | Tool::Knot) {
            let snap_pos = to_screen(half_to_squares(at.snap));
            painter.circle_stroke(
                snap_pos,
                4.0,
                egui::Stroke::new(1.5_f32, Color32::from_rgb(255, 200, 0)),
            );
            if let (Some(start), Some(f)) = (state.bs_start, floss(state.selected as u16)) {
                painter.line_segment(
                    [to_screen(half_to_squares(start)), snap_pos],
                    egui::Stroke::new(bs_width, c32(f.display_rgb()).gamma_multiply(0.7)),
                );
            }
        }

        let input = ui.input(|i| ToolInput {
            primary_down: response.hovered() && i.pointer.primary_down(),
            secondary_down: response.hovered() && i.pointer.secondary_down(),
            primary_pressed: response.hovered() && i.pointer.primary_pressed(),
            primary_released: i.pointer.primary_released(),
            clicked: response.clicked(),
            secondary_clicked: response.secondary_clicked(),
        });
        if apply_tool(state, &at, &input) {
            state.texture_dirty = true;
            modified = true;
        }
    });

    modified
}

fn side_panel(ui: &mut egui::Ui, state: &mut CrossStitchState) -> bool {
    let mut modified = false;
    let craft = state.chart.craft;
    let (_, cells) = craft.cell_word();

    // Switching craft keeps the design; colors that don't belong to the
    // new craft's catalogs are re-matched (undo brings them back).
    ui.horizontal(|ui| {
        ui.label("Craft:");
        let mut new_craft = craft;
        egui::ComboBox::from_id_salt("chart_craft")
            .selected_text(craft.label())
            .show_ui(ui, |ui| {
                for g in GridCraft::ALL {
                    ui.selectable_value(&mut new_craft, g, g.label());
                }
            });
        if new_craft != craft {
            let chart = &mut state.chart;
            chart.craft = new_craft;
            if new_craft
                .size_label_xy(chart.fabric.count, chart.fabric.count_y)
                .is_none()
            {
                chart.fabric.count = new_craft.default_per_inch();
                chart.fabric.count_y = new_craft.default_per_inch_y();
            }
            chart.board = new_craft.default_board(chart.fabric.count);
            state.catalog = palette_catalog(chart);
            state.convert = ConvertSettings::for_craft(new_craft);
            state.craft_note = if palette_fits_craft(chart, new_craft) {
                String::new()
            } else {
                chart.rematch_colors(state.catalog);
                match state.catalog {
                    Some(c) => format!("Colors re-matched to {}.", c.display_name()),
                    None => "Colors converted to free colors.".to_string(),
                }
            };
            if !new_craft.has_part_stitches()
                && !(new_craft.has_triangles() && state.tool == Tool::ThreeQuarter)
            {
                state.tool = Tool::Full;
            }
            state.selected = state.selected.min(chart.palette.len().saturating_sub(1));
            state.texture_dirty = true;
            modified = true;
        }
    });
    if !state.craft_note.is_empty() {
        ui.small(&state.craft_note);
    }
    let craft = state.chart.craft;

    ui.separator();
    ui.heading(if craft == GridCraft::CrossStitch {
        "Fabric & size"
    } else {
        "Size"
    });
    ui.horizontal_wrapped(|ui| {
        let mut count = state.chart.fabric.count;
        let mut count_y = state.chart.fabric.count_y;
        if size_ui(ui, "chart_cell_size", craft, &mut count, &mut count_y) {
            state.chart.fabric.count = count;
            state.chart.fabric.count_y = count_y;
            state.texture_dirty = true;
            // A new row gauge changes how many rows keep a picture's
            // proportions.
            if let (Some(a), true) = (state.source_aspect(), craft.has_row_gauge()) {
                let w = state.chart.width;
                state.rerender(w, ((w as f32 * a).round() as usize).max(1));
            }
            // Pixelhobby's plate and pack sizes depend on pixel size.
            if !craft.boards(count).is_empty() && craft == GridCraft::PixelHobby {
                state.chart.board = craft.default_board(count);
            }
            modified = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(match craft {
            GridCraft::CrossStitch => "Fabric color:",
            GridCraft::Knitting => "Main color (MC):",
            GridCraft::Quilt => "Background fabric:",
            _ => "Background:",
        });
        if egui::color_picker::color_edit_button_srgb(ui, &mut state.chart.fabric.rgb).changed() {
            state.texture_dirty = true;
            modified = true;
        }
    });

    let (mut w, mut h) = (state.chart.width, state.chart.height);
    let aspect = state.source_aspect();
    let mut size_changed = false;
    ui.horizontal(|ui| {
        ui.label("Width:");
        size_changed |= ui
            .add(egui::DragValue::new(&mut w).range(1..=1000))
            .changed();
        ui.label("Height:");
        size_changed |= ui
            .add_enabled(
                aspect.is_none(),
                egui::DragValue::new(&mut h).range(1..=1000),
            )
            .changed();
        ui.label(cells);
    });
    if size_changed {
        if let Some(a) = aspect {
            h = ((w as f32 * a).round() as usize).max(1);
        }
        state.resize(w, h);
        modified = true;
    }
    let (w_in, h_in) = state.chart.finished_size_in();
    ui.label(format!(
        "Finished size: {w_in:.1} x {h_in:.1} in ({:.1} x {:.1} cm)",
        w_in * 2.54,
        h_in * 2.54
    ));

    if craft == GridCraft::Knitting {
        ui.horizontal(|ui| {
            ui.label("Worked:");
            modified |= ui
                .radio_value(&mut state.chart.worked_in_round, false, "Flat")
                .changed();
            modified |= ui
                .radio_value(&mut state.chart.worked_in_round, true, "In the round")
                .changed();
        });
        let warnings =
            abyssal_thread_crossstitch::knit::float_warnings(&state.chart, state.max_float);
        ui.horizontal(|ui| {
            ui.label("Flag floats longer than");
            ui.add(egui::DragValue::new(&mut state.max_float).range(2..=20));
            ui.label("sts");
        });
        if warnings.is_empty() {
            ui.small("\u{2714} No long floats, at most 2 colors per row.");
        } else {
            let word = if state.chart.worked_in_round {
                "Rnd"
            } else {
                "Row"
            };
            let list: Vec<String> = warnings
                .iter()
                .take(12)
                .map(|w| {
                    let mut notes = Vec::new();
                    if w.longest_float > state.max_float {
                        notes.push(format!("{}-st float", w.longest_float));
                    }
                    if w.colors > 2 {
                        notes.push(format!("{} colors", w.colors));
                    }
                    format!("{word} {}: {}", w.row, notes.join(", "))
                })
                .collect();
            ui.colored_label(
                Color32::from_rgb(220, 150, 60),
                format!("\u{26a0} {} row(s) to check for stranded knitting:", warnings.len()),
            )
            .on_hover_text("Long floats snag - trap them every few stitches. Rows with 3+ colors are awkward stranded; consider intarsia or duplicate stitch.");
            ui.small(list.join("\n"));
            if warnings.len() > 12 {
                ui.small(format!(
                    "...and {} more (see the written instructions).",
                    warnings.len() - 12
                ));
            }
        }
    }

    if craft.has_hoop() {
        ui.horizontal(|ui| {
            ui.label("Hoop:");
            egui::ComboBox::from_id_salt("xstitch_hoop")
                .selected_text(format!("{} in", state.hoop_in))
                .show_ui(ui, |ui| {
                    for &d in COMMON_HOOPS_IN {
                        ui.selectable_value(&mut state.hoop_in, d, format!("{d} in"));
                    }
                });
            if state.chart.fits_in_hoop(state.hoop_in) {
                ui.colored_label(Color32::from_rgb(90, 180, 90), "\u{2714} fits");
            } else {
                ui.colored_label(Color32::from_rgb(220, 120, 60), "\u{2718} too big");
            }
        });
        let across = ((state.hoop_in - 2.0 * HOOP_MARGIN_IN) * state.chart.fabric.count).floor();
        ui.small(format!(
            "A {} in hoop shows about {across} squares across at {}-count ({HOOP_MARGIN_IN} in margin).",
            state.hoop_in, state.chart.fabric.count
        ));
        if state.source_image.is_some()
            && ui
                .button("Size design to fit hoop")
                .on_hover_text("Re-renders the picture as large as it can be while still fitting inside the hoop.")
                .clicked()
        {
            state.fit_to_hoop();
            modified = true;
        }
    }

    let boards = craft.boards(state.chart.fabric.count);
    if !boards.is_empty() {
        let word = craft.board_word();
        ui.horizontal(|ui| {
            let mut use_boards = state.chart.board.is_some();
            if ui
                .checkbox(&mut use_boards, format!("Split into {word}s"))
                .changed()
            {
                state.chart.board = use_boards.then(|| (boards[0].width, boards[0].height));
                modified = true;
            }
        });
        if let Some((mut bw, mut bh)) = state.chart.board {
            ui.horizontal(|ui| {
                let mut changed = false;
                egui::ComboBox::from_id_salt("chart_board")
                    .selected_text(format!("{bw} x {bh}"))
                    .show_ui(ui, |ui| {
                        for b in boards {
                            if ui
                                .selectable_label((bw, bh) == (b.width, b.height), b.label)
                                .clicked()
                            {
                                (bw, bh) = (b.width, b.height);
                                changed = true;
                            }
                        }
                    });
                changed |= ui
                    .add(egui::DragValue::new(&mut bw).range(1..=200))
                    .changed();
                ui.label("x");
                changed |= ui
                    .add(egui::DragValue::new(&mut bh).range(1..=200))
                    .changed();
                if changed {
                    state.chart.board = Some((bw, bh));
                    modified = true;
                }
            });
            if let Some((nx, ny)) = state.chart.boards_needed() {
                ui.small(format!(
                    "{} {word}{} ({nx} across x {ny} down) - red lines in the chart.",
                    nx * ny,
                    if nx * ny == 1 { "" } else { "s" }
                ));
            }
        }
    }

    if state.source_image.is_some() {
        ui.separator();
        ui.heading("From picture");
        let mut rerender = false;
        ui.horizontal(|ui| {
            ui.label("Max colors:");
            rerender |= ui
                .add(egui::Slider::new(&mut state.colors, 1..=craft.max_colors()))
                .changed();
        });
        rerender |= convert_settings_ui(ui, "chart_convert_catalog", craft, &mut state.convert);
        if rerender {
            state.catalog = state.convert.catalog;
            let (w, h) = (state.chart.width, state.chart.height);
            state.rerender(w, h);
            modified = true;
        }
        ui.small("Re-rendering replaces hand edits - do touch-ups last.");
    } else {
        ui.horizontal(|ui| {
            catalog_ui(ui, "chart_catalog", craft, &mut state.catalog);
        });
    }
    if !palette_fits_craft(&state.chart, craft)
        && ui
            .button(match state.catalog {
                Some(c) => format!("Re-match all colors to {}", c.display_name()),
                None => "Convert all colors to free colors".to_string(),
            })
            .on_hover_text("Replaces every color with its closest match, merging colors that land on the same one.")
            .clicked()
    {
        state.chart.rematch_colors(state.catalog);
        state.selected = state.selected.min(state.chart.palette.len().saturating_sub(1));
        state.texture_dirty = true;
        modified = true;
    }

    ui.separator();
    ui.heading("Clean up");
    ui.horizontal(|ui| {
        ui.label("Merge specks smaller than");
        ui.add(egui::DragValue::new(&mut state.cleanup_min).range(2..=20));
        ui.label(cells);
    });
    if ui
        .button("Clean up confetti")
        .on_hover_text("Recolors small isolated patches (lone cells, tiny holes) to whatever surrounds them. Undo if you don't like the result.")
        .clicked()
    {
        let n = state.chart.remove_confetti(state.cleanup_min);
        state.cleanup_result = format!("{n} cell{} changed", if n == 1 { "" } else { "s" });
        if n > 0 {
            state.texture_dirty = true;
            modified = true;
        }
    }
    if !state.cleanup_result.is_empty() {
        ui.small(&state.cleanup_result);
    }

    ui.separator();
    ui.heading(if craft == GridCraft::CrossStitch {
        "Floss"
    } else {
        "Colors"
    });
    let usage = state.chart.usage();
    let mut remove = None;
    let catalog = state.catalog;
    for (i, f) in state.chart.palette.clone().iter().enumerate() {
        let resp = ui
            .horizontal(|ui| {
                let r = swatch(ui, f, 20.0, i == state.selected);
                let hover = match usage.get(i) {
                    Some(u) if craft.has_part_stitches() => usage_summary(u),
                    Some(u) => format!("{} {cells}", u.full),
                    None => String::new(),
                };
                ui.label(f.label()).on_hover_text(hover);
                r
            })
            .inner;
        if resp.clicked() {
            state.selected = i;
            if matches!(state.tool, Tool::Erase) {
                state.tool = Tool::Full;
            }
        }
        resp.context_menu(|ui| {
            ui.label(f.label());
            if craft.has_part_stitches() {
                ui.horizontal(|ui| {
                    ui.label("Strands:");
                    let mut strands = f.strands;
                    if ui
                        .add(egui::DragValue::new(&mut strands).range(1..=6))
                        .changed()
                    {
                        state.chart.palette[i].strands = strands;
                        modified = true;
                    }
                    ui.label("Backstitch strands:");
                    let mut bs = f.bs_strands;
                    if ui.add(egui::DragValue::new(&mut bs).range(1..=6)).changed() {
                        state.chart.palette[i].bs_strands = bs;
                        modified = true;
                    }
                });
                ui.separator();
                match &f.blend {
                    Some(b) => {
                        ui.label(format!("Blended with {}", b.label()));
                        if ui.button("Remove blend").clicked() {
                            state.chart.palette[i].blend = None;
                            state.texture_dirty = true;
                            modified = true;
                            ui.close_menu();
                        }
                    }
                    None => {
                        ui.horizontal(|ui| {
                            ui.label("Blend with:");
                            ui.add(
                                egui::TextEdit::singleline(&mut state.blend_code)
                                    .hint_text("e.g. 3865 or Anchor 403")
                                    .desired_width(120.0),
                            );
                        });
                        let found = find_thread(&state.blend_code, catalog.unwrap_or(Catalog::Dmc));
                        match found {
                            Some(t) => {
                                if ui
                                    .button(format!("Blend with {} {} {}", t.brand, t.code, t.name))
                                    .clicked()
                                {
                                    state.chart.palette[i].blend =
                                        Some(BlendThread::from_thread(t));
                                    state.texture_dirty = true;
                                    modified = true;
                                    ui.close_menu();
                                }
                            }
                            None if !state.blend_code.trim().is_empty() => {
                                ui.weak("No such thread");
                            }
                            None => {}
                        }
                    }
                }
                ui.separator();
            }
            if ui.button("Remove (empties every cell using it)").clicked() {
                remove = Some(i);
                ui.close_menu();
            }
        });
    }
    if state.chart.palette.is_empty() {
        ui.label("No colors yet - add one below.");
    } else if craft.has_part_stitches() {
        ui.small("Click to select. Right-click for strands, blends, or to remove.");
    } else {
        ui.small("Click to select. Right-click to remove.");
    }
    if let Some(i) = remove {
        state.chart.remove_floss(i);
        state.selected = state
            .selected
            .min(state.chart.palette.len().saturating_sub(1));
        state.texture_dirty = true;
        modified = true;
    }

    let add_label = if craft == GridCraft::CrossStitch {
        "Add floss"
    } else {
        "Add color"
    };
    ui.collapsing(add_label, |ui| {
        let mut picked = None;
        match state.catalog {
            None => {
                ui.horizontal(|ui| {
                    egui::color_picker::color_edit_button_srgb(ui, &mut state.free_color);
                    if ui.button("Add this color").clicked() {
                        picked = Some(Floss::free(state.free_color, ' '));
                    }
                });
            }
            Some(cat) => {
                ui.horizontal(|ui| {
                    ui.label("Closest to:");
                    egui::color_picker::color_edit_button_srgb(ui, &mut state.suggest_color);
                });
                ui.horizontal_wrapped(|ui| {
                    for t in cat.nearest_n(state.suggest_color, 8) {
                        let f = Floss::from_thread(t, ' ');
                        if swatch(ui, &f, 22.0, false)
                            .on_hover_text(f.label())
                            .clicked()
                        {
                            picked = Some(f);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(format!("Search {}:", cat.display_name()));
                    ui.text_edit_singleline(&mut state.floss_query);
                });
                egui::ScrollArea::vertical()
                    .id_salt("floss_search")
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for t in cat.search(&state.floss_query).into_iter().take(200) {
                            let f = Floss::from_thread(t, ' ');
                            let already = state.chart.palette.iter().any(|p| p.same_thread(&f));
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(16.0, 16.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(rect, 2.0, c32(f.rgb));
                                if ui
                                    .add_enabled(
                                        !already,
                                        egui::Button::new(f.label()).frame(false),
                                    )
                                    .clicked()
                                {
                                    picked = Some(f.clone());
                                }
                            });
                        }
                    });
            }
        }
        if let Some(f) = picked {
            let symbol = state.chart.next_symbol();
            state.selected = state.chart.add_floss(Floss { symbol, ..f }) as usize;
            if matches!(state.tool, Tool::Erase) {
                state.tool = Tool::Full;
            }
            modified = true;
        }
    });

    if let Some(mut text) = match craft {
        GridCraft::Knitting => Some(abyssal_thread_crossstitch::knit::instructions(
            &state.chart,
            state.max_float,
        )),
        GridCraft::Quilt => Some(abyssal_thread_crossstitch::quilt::instructions(
            &state.chart,
        )),
        _ => None,
    } {
        ui.separator();
        let title = if craft == GridCraft::Quilt {
            "Cutting & assembly"
        } else {
            "Written instructions"
        };
        ui.collapsing(title, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("chart_instructions")
                .max_height(240.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut text)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .interactive(false),
                    );
                });
            if ui.button("Copy").clicked() {
                ui.output_mut(|o| o.copied_text = text.clone());
            }
        });
    }

    ui.separator();
    ui.collapsing("Export image (PNG)", |ui| {
        ui.horizontal(|ui| {
            ui.label("Pixels per cell:");
            ui.add(egui::DragValue::new(&mut state.png_scale).range(1..=40));
            ui.checkbox(&mut state.png_grid, "Grid lines");
        });
        if craft == GridCraft::PixelArt {
            ui.small("1 pixel per cell gives the pixel art itself; empty cells are transparent.");
        }
        if ui.button("Save PNG...").clicked() {
            let name = state
                .chart
                .name
                .clone()
                .unwrap_or_else(|| "chart".to_string());
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG image", &["png"])
                .set_file_name(format!("{name}.png"))
                .save_file()
            {
                state.png_result =
                    match render_image(&state.chart, state.png_scale, state.png_grid).save(&path) {
                        Ok(()) => format!("Saved {}", path.display()),
                        Err(e) => format!("Couldn't save: {e}"),
                    };
            }
        }
        if !state.png_result.is_empty() {
            ui.small(&state.png_result);
        }
    });
    modified
}

/// Materials tab: design summary, the color key, and the shopping list,
/// worded for the chart's craft.
pub fn show_materials(ui: &mut egui::Ui, chart: &Chart) {
    let craft = chart.craft;
    let (_, cells) = craft.cell_word();
    for line in abyssal_thread_crossstitch::export::info_lines(chart) {
        ui.label(line);
    }
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Key");
        egui::Grid::new("materials_key")
            .striped(true)
            .num_columns(4)
            .show(ui, |ui| {
                if craft.has_part_stitches() {
                    for h in ["", "Floss", "Strands", "Used for"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for (f, u) in chart.palette.iter().zip(chart.usage()) {
                        swatch(ui, f, 20.0, false);
                        ui.label(f.label());
                        let strands = if f.blend.is_some() {
                            format!("{} each", f.strands)
                        } else {
                            f.strands.to_string()
                        };
                        ui.label(if u.backstitch_squares > 0.0 {
                            format!("{strands} (backstitch {})", f.bs_strands)
                        } else {
                            strands
                        });
                        ui.label(usage_summary(&u));
                        ui.end_row();
                    }
                } else {
                    for h in ["", "Color", cells] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for (f, u) in chart.palette.iter().zip(chart.usage()) {
                        swatch(ui, f, 20.0, false);
                        ui.label(f.label());
                        ui.label(u.full.to_string());
                        ui.end_row();
                    }
                }
            });
        ui.add_space(8.0);
        ui.heading("Shopping list");
        let list = chart.shopping_list();
        let mut total = 0;
        egui::Grid::new("materials_shopping")
            .striped(true)
            .num_columns(3)
            .show(ui, |ui| {
                for item in &list {
                    total += item.buy;
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, c32(item.rgb));
                    ui.label(&item.label);
                    let buy = item.buy_text();
                    ui.label(if buy.is_empty() { format!("{} {cells}", item.used) } else { buy });
                    ui.end_row();
                }
            });
        ui.separator();
        if craft == GridCraft::CrossStitch {
            ui.label(format!(
                "Total: {total} skeins. Estimates assume about 1,800 full crosses per skein at 14-count with 2 strands (part stitches, backstitch and knots scaled from that) - round up if unsure."
            ));
        } else if let Some(p) = craft.packaging(chart.fabric.count) {
            ui.label(format!(
                "Total: {total} packs, assuming one {}{} - pack sizes vary by seller.",
                p.unit,
                if p.spare > 0.0 { format!(" and {:.0}% spare", p.spare * 100.0) } else { String::new() }
            ));
        }
        show_instructions(ui, chart);
    });
    if ui.button("Copy shopping list").clicked() {
        ui.output_mut(|o| {
            o.copied_text = abyssal_thread_crossstitch::export::materials_text(chart)
        });
    }
}

/// Knitting's written row instructions / a quilt's cutting list and
/// assembly, under the Materials tab's lists.
fn show_instructions(ui: &mut egui::Ui, chart: &Chart) {
    let Some(mut text) = abyssal_thread_crossstitch::export::instructions_text(chart) else {
        return;
    };
    ui.add_space(8.0);
    ui.heading(if chart.craft == GridCraft::Quilt {
        "Cutting & assembly"
    } else {
        "Written instructions"
    });
    ui.add(
        egui::TextEdit::multiline(&mut text)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY)
            .interactive(false),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use abyssal_thread_crossstitch::threads::find_dmc;

    fn state() -> CrossStitchState {
        let mut c = Chart::new(4, 4);
        c.add_floss(Floss::from_thread(find_dmc("310").unwrap(), 'X'));
        c.add_floss(Floss::from_thread(find_dmc("666").unwrap(), 'O'));
        CrossStitchState::new(c)
    }

    fn at(x: f32, y: f32) -> PointerAt {
        PointerAt {
            p: (x, y),
            cell: (x as usize, y as usize),
            snap: ((x * 2.0).round() as u32, (y * 2.0).round() as u32),
        }
    }

    fn input() -> ToolInput {
        ToolInput {
            primary_down: false,
            secondary_down: false,
            primary_pressed: false,
            primary_released: false,
            clicked: false,
            secondary_clicked: false,
        }
    }

    fn press() -> ToolInput {
        ToolInput {
            primary_down: true,
            clicked: true,
            ..input()
        }
    }

    #[test]
    fn three_quarter_fills_the_triangle_of_the_nearest_corner() {
        let mut s = state();
        s.tool = Tool::ThreeQuarter;
        // Top-left corner of square (1,1) -> "/" split, first triangle.
        assert!(apply_tool(&mut s, &at(1.2, 1.1), &press()));
        assert_eq!(
            s.chart.partial(1, 1),
            Some(&Partial::Split {
                diagonal: Diagonal::Slash,
                first: Some(0),
                second: None
            })
        );
        // Opposite corner in another floss keeps the first triangle.
        s.selected = 1;
        assert!(apply_tool(&mut s, &at(1.9, 1.9), &press()));
        assert_eq!(
            s.chart.partial(1, 1),
            Some(&Partial::Split {
                diagonal: Diagonal::Slash,
                first: Some(0),
                second: Some(1)
            })
        );
        // Same again changes nothing.
        assert!(!apply_tool(&mut s, &at(1.9, 1.9), &press()));
        // A corner on the other diagonal starts a fresh "\" split.
        assert!(apply_tool(&mut s, &at(1.9, 1.1), &press()));
        assert_eq!(
            s.chart.partial(1, 1),
            Some(&Partial::Split {
                diagonal: Diagonal::Backslash,
                first: None,
                second: Some(1)
            })
        );
    }

    #[test]
    fn quarter_half_and_full_replace_each_other_and_right_click_clears() {
        let mut s = state();
        s.tool = Tool::Quarter;
        apply_tool(&mut s, &at(0.9, 0.9), &press());
        apply_tool(&mut s, &at(0.1, 0.1), &press());
        assert_eq!(
            s.chart.partial(0, 0),
            Some(&Partial::Quarters([Some(0), None, None, Some(0)]))
        );
        s.tool = Tool::Half(Diagonal::Backslash);
        apply_tool(&mut s, &at(0.5, 0.5), &press());
        assert_eq!(
            s.chart.partial(0, 0),
            Some(&Partial::Half {
                diagonal: Diagonal::Backslash,
                floss: 0
            })
        );
        s.tool = Tool::Full;
        apply_tool(&mut s, &at(0.5, 0.5), &press());
        assert_eq!((s.chart.get(0, 0), s.chart.partial(0, 0)), (Some(0), None));
        let right = ToolInput {
            secondary_down: true,
            ..input()
        };
        assert!(apply_tool(&mut s, &at(0.5, 0.5), &right));
        assert!(!s.chart.is_stitched(0, 0));
    }

    #[test]
    fn backstitch_is_drawn_from_press_to_release_and_right_click_removes_it() {
        let mut s = state();
        s.tool = Tool::Backstitch;
        let pressed = ToolInput {
            primary_pressed: true,
            primary_down: true,
            ..input()
        };
        assert!(!apply_tool(&mut s, &at(0.0, 0.0), &pressed));
        let released = ToolInput {
            primary_released: true,
            ..input()
        };
        assert!(apply_tool(&mut s, &at(2.5, 1.0), &released));
        assert_eq!(
            s.chart.backstitches,
            vec![Backstitch {
                from: (0, 0),
                to: (5, 2),
                floss: 0
            }]
        );
        // Redrawing the same line backwards doesn't duplicate it.
        apply_tool(&mut s, &at(2.5, 1.0), &pressed);
        assert!(!apply_tool(&mut s, &at(0.0, 0.0), &released));
        // A click without moving adds nothing.
        apply_tool(&mut s, &at(3.0, 3.0), &pressed);
        assert!(!apply_tool(&mut s, &at(3.0, 3.0), &released));
        let right = ToolInput {
            secondary_clicked: true,
            ..input()
        };
        assert!(
            apply_tool(&mut s, &at(1.25, 0.55), &right),
            "near the line's midpoint"
        );
        assert!(s.chart.backstitches.is_empty());
    }

    #[test]
    fn knots_snap_to_grid_points_and_recolor_in_place() {
        let mut s = state();
        s.tool = Tool::Knot;
        assert!(apply_tool(&mut s, &at(1.4, 1.6), &press()));
        assert_eq!(
            s.chart.knots,
            vec![Knot {
                at: (3, 3),
                floss: 0
            }]
        );
        assert!(
            !apply_tool(&mut s, &at(1.5, 1.5), &press()),
            "same knot again"
        );
        s.selected = 1;
        assert!(apply_tool(&mut s, &at(1.5, 1.5), &press()));
        assert_eq!(
            s.chart.knots,
            vec![Knot {
                at: (3, 3),
                floss: 1
            }]
        );
        s.tool = Tool::Erase;
        let drag = ToolInput {
            primary_down: true,
            ..input()
        };
        assert!(apply_tool(&mut s, &at(1.6, 1.4), &drag));
        assert!(s.chart.knots.is_empty());
    }

    #[test]
    fn every_grid_craft_converts_a_picture_with_its_own_catalog_and_board() {
        let mut img = image::RgbImage::from_pixel(30, 30, image::Rgb([250, 250, 250]));
        for y in 10..20 {
            for x in 10..20 {
                img.put_pixel(x, y, image::Rgb([200, 30, 40]));
            }
        }
        for craft in GridCraft::ALL {
            let fabric = Fabric {
                count: craft.default_per_inch(),
                ..Fabric::default()
            };
            let chart = chart_from_image(&img, 4, fabric, ConvertSettings::for_craft(craft), craft);
            assert_eq!(chart.craft, craft);
            assert_eq!(
                chart.board,
                craft.default_board(chart.fabric.count),
                "{craft:?}"
            );
            assert!(
                palette_fits_craft(&chart, craft),
                "{craft:?}: {:?}",
                chart.palette
            );
            assert_eq!(
                palette_catalog(&chart),
                craft.default_catalog(),
                "{craft:?}"
            );
            let red = chart.get(15, 15).expect("motif is filled");
            let rgb = chart.palette[red as usize].rgb;
            assert!(
                rgb[0] as i32 > rgb[1] as i32 + 60,
                "{craft:?}: red matched to {rgb:?}"
            );
            if craft.default_skip_background() {
                assert_eq!(chart.get(0, 0), None, "{craft:?}");
            } else {
                assert!(chart.get(0, 0).is_some(), "{craft:?}");
            }
        }
    }

    #[test]
    fn switching_catalog_is_detected_as_a_mismatch() {
        let mut c = Chart::new_for(GridCraft::FuseBeads);
        c.add_floss(Floss::from_thread(find_dmc("310").unwrap(), 'X'));
        assert!(!palette_fits_craft(&c, GridCraft::FuseBeads));
        assert!(palette_fits_craft(&c, GridCraft::CrossStitch));
        c.rematch_colors(Some(Catalog::Hama));
        assert!(palette_fits_craft(&c, GridCraft::FuseBeads));
        assert_eq!(palette_catalog(&c), Some(Catalog::Hama));
    }

    #[test]
    fn png_render_scales_cells_and_is_transparent_only_for_pixel_art() {
        let mut c = Chart::new_for(GridCraft::PixelArt);
        c.crop_or_pad(2, 1);
        c.add_floss(Floss::free([10, 20, 30], 'X'));
        c.set(0, 0, Some(0));
        let img = render_image(&c, 3, false);
        assert_eq!(img.dimensions(), (6, 3));
        assert_eq!(img.get_pixel(2, 2).0, [10, 20, 30, 255]);
        assert_eq!(
            img.get_pixel(3, 0).0[3],
            0,
            "empty pixel-art cell is transparent"
        );
        c.craft = GridCraft::FuseBeads;
        let img = render_image(&c, 1, false);
        assert_eq!(
            img.get_pixel(1, 0).0,
            [255, 255, 255, 255],
            "background color"
        );
        let gridded = render_image(&c, 4, true);
        assert_eq!(gridded.get_pixel(0, 0).0[3], 90, "grid line");
    }

    #[test]
    fn knit_charts_keep_picture_proportions_with_more_rows() {
        // A square picture on DK gauge (22 sts x 30 rows / 4 in) needs
        // 30/22 as many rows as stitches to stay square.
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            100,
            100,
            image::Rgb([200, 0, 0]),
        ));
        let mut chart = Chart::new_for(GridCraft::Knitting);
        chart.crop_or_pad(22, 22);
        let mut s = CrossStitchState::from_import(
            chart,
            img,
            2,
            ResizeFilter::Nearest,
            ConvertSettings::for_craft(GridCraft::Knitting),
        );
        let a = s.source_aspect().unwrap();
        assert!((a - 30.0 / 22.0).abs() < 1e-4, "{a}");
        s.resize(22, (22.0 * a).round() as usize);
        assert_eq!((s.chart.width, s.chart.height), (22, 30));
        let (w, h) = s.chart.finished_size_in();
        assert!(
            (w - h).abs() < 0.01,
            "finished shape stays square: {w} x {h}"
        );
        // PNG rows are proportionally shorter too.
        let img = render_image(&s.chart, 10, false);
        assert_eq!(img.dimensions(), (220, 30 * 7));
    }

    #[test]
    fn quilt_triangles_use_the_three_quarter_tool() {
        let mut c = Chart::new_for(GridCraft::Quilt);
        c.add_floss(Floss::free([200, 0, 0], 'X'));
        let mut s = CrossStitchState::new(c);
        s.tool = Tool::ThreeQuarter;
        assert!(apply_tool(&mut s, &at(0.1, 0.9), &press()));
        assert_eq!(
            s.chart.partial(0, 0),
            Some(&Partial::Split {
                diagonal: Diagonal::Backslash,
                first: Some(0),
                second: None
            })
        );
        let text = abyssal_thread_crossstitch::quilt::instructions(&s.chart);
        assert!(text.contains("Row 1: X\\BG BG"), "{text}");
    }

    #[test]
    fn an_empty_palette_can_only_erase() {
        let mut s = CrossStitchState::new(Chart::new(2, 2));
        for tool in [
            Tool::Full,
            Tool::ThreeQuarter,
            Tool::Quarter,
            Tool::Knot,
            Tool::Fill,
        ] {
            s.tool = tool;
            assert!(!apply_tool(&mut s, &at(0.5, 0.5), &press()), "{tool:?}");
        }
    }
}
