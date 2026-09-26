//! "New pattern" picker: one card per craft, shown on launch and from the
//! toolbar's "New..." button. The craft chosen here isn't an app-wide
//! mode - it's just what kind of pattern gets created; after that the
//! pattern itself carries its craft (see `abyssal_thread_core::Craft`).

use abyssal_thread_core::Craft;
use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Blank,
    Picture,
    Text,
    OpenFile,
}

/// Returns the chosen craft and starting point, if one was clicked this
/// frame. `open` is cleared when the window is closed (or a choice made).
pub fn show(ctx: &egui::Context, open: &mut bool) -> Option<(Craft, Start)> {
    let mut choice = None;
    let mut still_open = *open;
    egui::Window::new("New pattern")
        .collapsible(false)
        .resizable(false)
        .open(&mut still_open)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label("What are you making?");
            ui.add_space(6.0);
            egui::Grid::new("craft_cards")
                .num_columns(2)
                .spacing([10.0, 10.0])
                .show(ui, |ui| {
                    for (i, craft) in Craft::ALL.into_iter().enumerate() {
                        card(ui, craft, &mut choice);
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
        });
    if choice.is_some() {
        still_open = false;
    }
    *open = still_open;
    choice
}

fn card(ui: &mut egui::Ui, craft: Craft, choice: &mut Option<(Craft, Start)>) {
    let available = craft.is_available();
    egui::Frame::group(ui.style())
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(300.0);
            ui.set_min_height(78.0);
            ui.add_enabled_ui(available, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(craft.label());
                    if !available {
                        ui.weak("- coming soon");
                    }
                });
                ui.small(craft.blurb());
                if available {
                    ui.horizontal(|ui| {
                        for (label, start) in [
                            ("Blank", Start::Blank),
                            ("From picture", Start::Picture),
                            ("From text", Start::Text),
                            ("Open file...", Start::OpenFile),
                        ] {
                            if ui.small_button(label).clicked() {
                                *choice = Some((craft, start));
                            }
                        }
                    });
                }
            });
        });
}
