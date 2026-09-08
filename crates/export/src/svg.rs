//! Generates a schematic 2D crochet chart as SVG - one symbol per stitch,
//! arranged by round/index rather than the literal 3D layout (charts are
//! meant to be read, not to be geometrically accurate). Symbols follow the
//! standard US crochet-chart conventions: an oval for chain, a filled dot
//! for slip stitch, a "+" for single crochet, and a "T" (vertical stem +
//! top cap) with zero/one/two diagonal "yarn over" tick marks down the stem
//! for half double/double/treble crochet - not just three different
//! heights of the same shape. Loop-only and post modifiers are drawn as a
//! small mark layered on top of the base stitch's own symbol (see
//! `StitchKind::modifier_symbol`), matching how real charts layer these
//! rather than replacing the base symbol outright. Increase/decrease are
//! drawn as two real copies of the base stitch's own symbol
//! branching/merging at a shared point (`branch_glyph`), not a generic
//! V/inverted-V that hides which stitch is actually being doubled/merged.
//!
//! Front/back-post's left/right curl direction (see `glyph`'s doc comment)
//! is this exporter's own convention, not a CGOA/industry standard - a
//! one-line caption is appended to the chart whenever a post stitch
//! actually appears in it, so the chart doesn't get mistaken for following
//! an established symbol set on that specific point.

use abyssal_thread_core::StitchGraph;

const CELL: f32 = 24.0;
const RADIUS: f32 = 8.0;

pub fn export_svg_chart(g: &StitchGraph) -> String {
    let max_round_len = g.rounds.iter().map(|r| r.len()).max().unwrap_or(0);
    let width = (max_round_len as f32 + 2.0) * CELL;
    let has_post_stitch = g
        .graph
        .node_weights()
        .any(|n| matches!(n.kind.modifier_symbol(), Some("fpost") | Some("bpost")));
    // One extra row of height for the post-stitch legend caption, only
    // when it's actually needed - an unrelated chart shouldn't grow a
    // blank strip at the bottom for a convention it doesn't use.
    let caption_rows = if has_post_stitch { 1.0 } else { 0.0 };
    let height = (g.rounds.len() as f32 + 2.0 + caption_rows) * CELL;

    let mut body = String::new();

    for (round_idx, round) in g.rounds.iter().enumerate() {
        // Rows are drawn bottom-to-top, matching how rounds are worked
        // upward, and offset down by `caption_rows` so the caption (if
        // any) has its own clear space below round 0 instead of crowding it.
        let y = height - (round_idx as f32 + 1.5 + caption_rows) * CELL;
        let mut i = 0;
        while i < round.len() {
            let node_idx = round[i];
            let node = &g.graph[node_idx];
            let x = (i as f32 + 1.0) * CELL;

            // An increase produces *two* separate graph nodes (both
            // `is_increase() == true`, sharing one parent) - unlike a
            // decrease, which is already a single node with two parent
            // edges. Pair them up here (mirroring `gui::grid`'s
            // `build_cells`) so one increase gets exactly one branch glyph
            // spanning both grid columns, not two glyphs doubled on top of
            // each other.
            if node.kind.is_increase() && i + 1 < round.len() {
                let next_idx = round[i + 1];
                let next = &g.graph[next_idx];
                if next.kind.is_increase() && g.parent_of(node_idx) == g.parent_of(next_idx) {
                    let mid_x = x + CELL / 2.0;
                    let symbol = node.kind.chart_symbol();
                    let label = node.label.as_deref().or(next.label.as_deref());
                    body.push_str(&branch_glyph(symbol, true, CELL * 0.35, mid_x, y, label));
                    i += 2;
                    continue;
                }
            }

            let symbol = node.kind.chart_symbol();
            if node.kind.is_decrease() {
                // Already a single node (two Parent edges) - one glyph, no
                // pairing needed.
                body.push_str(&branch_glyph(symbol, false, RADIUS * 0.7, x, y, node.label.as_deref()));
            } else {
                // Also catches a lone, unpaired `is_increase()` node (its
                // partner had a label, a different parent, or some other
                // mismatch that broke the pairing above) - falls back to a
                // single plain symbol rather than a broken/doubled glyph.
                let modifier = node.kind.modifier_symbol();
                body.push_str(&glyph(symbol, modifier, x, y, node.label.as_deref()));
            }
            i += 1;
        }
    }

    let caption = if has_post_stitch {
        format!(
            r#"<text x="{cx}" y="{cy}" text-anchor="middle" font-style="italic" font-size="9">Front/back-post curl direction (right/left) is this chart's own convention, not a CGOA standard.</text>"#,
            cx = width / 2.0,
            cy = height - CELL * 0.4,
        )
    } else {
        String::new()
    };

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" font-family="sans-serif" font-size="10">
<rect width="100%" height="100%" fill="white"/>
{body}{caption}</svg>"#
    )
}

