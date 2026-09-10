//! Point-and-click builder for `DEF: name = ops...` custom stitches -
//! covers both supported forms:
//!
//! - **Alias** - a named, reusable sequence of ordinary DSL ops (`DEF:
//!   shell = 3dc, ch1`).
//! - **Raw geometry** - `%`/`%-N`/`%+N`/`@N` relative-attachment syntax
//!   for genuinely novel stitch geometry like picots and closed loops (see
//!   `raw_def.rs`'s module doc for the full grammar). Rather than a
//!   node/edge graph editor, this is a form: place stitches in order, then
//!   optionally mark one as "relative" and pick its target position(s)
//!   from a running list of already-placed positions instead of typing
//!   `%-N` by hand. The assembled body is validated live against the real
//!   `raw_def::parse_raw_def` parser before "Insert" is enabled, so
//!   whatever gets inserted is guaranteed to parse back in.

use eframe::egui;

use abyssal_thread_lang::raw_def::{self, RawAttachRef, RawOp};

const KNOWN_ABBREVS: &[&str] = &["sc", "hdc", "dc", "tr", "ch", "ss", "inc", "dec"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuilderMode {
    Alias,
    RawGeometry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefKind {
    SelfRef,
    Back,
    Forward,
}

pub struct DefBuilderState {
    name: String,
    mode: BuilderMode,

    // Alias mode.
    ops: Vec<(u32, String)>,
    new_count: u32,
    new_abbrev_idx: usize,

    // Raw-geometry mode.
    raw_ops: Vec<RawOp>,
    raw_abbrev_idx: usize,
    raw_is_relative: bool,
    raw_back_enabled: bool,
    raw_back: u32,
    pending_refs: Vec<RawAttachRef>,
    new_ref_kind: RefKind,
    new_ref_n: u32,
}

impl Default for DefBuilderState {
    fn default() -> Self {
        Self {
            name: String::new(),
            mode: BuilderMode::Alias,
            ops: Vec::new(),
            new_count: 1,
            new_abbrev_idx: 0,
            raw_ops: Vec::new(),
            raw_abbrev_idx: 0,
            raw_is_relative: false,
            raw_back_enabled: false,
            raw_back: 1,
            pending_refs: Vec::new(),
            new_ref_kind: RefKind::SelfRef,
            new_ref_n: 1,
        }
    }
}

/// Whether `name` is legal as a `DEF:` name - matches the lexer's `Ident`
/// grammar (letters/digits/underscore only, and non-empty), so a name
/// accepted here is guaranteed not to itself break re-parsing the emitted
/// `DEF:` line. Pulled out as its own function (rather than left inline in
/// `show`) specifically so it's testable without a live `egui::Ui`.
fn is_valid_def_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty() && trimmed.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Formats a `DEF:` line from a name and ordered `(count, abbrev)` pairs,
/// e.g. `format_def_line("shell", &[(3, "dc".into()), (1, "ch".into())])`
/// -> `"DEF: shell = 3dc, 1ch\n"`.
fn format_def_line(name: &str, ops: &[(u32, String)]) -> String {
    let body: Vec<String> = ops.iter().map(|(c, a)| format!("{c}{a}")).collect();
    format!("DEF: {} = {}\n", name.trim(), body.join(", "))
}

/// Formats one raw-geometry term, e.g. `RawOp::Stitch{abbrev:"ch",count:3}`
/// -> `"3ch"`, or a relative stitch with refs -> `"ss@1[%,%-4]"`.
fn format_raw_term(op: &RawOp) -> String {
    match op {
        RawOp::Stitch { abbrev, count } => {
            if *count == 1 {
                abbrev.clone()
            } else {
                format!("{count}{abbrev}")
            }
        }
        RawOp::RelativeStitch { abbrev, back, refs } => {
            let back_str = back.map(|n| n.to_string()).unwrap_or_default();
            let mut term = format!("{abbrev}@{back_str}");
            if !refs.is_empty() {
                let inner: Vec<String> = refs.iter().map(format_raw_ref).collect();
                term.push('[');
                term.push_str(&inner.join(","));
                term.push(']');
            }
            term
        }
    }
}

fn format_raw_ref(r: &RawAttachRef) -> String {
    match r {
        RawAttachRef::SelfRef => "%".to_string(),
        RawAttachRef::Back(n) => format!("%-{n}"),
        RawAttachRef::Forward(n) => format!("%+{n}"),
    }
}

/// Joins raw-geometry ops into the comma-separated body text that goes
/// after `=` in a `DEF:` line, e.g. `"3ch, ss@1[%,%-4]"`.
fn format_raw_body(ops: &[RawOp]) -> String {
    ops.iter()
        .map(format_raw_term)
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_raw_def_line(name: &str, ops: &[RawOp]) -> String {
    format!("DEF: {} = {}\n", name.trim(), format_raw_body(ops))
}

/// How many cursor positions `op` contributes once placed - a `3ch`
/// contributes 3 positions (matching `raw_def.rs`'s counting rule), a
/// relative stitch always contributes exactly 1 (raw bodies don't support
/// a count prefix on that form).
fn raw_op_position_count(op: &RawOp) -> u32 {
    match op {
        RawOp::Stitch { count, .. } => *count,
        RawOp::RelativeStitch { .. } => 1,
    }
}

/// Returns `Some(def_line)` (e.g. `"DEF: shell = 3dc, ch1\n"`) when
/// "Insert into DSL" was clicked this frame.
pub fn show(ui: &mut egui::Ui, state: &mut DefBuilderState) -> Option<String> {
    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.text_edit_singleline(&mut state.name);
    });

    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.mode, BuilderMode::Alias, "Alias (repeat ops)");
        ui.selectable_value(
            &mut state.mode,
            BuilderMode::RawGeometry,
            "Raw geometry (picots, closed loops)",
        );
    });

    let name_valid = is_valid_def_name(&state.name);
    if !name_valid && !state.name.is_empty() {
        ui.colored_label(
            egui::Color32::from_rgb(220, 90, 90),
            "Name must be letters/digits/underscore only.",
        );
    }

    match state.mode {
        BuilderMode::Alias => show_alias(ui, state, name_valid),
        BuilderMode::RawGeometry => show_raw(ui, state, name_valid),
    }
}

