//! A small "recently used colors" strip shared between the colorwork paint
//! grid (`colorwork_grid.rs`) and the shaped-pattern grid editor's
//! per-stitch color picker (`grid.rs`). Before this, each grew its own
//! color UI independently - colorwork got a real palette (derived from the
//! grid's own pixel colors, plus manual add/remove), while the shaped
//! grid's popup was a bare `color_edit_button_srgb` with no memory of
//! anything picked before. A mixed-technique pattern (e.g. a colorwork
//! amigurumi body with a few hand-picked accent stitches in the shaped
//! part) had no way to reuse a color between the two editors at all.
//!
//! `RecentColors` is intentionally minimal and caller-agnostic: it doesn't
//! know about `ColorGrid`, `GridCell`, or either editor's data model. It
//! just remembers colors and renders a row of clickable swatches, leaving
//! "what picking one means" (add to a palette, fill in a popup's field,
//! etc.) entirely up to the caller.

use eframe::egui::{self, Color32};

/// How many recent colors to remember. Small on purpose - this is a quick
/// "grab something I used a minute ago" strip, not a color history archive.
const MAX_RECENT: usize = 12;

#[derive(Debug, Clone, Default)]
pub struct RecentColors {
    /// Most-recently-used first.
    colors: Vec<[u8; 3]>,
}

impl RecentColors {
    /// Records a color as most-recently-used. Moves it to the front if
    /// already present (rather than storing a duplicate entry) and evicts
    /// the oldest color once `MAX_RECENT` is exceeded.
    pub fn record(&mut self, color: [u8; 3]) {
        self.colors.retain(|&c| c != color);
        self.colors.insert(0, color);
        self.colors.truncate(MAX_RECENT);
    }

    /// Renders the strip (nothing at all if empty, rather than an
    /// awkward-looking empty "Recent:" row before anything's been picked
    /// yet). Returns the clicked swatch's color, if any - the caller
    /// decides what that means for its own data.
    pub fn show(&self, ui: &mut egui::Ui) -> Option<[u8; 3]> {
        if self.colors.is_empty() {
            return None;
        }
        let mut picked = None;
        ui.horizontal(|ui| {
            ui.label("Recent:");
            for &[r, g, b] in &self.colors {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
                ui.painter()
                    .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
                ui.painter().rect_stroke(
                    rect,
                    2.0,
                    egui::Stroke::new(1.0_f32, Color32::from_gray(120)),
                );
                if response.clicked() {
                    picked = Some([r, g, b]);
                }
                response.on_hover_text(format!("#{r:02x}{g:02x}{b:02x} - click to reuse"));
            }
        });
        picked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_most_recent_first_and_deduplicates() {
        let mut recent = RecentColors::default();
        recent.record([255, 0, 0]);
        recent.record([0, 255, 0]);
        recent.record([255, 0, 0]); // re-recording moves it back to front
        assert_eq!(recent.colors, vec![[255, 0, 0], [0, 255, 0]]);
    }

    #[test]
    fn evicts_oldest_past_the_cap() {
        let mut recent = RecentColors::default();
        for i in 0..(MAX_RECENT + 3) {
            recent.record([i as u8, 0, 0]);
        }
        assert_eq!(recent.colors.len(), MAX_RECENT);
        // Most recently added should still be at the front.
        assert_eq!(recent.colors[0], [(MAX_RECENT + 2) as u8, 0, 0]);
    }
}
