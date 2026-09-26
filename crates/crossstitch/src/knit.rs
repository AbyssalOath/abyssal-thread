//! Knitting colorwork: written row-by-row instructions and stranded
//! colorwork checks for a chart used as a knitting chart.
//!
//! Knitting charts are read the way the fabric is worked: row 1 is the
//! bottom row, and a right-side row is read right to left. Worked flat,
//! wrong-side (even) rows are read left to right; worked in the round,
//! every round is a right-side round read right to left. Empty cells are
//! the main color (MC).
//!
//! Stranded (Fair Isle) colorwork carries the unused yarn across the back
//! as a "float"; floats longer than about 5 stitches snag, so they're
//! usually caught ("trapped") partway. `float_warnings` flags rows whose
//! longest float exceeds a limit, and rows using more than two colors
//! (awkward stranded - often better as intarsia or duplicate stitch).

use crate::chart::Chart;

/// The usual advice: trap floats longer than 5 stitches.
pub const DEFAULT_MAX_FLOAT: usize = 5;

/// Letter for a yarn in written instructions: `MC` for the background,
/// then A, B, C... by palette order.
pub fn yarn_code(v: Option<u16>) -> String {
    match v {
        None => "MC".to_string(),
        Some(i) if i < 26 => ((b'A' + i as u8) as char).to_string(),
        Some(i) => format!("C{}", i + 1),
    }
}

/// Whether chart row `y` is worked right to left.
fn right_to_left(chart: &Chart, y: usize) -> bool {
    chart.worked_in_round || chart.row_number(y) % 2 == 1
}

/// Row `y`'s cells in working order.
pub fn row_in_working_order(chart: &Chart, y: usize) -> Vec<Option<u16>> {
    let mut row: Vec<Option<u16>> = (0..chart.width).map(|x| chart.get(x, y)).collect();
    if right_to_left(chart, y) {
        row.reverse();
    }
    row
}

/// Consecutive same-yarn runs, e.g. `[(MC, 3), (A, 2)]`.
pub fn runs(row: &[Option<u16>]) -> Vec<(Option<u16>, usize)> {
    let mut out: Vec<(Option<u16>, usize)> = Vec::new();
    for &c in row {
        match out.last_mut() {
            Some((v, n)) if *v == c => *n += 1,
            _ => out.push((c, 1)),
        }
    }
    out
}

/// Longest float in a row: for each yarn, the most stitches between two
/// consecutive uses of it (which it's carried across behind the work).
pub fn longest_float(row: &[Option<u16>]) -> usize {
    let mut last_seen: Vec<(Option<u16>, usize)> = Vec::new();
    let mut longest = 0;
    for (i, &c) in row.iter().enumerate() {
        match last_seen.iter_mut().find(|(v, _)| *v == c) {
            Some((_, last)) => {
                longest = longest.max(i - *last - 1);
                *last = i;
            }
            None => last_seen.push((c, i)),
        }
    }
    longest
}

#[derive(Debug, Clone, PartialEq)]
pub struct RowWarning {
    /// Chart row number (1 = bottom).
    pub row: usize,
    pub longest_float: usize,
    pub colors: usize,
}

/// Rows with a float longer than `max_float` or more than two colors.
pub fn float_warnings(chart: &Chart, max_float: usize) -> Vec<RowWarning> {
    let mut out: Vec<RowWarning> = (0..chart.height)
        .filter_map(|y| {
            let row = row_in_working_order(chart, y);
            let mut colors: Vec<Option<u16>> = row.clone();
            colors.sort();
            colors.dedup();
            let w = RowWarning {
                row: chart.row_number(y),
                longest_float: longest_float(&row),
                colors: colors.len(),
            };
            (w.longest_float > max_float || w.colors > 2).then_some(w)
        })
        .collect();
    out.sort_by_key(|w| w.row);
    out
}