fn show_alias(ui: &mut egui::Ui, state: &mut DefBuilderState, name_valid: bool) -> Option<String> {
    let mut result = None;

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
        ui.add(egui::DragValue::new(&mut state.new_count).range(1..=99));
        egui::ComboBox::from_id_salt("def_builder_abbrev")
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

fn show_raw(ui: &mut egui::Ui, state: &mut DefBuilderState, name_valid: bool) -> Option<String> {
    let mut result = None;

    ui.label(
        "Place stitches in order, then optionally mark one as a relative/\
         closing stitch and send its attachment back to an earlier \
         position instead of the normal next-in-line parent - this is how \
         picots and other closed loops are built.",
    );

    let mut remove_idx = None;
    let mut cursor = 0u32;
    for (i, op) in state.raw_ops.iter().enumerate() {
        let count = raw_op_position_count(op);
        let range = if count == 1 {
            format!("{cursor}")
        } else {
            format!("{cursor}-{}", cursor + count - 1)
        };
        ui.horizontal(|ui| {
            ui.label(format!("[{range}] {}", format_raw_term(op)));
            if ui.button("Remove").clicked() {
                remove_idx = Some(i);
            }
        });
        cursor += count;
    }
    if let Some(i) = remove_idx {
        state.raw_ops.remove(i);
    }
    ui.label(format!("Cursor is now at position {cursor}."));

    ui.separator();
    ui.checkbox(
        &mut state.raw_is_relative,
        "This stitch is relative/closing (uses %)",
    );

    ui.horizontal(|ui| {
        if !state.raw_is_relative {
            ui.add(egui::DragValue::new(&mut state.new_count).range(1..=99));
        }
        egui::ComboBox::from_id_salt("def_builder_raw_abbrev")
            .selected_text(KNOWN_ABBREVS[state.raw_abbrev_idx])
            .show_ui(ui, |ui| {
                for (i, abbrev) in KNOWN_ABBREVS.iter().enumerate() {
                    ui.selectable_value(&mut state.raw_abbrev_idx, i, *abbrev);
                }
            });
    });

    if state.raw_is_relative {
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut state.raw_back_enabled,
                "Override primary parent - N positions before cursor:",
            );
            ui.add_enabled(
                state.raw_back_enabled,
                egui::DragValue::new(&mut state.raw_back).range(0..=cursor.max(1)),
            );
        });

        ui.label("Extra closing edges (attach into more than one earlier point):");
        let mut remove_ref_idx = None;
        for (i, r) in state.pending_refs.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format_raw_ref(r));
                if ui.button("Remove").clicked() {
                    remove_ref_idx = Some(i);
                }
            });
        }
        if let Some(i) = remove_ref_idx {
            state.pending_refs.remove(i);
        }

        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("def_builder_ref_kind")
                .selected_text(match state.new_ref_kind {
                    RefKind::SelfRef => "self (%)",
                    RefKind::Back => "N before (%-N)",
                    RefKind::Forward => "N after (%+N)",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.new_ref_kind, RefKind::SelfRef, "self (%)");
                    ui.selectable_value(&mut state.new_ref_kind, RefKind::Back, "N before (%-N)");
                    ui.selectable_value(&mut state.new_ref_kind, RefKind::Forward, "N after (%+N)");
                });
            if state.new_ref_kind != RefKind::SelfRef {
                ui.add(egui::DragValue::new(&mut state.new_ref_n).range(1..=99));
            }
            if ui.button("+ Add reference").clicked() {
                let r = match state.new_ref_kind {
                    RefKind::SelfRef => RawAttachRef::SelfRef,
                    RefKind::Back => RawAttachRef::Back(state.new_ref_n),
                    RefKind::Forward => RawAttachRef::Forward(state.new_ref_n),
                };
                state.pending_refs.push(r);
            }
        });

        if ui.button("+ Add relative stitch").clicked() {
            state.raw_ops.push(RawOp::RelativeStitch {
                abbrev: KNOWN_ABBREVS[state.raw_abbrev_idx].to_string(),
                back: if state.raw_back_enabled {
                    Some(state.raw_back)
                } else {
                    None
                },
                refs: std::mem::take(&mut state.pending_refs),
            });
            state.raw_back_enabled = false;
            state.raw_back = 1;
        }
    } else if ui.button("+ Add stitch").clicked() {
        state.raw_ops.push(RawOp::Stitch {
            abbrev: KNOWN_ABBREVS[state.raw_abbrev_idx].to_string(),
            count: state.new_count,
        });
    }

    // Live-validate the assembled body against the real raw-geometry
    // parser rather than trusting the UI logic in isolation, and require
    // it to actually contain a '%' - a relative stitch with an overridden
    // parent but no extra `[...]` refs never emits one, which would get
    // silently misclassified as a plain alias body instead of raw
    // geometry by `raw_def::looks_like_raw_body` at parse time. See
    // raw_def.rs's module doc.
    let raw_body = format_raw_body(&state.raw_ops);
    let round_trips = raw_def::parse_raw_def(&raw_body).is_ok();
    let is_raw = raw_def::looks_like_raw_body(&raw_body);

    if !state.raw_ops.is_empty() && !is_raw {
        ui.colored_label(
            egui::Color32::from_rgb(220, 90, 90),
            "This body has no '%' in it, so it would be parsed as a plain \
             alias instead of raw geometry - add at least one closing edge \
             to a relative stitch (or use the Alias tab instead).",
        );
    }

    let can_insert = name_valid && !state.raw_ops.is_empty() && round_trips && is_raw;
    if ui
        .add_enabled(can_insert, egui::Button::new("Insert DEF into DSL"))
        .clicked()
    {
        result = Some(format_raw_def_line(&state.name, &state.raw_ops));
        state.name.clear();
        state.raw_ops.clear();
        state.pending_refs.clear();
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
    fn the_emitted_alias_def_line_round_trips_through_the_real_parser() {
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

    #[test]
    fn formats_a_plain_stitch_term_without_a_redundant_count_prefix() {
        assert_eq!(
            format_raw_term(&RawOp::Stitch {
                abbrev: "ch".into(),
                count: 1
            }),
            "ch"
        );
        assert_eq!(
            format_raw_term(&RawOp::Stitch {
                abbrev: "ch".into(),
                count: 3
            }),
            "3ch"
        );
    }

    #[test]
    fn formats_a_relative_stitch_with_no_back_and_no_refs() {
        assert_eq!(
            format_raw_term(&RawOp::RelativeStitch {
                abbrev: "ss".into(),
                back: None,
                refs: vec![],
            }),
            "ss@"
        );
    }

    #[test]
    fn formats_a_picot_raw_body() {
        let ops = vec![
            RawOp::Stitch {
                abbrev: "ch".into(),
                count: 3,
            },
            RawOp::RelativeStitch {
                abbrev: "ss".into(),
                back: Some(1),
                refs: vec![RawAttachRef::SelfRef, RawAttachRef::Back(4)],
            },
        ];
        assert_eq!(format_raw_body(&ops), "3ch, ss@1[%,%-4]");
    }

    #[test]
    fn raw_op_position_count_counts_stitch_repeats_but_not_relative_stitches() {
        assert_eq!(
            raw_op_position_count(&RawOp::Stitch {
                abbrev: "ch".into(),
                count: 3
            }),
            3
        );
        assert_eq!(
            raw_op_position_count(&RawOp::RelativeStitch {
                abbrev: "ss".into(),
                back: None,
                refs: vec![],
            }),
            1
        );
    }

    #[test]
    fn a_picot_body_round_trips_through_the_real_raw_geometry_parser() {
        let ops = vec![
            RawOp::Stitch {
                abbrev: "ch".into(),
                count: 3,
            },
            RawOp::RelativeStitch {
                abbrev: "ss".into(),
                back: Some(1),
                refs: vec![RawAttachRef::SelfRef, RawAttachRef::Back(4)],
            },
        ];
        let body = format_raw_body(&ops);
        assert_eq!(raw_def::parse_raw_def(&body).unwrap(), ops);
        assert!(raw_def::looks_like_raw_body(&body));
    }

    #[test]
    fn a_relative_stitch_with_no_refs_is_not_classified_as_raw() {
        // Mirrors the UI's own live-validation check: an overridden parent
        // with no extra closing edges never emits a '%', so the assembled
        // body would be silently misparsed as an alias rather than raw
        // geometry if inserted.
        let ops = vec![RawOp::RelativeStitch {
            abbrev: "ss".into(),
            back: Some(1),
            refs: vec![],
        }];
        let body = format_raw_body(&ops);
        assert!(!raw_def::looks_like_raw_body(&body));
    }

    #[test]
    fn the_emitted_raw_def_line_round_trips_through_the_real_parser() {
        let ops = vec![
            RawOp::Stitch {
                abbrev: "ch".into(),
                count: 3,
            },
            RawOp::RelativeStitch {
                abbrev: "ss".into(),
                back: Some(1),
                refs: vec![RawAttachRef::SelfRef, RawAttachRef::Back(4)],
            },
        ];
        let line = format_raw_def_line("picot", &ops);
        let src = format!("{line}6sc\n");
        let pattern =
            abyssal_thread_lang::parser::parse(&src).expect("builder output should parse");
        assert_eq!(
            pattern.definitions,
            vec![("picot".to_string(), "3ch, ss@1[%,%-4]".to_string())]
        );
    }
}
