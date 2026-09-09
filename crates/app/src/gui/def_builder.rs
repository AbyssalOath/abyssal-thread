//! Point-and-click builder for `DEF: name = ops...` custom-stitch aliases -
//! covers the alias form only (a named, reusable sequence of ordinary DSL
//! ops). The raw-geometry form (`%`/bracket relative-attachment syntax,
//! see `raw_def.rs`) still has no visual builder - defining genuinely
//! novel stitch geometry through point-and-click would need a node/edge
//! graph editor, not a list of dropdowns, and is out of scope here.

use eframe::egui;

const KNOWN_ABBREVS: &[&str] = &["sc", "hdc", "dc", "tr", "ch", "ss", "inc", "dec"];

pub struct DefBuilderState {
    name: String,
    ops: Vec<(u32, String)>,
    new_count: u32,
    new_abbrev_idx: usize,
}

impl Default for DefBuilderState {
    fn default() -> Self {
        Self { name: String::new(), ops: Vec::new(), new_count: 1, new_abbrev_idx: 0 }
    }
}

/// Returns `Some(def_line)` (e.g. `"DEF: shell = 3dc, ch1\n"`) when
/// "Insert into DSL" was clicked this frame.
pub fn show(ui: &mut egui::Ui, state: &mut DefBuilderState) -> Option<String> {
    let mut result = None;

    ui.horizontal(|ui| {
        ui.label("Alias name:");
        ui.text_edit_singleline(&mut state.name);
    });

    ui.label("Stitches in this alias, in order:");
    let mut remove_idx = None;
    for (i, (count, abbrev)) in state.ops.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{i}: {count}{abbrev}"));
            if ui.button("Remove").clicked() {
                remove_idx = Some(i);
            }
        });
    }
    if let Some(i) = remove_idx {
        state.ops.remove(i);
    }

    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut state.new_count).clamp_range(1..=99));
        egui::ComboBox::from_id_source("def_builder_abbrev")
            .selected_text(KNOWN_ABBREVS[state.new_abbrev_idx])
            .show_ui(ui, |ui| {
                for (i, abbrev) in KNOWN_ABBREVS.iter().enumerate() {
                    ui.selectable_value(&mut state.new_abbrev_idx, i, *abbrev);
                }
            });
        if ui.button("+ Add stitch").clicked() {
            state.ops.push((state.new_count, KNOWN_ABBREVS[state.new_abbrev_idx].to_string()));
        }
    });

    let name_valid = !state.name.trim().is_empty() && state.name.chars().all(|c| c.is_alphanumeric() || c == '_');
    if !name_valid && !state.name.is_empty() {
        ui.colored_label(egui::Color32::from_rgb(220, 90, 90), "Name must be letters/digits/underscore only.");
    }

    let can_insert = name_valid && !state.ops.is_empty();
    if ui.add_enabled(can_insert, egui::Button::new("Insert DEF into DSL")).clicked() {
        let body: Vec<String> = state.ops.iter().map(|(c, a)| format!("{c}{a}")).collect();
        result = Some(format!("DEF: {} = {}\n", state.name.trim(), body.join(", ")));
        state.name.clear();
        state.ops.clear();
    }

    result
}