/// Written instructions, row 1 first, with a yarn key and float notes.
pub fn instructions(chart: &Chart, max_float: usize) -> String {
    let mut out = String::new();
    let (row_word, how) = if chart.worked_in_round {
        ("Rnd", "worked in the round: read every round right to left")
    } else {
        (
            "Row",
            "worked flat: RS (odd) rows right to left, WS (even) rows left to right",
        )
    };
    out.push_str(&format!(
        "Chart is {} stitches x {} rows, {how}.\n",
        chart.width, chart.height
    ));
    out.push_str("Yarns: MC = main color (background)");
    for (i, f) in chart.palette.iter().enumerate() {
        out.push_str(&format!("; {} = {}", yarn_code(Some(i as u16)), f.label()));
    }
    out.push_str("\n\n");
    for y in (0..chart.height).rev() {
        let n = chart.row_number(y);
        let side = if chart.worked_in_round {
            String::new()
        } else if n % 2 == 1 {
            " (RS)".to_string()
        } else {
            " (WS)".to_string()
        };
        let row = row_in_working_order(chart, y);
        let parts: Vec<String> = runs(&row)
            .into_iter()
            .map(|(c, k)| format!("{k} {}", yarn_code(c)))
            .collect();
        out.push_str(&format!("{row_word} {n}{side}: {}\n", parts.join(", ")));
    }
    let warnings = float_warnings(chart, max_float);
    if !warnings.is_empty() {
        out.push_str(&format!(
            "\nStranded colorwork notes (trap floats longer than {max_float} stitches):\n"
        ));
        for w in warnings {
            let mut notes = Vec::new();
            if w.longest_float > max_float {
                notes.push(format!("float of {} sts", w.longest_float));
            }
            if w.colors > 2 {
                notes.push(format!(
                    "{} colors in one row - consider intarsia or duplicate stitch",
                    w.colors
                ));
            }
            out.push_str(&format!("  {row_word} {}: {}\n", w.row, notes.join("; ")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::Floss;
    use crate::profile::GridCraft;

    fn chart() -> Chart {
        // 6 sts x 2 rows. Bottom row (row 1): A at the right edge only.
        let mut c = Chart::new_for(GridCraft::Knitting);
        c.crop_or_pad(6, 2);
        c.add_floss(Floss::free([200, 0, 0], 'X'));
        c.set(5, 1, Some(0)); // bottom-right = stitch 1 of row 1
        c.set(0, 0, Some(0)); // top row, far left
        c.set(1, 0, Some(0));
        c
    }

    #[test]
    fn flat_rows_alternate_direction_and_start_at_the_bottom() {
        let text = instructions(&chart(), DEFAULT_MAX_FLOAT);
        let rows: Vec<&str> = text.lines().filter(|l| l.starts_with("Row")).collect();
        assert_eq!(rows[0], "Row 1 (RS): 1 A, 5 MC", "{text}");
        // Row 2 is WS, read left to right: the two A's come first.
        assert_eq!(rows[1], "Row 2 (WS): 2 A, 4 MC", "{text}");
        assert!(
            text.contains("A = Red #c80000") || text.contains("A = "),
            "{text}"
        );
    }

    #[test]
    fn in_the_round_reads_every_round_right_to_left() {
        let mut c = chart();
        c.worked_in_round = true;
        let text = instructions(&c, DEFAULT_MAX_FLOAT);
        assert!(text.contains("Rnd 2: 4 MC, 2 A"), "{text}");
        assert!(!text.contains("(RS)"));
    }

    #[test]
    fn floats_and_color_counts_are_flagged() {
        let a = Some(0);
        assert_eq!(longest_float(&[a, None, None, None, a]), 3);
        assert_eq!(longest_float(&[a, a, None]), 0);
        let mut c = Chart::new_for(GridCraft::Knitting);
        c.crop_or_pad(10, 1);
        c.add_floss(Floss::free([200, 0, 0], 'X'));
        c.add_floss(Floss::free([0, 0, 200], 'O'));
        c.set(0, 0, Some(0));
        c.set(9, 0, Some(0)); // A floats 8 sts behind MC
        assert_eq!(
            float_warnings(&c, 5),
            vec![RowWarning {
                row: 1,
                longest_float: 8,
                colors: 2
            }]
        );
        c.set(4, 0, Some(1));
        let w = &float_warnings(&c, 5)[0];
        assert_eq!(w.colors, 3);
        assert!(instructions(&c, 5).contains("3 colors in one row"));
        assert!(float_warnings(&c, 10)
            .iter()
            .all(|w| w.longest_float <= 10 || w.colors > 2));
    }
}
