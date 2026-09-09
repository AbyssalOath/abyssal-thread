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
        Self {
            name: String::new(),
            ops: Vec::new(),
            new_count: 1,
            new_abbrev_idx: 0,
        }
    }
}

/// Whether `name` is legal as a `DEF:` alias name - matches the lexer's
/// `Ident` grammar (letters/digits/underscore only, and non-empty), so a
/// name accepted here is guaranteed not to itself break re-parsing the
/// emitted `DEF:` line. Pulled out as its own function (rather than left
/// inline in `show`) specifically so it's testable without a live
/// `egui::Ui` - `show` calls this and nothing else for validation.
fn is_valid_def_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty() && trimmed.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Formats a `DEF:` line from a name and ordered `(count, abbrev)` pairs,
/// e.g. `format_def_line("shell", &[(3, "dc".into()), (1, "ch".into())])`
/// -> `"DEF: shell = 3dc, 1ch\n"`. Same reasoning as `is_valid_def_name`:
/// pulled out so the actual text-assembly logic is testable directly,
/// rather than only indirectly through a full `show()` call.
fn format_def_line(name: &str, ops: &[(u32, String)]) -> String {
    let body: Vec<String> = ops.iter().map(|(c, a)| format!("{c}{a}")).collect();
    format!("DEF: {} = {}\n", name.trim(), body.join(", "))
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
            state.ops.push((
                state.new_count,
                KNOWN_ABBREVS[state.new_abbrev_idx].to_string(),
            ));
        }
    });

    let name_valid = is_valid_def_name(&state.name);
    if !name_valid && !state.name.is_empty() {
        ui.colored_label(
            egui::Color32::from_rgb(220, 90, 90),
            "Name must be letters/digits/underscore only.",
        );
    }

    let can_insert = name_valid && !state.ops.is_empty();
    if ui
        .add_enabled(can_insert, egui::Button::new("Insert DEF into DSL"))
        .clicked()
    {
        result = Some(format_def_line(&state.name, &state.ops));
        state.name.clear();
        state.ops.clear();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_letters_digits_and_underscore() {
        assert!(is_valid_def_name("shell"));
        assert!(is_valid_def_name("shell_2"));
        assert!(is_valid_def_name("Shell2"));
        assert!(is_valid_def_name("  shell  ")); // leading/trailing whitespace trimmed first
    }

    #[test]
    fn rejects_empty_or_whitespace_only_names() {
        assert!(!is_valid_def_name(""));
        assert!(!is_valid_def_name("   "));
    }

    #[test]
    fn rejects_names_with_illegal_characters() {
        // Anything that isn't alphanumeric/underscore would either break
        // re-parsing the emitted DEF line or collide with other DSL syntax
        // (spaces, `.`, `~`, `-`, etc.).
        assert!(!is_valid_def_name("shell stitch"));
        assert!(!is_valid_def_name("shell.2"));
        assert!(!is_valid_def_name("shell-2"));
        assert!(!is_valid_def_name("shell!"));
    }

    #[test]
    fn formats_a_multi_stitch_def_line() {
        let ops = vec![(3u32, "dc".to_string()), (1u32, "ch".to_string())];
        assert_eq!(format_def_line("shell", &ops), "DEF: shell = 3dc, 1ch\n");
    }

    #[test]
    fn formats_a_single_stitch_def_line() {
        let ops = vec![(1u32, "sc".to_string())];
        assert_eq!(format_def_line("simple", &ops), "DEF: simple = 1sc\n");
    }

    #[test]
    fn trims_the_name_but_not_internal_content() {
        let ops = vec![(1u32, "sc".to_string())];
        assert_eq!(format_def_line("  padded  ", &ops), "DEF: padded = 1sc\n");
    }

    #[test]
    fn the_emitted_def_line_round_trips_through_the_real_parser() {
        // The actual point of both helpers existing: whatever the builder
        // produces has to be valid DSL the rest of the app can parse back
        // in - this is the integration check that the two pure functions
        // above are exercised in isolation for.
        let ops = vec![(3u32, "dc".to_string()), (1u32, "ch".to_string())];
        let line = format_def_line("shell", &ops);
        let src = format!("{line}6sc\n");
        let pattern =
            abyssal_thread_lang::parser::parse(&src).expect("builder output should parse");
        assert_eq!(
            pattern.definitions,
            vec![("shell".to_string(), "3dc, 1ch".to_string())]
        );
    }
}