/// Draws the base shape for any stitch that ISN'T an increase/decrease -
/// those go through `branch_glyph` instead, since they need two copies of
/// the shape rather than one. `modifier` layers on a loop-only/post mark;
/// front/back-post's right/left curl choice is `export_svg_chart`'s own
/// convention (see the module doc's caption), not a standard one.
fn glyph(symbol: &str, modifier: Option<&str>, x: f32, y: f32, label: Option<&str>) -> String {
    let shape = match symbol {
        "oval" => format!(
            r#"<ellipse cx="{x}" cy="{y}" rx="{r}" ry="{r2}" fill="none" stroke="black"/>"#,
            r = RADIUS,
            r2 = RADIUS * 0.6
        ),
        "dot" => format!(r#"<circle cx="{x}" cy="{y}" r="3" fill="black"/>"#),
        "plus" => format!(
            r#"<line x1="{x1}" y1="{y}" x2="{x2}" y2="{y}" stroke="black"/><line x1="{x}" y1="{y1}" x2="{x}" y2="{y2}" stroke="black"/>"#,
            x1 = x - RADIUS,
            x2 = x + RADIUS,
            y1 = y - RADIUS,
            y2 = y + RADIUS
        ),
        // Half double/double/treble crochet are the same "T" (vertical
        // stem + horizontal top cap) with zero, one, or two diagonal
        // "yarn over" tick marks crossing the stem - matching the real
        // convention of one tick per yarn-over-before-insertion, rather
        // than just three different stem heights of an otherwise-identical
        // shape.
        "t" | "t-small" | "t-tall" => {
            let (h, ticks) = match symbol {
                "t-small" => (RADIUS * 0.7, 0),
                "t" => (RADIUS * 1.1, 1),
                _ => (RADIUS * 1.5, 2), // "t-tall"
            };
            let mut s = format!(
                r#"<line x1="{x}" y1="{y1}" x2="{x}" y2="{y2}" stroke="black"/><line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y1}" stroke="black"/>"#,
                x1 = x - RADIUS * 0.5,
                x2 = x + RADIUS * 0.5,
                y1 = y - h,
                y2 = y + h
            );
            for tick in 0..ticks {
                let ty = (y - h) + (2.0 * h) * (0.3 + 0.25 * tick as f32);
                s.push_str(&format!(
                    r#"<line x1="{tx1}" y1="{ty1}" x2="{tx2}" y2="{ty2}" stroke="black"/>"#,
                    tx1 = x - RADIUS * 0.55,
                    ty1 = ty + RADIUS * 0.3,
                    tx2 = x + RADIUS * 0.55,
                    ty2 = ty - RADIUS * 0.3
                ));
            }
            s
        }
        _ => format!(r#"<rect x="{rx}" y="{ry}" width="{s}" height="{s}" fill="none" stroke="black"/>"#,
            rx = x - RADIUS * 0.7, ry = y - RADIUS * 0.7, s = RADIUS * 1.4),
    };

    // Loop-only/post modifiers are layered on as a small extra mark beside
    // the base shape drawn above, rather than replacing it outright - front
    // /back loop only get a tick below/above the base (which loop the hook
    // goes under), front/back post get a small hook curling to the right
    // /left (worked around the post from the front/back). The left/right
    // choice for post stitches is this exporter's own approximation for
    // telling them apart at a glance, not a universally standardized
    // placement.
    let modifier_mark = match modifier {
        Some("flo") => format!(
            r#"<line x1="{x1}" y1="{ly}" x2="{x2}" y2="{ly}" stroke="black" stroke-width="2"/>"#,
            x1 = x - RADIUS,
            x2 = x + RADIUS,
            ly = y + RADIUS * 1.15
        ),
        Some("blo") => format!(
            r#"<line x1="{x1}" y1="{ly}" x2="{x2}" y2="{ly}" stroke="black" stroke-width="2"/>"#,
            x1 = x - RADIUS,
            x2 = x + RADIUS,
            ly = y - RADIUS * 1.15
        ),
        Some("fpost") => format!(
            r#"<path d="M {mx} {my1} A {r} {r} 0 0 1 {mx} {my2}" fill="none" stroke="black"/>"#,
            mx = x + RADIUS * 1.2,
            my1 = y - RADIUS * 0.6,
            my2 = y + RADIUS * 0.6,
            r = RADIUS * 0.6
        ),
        Some("bpost") => format!(
            r#"<path d="M {mx} {my1} A {r} {r} 0 0 0 {mx} {my2}" fill="none" stroke="black"/>"#,
            mx = x - RADIUS * 1.2,
            my1 = y - RADIUS * 0.6,
            my2 = y + RADIUS * 0.6,
            r = RADIUS * 0.6
        ),
        _ => String::new(),
    };

    let label_text = label
        .map(|l| format!(r#"<text x="{x}" y="{y2}" text-anchor="middle">{l}</text>"#, y2 = y - RADIUS - 4.0))
        .unwrap_or_default();

    format!("{shape}{modifier_mark}{label_text}\n")
}

/// Draws an increase/decrease as two full copies of the base stitch's own
/// symbol (whatever `chart_symbol()` resolved to - "T" for dc, oval for
/// chain, etc.), side by side, connected by two lines converging on a
/// single shared point - matching the real convention that "2 stitches
/// worked into 1" is drawn as two real stitch symbols sharing a base, not
/// an opaque V standing in for whatever stitch is actually being
/// doubled/merged. `is_increase` decides which side gets the single point:
/// below the pair (one parent, branching up into two children) for an
/// increase, above the pair (two parents, merging down into one child) for
/// a decrease - mirroring how rounds are drawn bottom-to-top in
/// `export_svg_chart`. `offset` is how far apart the two copies sit -
/// wider for an increase (its pair of nodes spans two grid columns) than a
/// decrease (a single node/column reaching down to two parents).
fn branch_glyph(symbol: &str, is_increase: bool, offset: f32, x: f32, y: f32, label: Option<&str>) -> String {
    let (x1, x2) = (x - offset, x + offset);
    let shape1 = glyph(symbol, None, x1, y, None);
    let shape2 = glyph(symbol, None, x2, y, None);

    let single_y = if is_increase { y + RADIUS * 1.8 } else { y - RADIUS * 1.8 };
    let converge = format!(
        r#"<line x1="{x1}" y1="{y}" x2="{x}" y2="{single_y}" stroke="black"/><line x1="{x2}" y1="{y}" x2="{x}" y2="{single_y}" stroke="black"/>"#,
    );

    let label_text = label
        .map(|l| format!(r#"<text x="{x}" y="{y2}" text-anchor="middle">{l}</text>"#, y2 = y - RADIUS * 2.2))
        .unwrap_or_default();

    format!("{shape1}{shape2}{converge}{label_text}\n")
}
