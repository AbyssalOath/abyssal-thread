//! OXS ("Open Cross Stitch") import/export - the XML interchange format
//! written by MacStitch/WinStitch (Ursa Software) and read by KXStitch,
//! Pattern Maker and others.
//!
//! Shape of the parts this module reads/writes:
//!
//! ```xml
//! <chart>
//!   <properties chartwidth="80" chartheight="60" charttitle="..." stitchesperinch="14" .../>
//!   <palette>
//!     <palette_item index="0" number="cloth" name="cloth" color="FFFFFF" .../>
//!     <palette_item index="1" number="DMC 310" name="Black" color="000000" strands="2" symbol="X" .../>
//!   </palette>
//!   <fullstitches><stitch x="0" y="0" palindex="1"/></fullstitches>
//!   <partstitches>...</partstitches>
//!   <backstitches>...</backstitches>
//!   <ornaments_inc_knots_and_beads>...</ornaments_inc_knots_and_beads>
//! </chart>
//! ```
//!
//! Palette item 0 is always the fabric ("cloth"). Mapping of the rest,
//! per Ursa's spec (ursasoftware.com/OXSFormat):
//!
//! - `<partstitch direction="1|2">`: the square split by a `\` (1) or `/`
//!   (2) diagonal, `palindex1` coloring the triangle on the left and
//!   `palindex2` the one on the right -> `Partial::Split` (three-quarter
//!   stitches). `direction="3|4"`: a `/` or `\` half stitch ->
//!   `Partial::Half`.
//! - `<backstitch objecttype="backstitch">` with `.5`-precision grid
//!   positions -> `Backstitch`.
//! - `<object>` in `ornaments_inc_knots_and_beads`: `knot` -> `Knot`,
//!   `tent` (direction 1 = `\`, 2 = `/`) -> half stitch, `quarter` ->
//!   quarter stitch in the corner the position falls in, `fullcross` ->
//!   full cross.
//! - Palette `bsstrands` -> backstitch strands; blends from our own
//!   `<blend>` child, else Ursa's `blendcolor`.
//!
//! Anything else (beads, daisy/bugle lines, buttons...) is counted and
//! reported in `warnings` rather than silently dropped, so someone opening
//! a pattern that uses them knows the chart is incomplete.

use crate::chart::{
    Backstitch, BlendThread, Chart, Corner, Diagonal, Floss, Knot, Partial, SYMBOLS,
};
use crate::threads::{find_dmc, Catalog};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OxsError {
    #[error("not valid XML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("not an OXS chart: {0}")]
    Format(String),
}

pub struct OxsImport {
    pub chart: Chart,
    pub warnings: Vec<String>,
}

fn attr<'a>(node: roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|a| a.name().eq_ignore_ascii_case(name))
        .map(|a| a.value())
}

fn attr_num<T: std::str::FromStr>(node: roxmltree::Node, name: &str) -> Option<T> {
    attr(node, name).and_then(|v| v.trim().parse().ok())
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// Splits an OXS `number` attribute ("DMC 310", "DMC-310", "Anchor 403",
/// "310") into brand and code.
///
/// Whitespace wins over `-`, so hyphenated brands we write ourselves
/// ("Artkal-S S01", "Perler 80-15179") split correctly; `-` is only a
/// separator when there's no whitespace at all ("DMC-310").
fn split_number(number: &str) -> (String, String) {
    let n = number.trim();
    let split = n
        .split_once(char::is_whitespace)
        .or_else(|| n.split_once('-'));
    match split {
        Some((brand, code)) if !brand.is_empty() && !code.trim().is_empty() => {
            (brand.to_string(), code.trim().to_string())
        }
        _ if find_dmc(n).is_some() => ("DMC".to_string(), n.to_string()),
        _ => (String::new(), n.to_string()),
    }
}

fn descendants_named<'a, 'i>(
    root: roxmltree::Node<'a, 'i>,
    name: &'static str,
) -> impl Iterator<Item = roxmltree::Node<'a, 'i>> {
    root.descendants()
        .filter(move |n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case(name))
}

pub fn parse_oxs(xml: &str) -> Result<OxsImport, OxsError> {
    let doc = roxmltree::Document::parse(xml)?;
    let root = doc.root_element();
    if !root.tag_name().name().eq_ignore_ascii_case("chart") {
        return Err(OxsError::Format(format!(
            "root element is <{}>, expected <chart>",
            root.tag_name().name()
        )));
    }
    let mut warnings = Vec::new();

    let props = descendants_named(root, "properties").next();
    let mut width: usize = props.and_then(|p| attr_num(p, "chartwidth")).unwrap_or(0);
    let mut height: usize = props.and_then(|p| attr_num(p, "chartheight")).unwrap_or(0);
    let title = props
        .and_then(|p| attr(p, "charttitle"))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string);
    let count: Option<f32> = props
        .and_then(|p| attr_num(p, "stitchesperinch"))
        .filter(|c: &f32| c.is_finite() && *c > 0.0);

    // OXS palette index -> our palette index (None for the cloth entry).
    let mut index_map: Vec<(i64, Option<u16>)> = Vec::new();
    let mut chart = Chart::new(0, 0);
    if let Some(c) = count {
        chart.fabric.count = c;
    }
    let count_y: Option<f32> = props
        .and_then(|p| attr_num(p, "stitchesperinch_y"))
        .filter(|c: &f32| c.is_finite() && *c > 0.0);
    if let (Some(x), Some(y)) = (count, count_y) {
        if (x - y).abs() > 1e-3 {
            chart.fabric.count_y = Some(y);
        }
    }
    let mut used_symbols: Vec<char> = Vec::new();
    for item in descendants_named(root, "palette_item") {
        let Some(index) = attr_num::<i64>(item, "index") else {
            warnings.push("skipped a palette item with no index".to_string());
            continue;
        };
        let rgb = attr(item, "color").and_then(parse_hex);
        let number = attr(item, "number").unwrap_or("");
        if index == 0 || number.trim().eq_ignore_ascii_case("cloth") {
            if let Some(rgb) = rgb {
                chart.fabric.rgb = rgb;
            }
            index_map.push((index, None));
            continue;
        }
        let Some(rgb) = rgb else {
            warnings.push(format!(
                "palette item {index} has no usable color; its stitches were skipped"
            ));
            continue;
        };
        let (brand, code) = split_number(number);
        let name = attr(item, "name").unwrap_or("").trim().to_string();
        let strands = attr_num::<u8>(item, "strands")
            .filter(|s| (1..=6).contains(s))
            .unwrap_or(2);
        let bs_strands = attr_num::<u8>(item, "bsstrands")
            .filter(|s| (1..=6).contains(s))
            .unwrap_or(1);
        // Keep the file's symbol only when it's one of ours and still
        // free - OXS writers disagree on what `symbol` means (a literal
        // character in some, a glyph index into a symbol font in others),
        // so anything else gets a fresh, unambiguous symbol.
        let symbol = attr(item, "symbol")
            .and_then(|s| {
                let mut ch = s.chars();
                match (ch.next(), ch.next()) {
                    (Some(c), None) if SYMBOLS.contains(&c) && !used_symbols.contains(&c) => {
                        Some(c)
                    }
                    _ => None,
                }
            })
            .unwrap_or(' ');
        let floss = Floss {
            brand,
            code,
            name,
            rgb,
            symbol,
            strands,
            bs_strands,
            blend: read_blend(item),
        };
        let idx = chart.add_floss(floss);
        used_symbols.push(chart.palette[idx as usize].symbol);
        index_map.push((index, Some(idx)));
    }

    // Resolves an OXS palette index: Ok(None) for cloth (= nothing
    // stitched), Err for an index the palette doesn't define.
    let resolve = |p: i64| match index_map.iter().find(|(i, _)| *i == p) {
        Some((_, v)) => Ok(*v),
        None if p == 0 => Ok(None),
        None => Err(()),
    };
    let mut bad_refs = 0usize;
    let mut bad_coords = 0usize;
    let mut unsupported: Vec<(String, usize)> = Vec::new();
    let mut note_unsupported = |kind: &str| match unsupported.iter_mut().find(|(k, _)| k == kind) {
        Some((_, n)) => *n += 1,
        None => unsupported.push((kind.to_string(), 1)),
    };

    let mut fulls: Vec<(usize, usize, u16)> = Vec::new();
    for s in descendants_named(root, "stitch") {
        let (Some(x), Some(y), Some(p)) = (
            attr_num::<usize>(s, "x"),
            attr_num::<usize>(s, "y"),
            attr_num::<i64>(s, "palindex"),
        ) else {
            bad_refs += 1;
            continue;
        };
        match resolve(p) {
            Ok(Some(idx)) => fulls.push((x, y, idx)),
            Ok(None) => {}
            Err(()) => bad_refs += 1,
        }
    }

    let mut partials: Vec<(usize, usize, Partial)> = Vec::new();
    for s in descendants_named(root, "partstitch") {
        let (Some(x), Some(y), Some(p1), Some(p2), Some(dir)) = (
            attr_num::<usize>(s, "x"),
            attr_num::<usize>(s, "y"),
            attr_num::<i64>(s, "palindex1"),
            attr_num::<i64>(s, "palindex2"),
            attr_num::<u8>(s, "direction"),
        ) else {
            bad_refs += 1;
            continue;
        };
        let (Ok(a), Ok(b)) = (resolve(p1), resolve(p2)) else {
            bad_refs += 1;
            continue;
        };
        let p = match dir {
            1 => Partial::Split {
                diagonal: Diagonal::Backslash,
                first: a,
                second: b,
            },
            2 => Partial::Split {
                diagonal: Diagonal::Slash,
                first: a,
                second: b,
            },
            3 | 4 => {
                let Some(floss) = a.or(b) else { continue };
                let diagonal = if dir == 3 {
                    Diagonal::Slash
                } else {
                    Diagonal::Backslash
                };
                Partial::Half { diagonal, floss }
            }
            _ => {
                note_unsupported("part stitch with an unknown direction");
                continue;
            }
        };
        if !p.is_empty() {
            partials.push((x, y, p));
        }
    }

    let mut backstitches: Vec<Backstitch> = Vec::new();
    for b in descendants_named(root, "backstitch") {
        let kind = attr(b, "objecttype")
            .unwrap_or("backstitch")
            .trim()
            .to_ascii_lowercase();
        if kind != "backstitch" {
            note_unsupported(&format!("\"{kind}\" line"));
            continue;
        }
        let coords = ["x1", "y1", "x2", "y2"].map(|a| attr(b, a).and_then(half_units));
        let [Some(x1), Some(y1), Some(x2), Some(y2)] = coords else {
            bad_coords += 1;
            continue;
        };
        match attr_num::<i64>(b, "palindex").map(resolve) {
            Some(Ok(Some(floss))) => backstitches.push(Backstitch {
                from: (x1, y1),
                to: (x2, y2),
                floss,
            }),
            Some(Ok(None)) => {}
            _ => bad_refs += 1,
        }
    }

    let mut knots: Vec<Knot> = Vec::new();
    let ornaments = descendants_named(root, "ornaments_inc_knots_and_beads").next();
    for o in ornaments
        .into_iter()
        .flat_map(|n| descendants_named(n, "object"))
    {
        let kind = attr(o, "objecttype")
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let (Some(hx), Some(hy)) = (
            attr(o, "x1").and_then(half_units),
            attr(o, "y1").and_then(half_units),
        ) else {
            bad_coords += 1;
            continue;
        };
        let floss = match attr_num::<i64>(o, "palindex").map(resolve) {
            Some(Ok(Some(f))) => f,
            Some(Ok(None)) => continue,
            _ => {
                bad_refs += 1;
                continue;
            }
        };
        // Square containing the point, and where in it the point sits
        // (quarter-square precision is all a quarter stitch needs).
        let raw = |a: &str| attr_num::<f32>(o, a).unwrap_or(0.0);
        let (fx, fy) = (raw("x1"), raw("y1"));
        let (sx, sy) = (fx.floor().max(0.0) as usize, fy.floor().max(0.0) as usize);
        match kind.as_str() {
            "knot" => knots.push(Knot {
                at: (hx, hy),
                floss,
            }),
            "fullcross" => fulls.push((sx, sy, floss)),
            "tent" => {
                let diagonal = if attr_num::<u8>(o, "direction") == Some(2) {
                    Diagonal::Slash
                } else {
                    Diagonal::Backslash
                };
                partials.push((sx, sy, Partial::Half { diagonal, floss }));
            }
            "quarter" => {
                let corner = Corner::nearest(fx - fx.floor(), fy - fy.floor());
                let mut q = [None; 4];
                q[corner as usize] = Some(floss);
                partials.push((sx, sy, Partial::Quarters(q)));
            }
            other => note_unsupported(&format!(
                "\"{}\" ornament",
                if other.is_empty() { "unnamed" } else { other }
            )),
        }
    }

    // Some writers omit/misstate chartwidth/chartheight - grow to fit.
    for &(x, y, _) in &fulls {
        width = width.max(x + 1);
        height = height.max(y + 1);
    }
    for (x, y, _) in &partials {
        width = width.max(x + 1);
        height = height.max(y + 1);
    }
    let points = backstitches
        .iter()
        .flat_map(|b| [b.from, b.to])
        .chain(knots.iter().map(|k| k.at));
    for (hx, hy) in points {
        width = width.max((hx as usize).div_ceil(2));
        height = height.max((hy as usize).div_ceil(2));
    }
    if width == 0 || height == 0 {
        return Err(OxsError::Format(
            "chart has no size and no stitches".to_string(),
        ));
    }
    if width.saturating_mul(height) > crate::cgp::MAX_CELLS {
        return Err(OxsError::Format(format!(
            "chart too large ({width}x{height})"
        )));
    }
    chart.width = width;
    chart.height = height;
    chart.cells = vec![None; width * height];
    chart.name = title;
    for (x, y, idx) in fulls {
        chart.set(x, y, Some(idx));
    }
    for (x, y, p) in partials {
        let merged = match (chart.partial(x, y), &p) {
            (Some(Partial::Quarters(old)), Partial::Quarters(new)) => {
                Partial::Quarters([0, 1, 2, 3].map(|i| new[i].or(old[i])))
            }
            (
                Some(Partial::Split {
                    diagonal: d0,
                    first: f0,
                    second: s0,
                }),
                Partial::Split {
                    diagonal,
                    first,
                    second,
                },
            ) if d0 == diagonal => Partial::Split {
                diagonal: *diagonal,
                first: first.or(*f0),
                second: second.or(*s0),
            },
            _ => p,
        };
        chart.set_partial(x, y, merged);
    }
    chart.backstitches = backstitches;
    chart.knots = knots;

    if bad_refs > 0 {
        warnings.push(format!(
            "skipped {bad_refs} stitch(es) with missing or unknown palette references"
        ));
    }
    if bad_coords > 0 {
        warnings.push(format!(
            "skipped {bad_coords} item(s) with missing or invalid positions"
        ));
    }
    for (kind, n) in unsupported {
        warnings.push(format!("{n} {kind}(s) not supported yet - skipped"));
    }

    Ok(OxsImport { chart, warnings })
}

/// OXS position ("3", "3.5") -> half-square units. Positions finer than a
/// half square are rounded to the nearest half.
fn half_units(v: &str) -> Option<u32> {
    let v: f32 = v.trim().parse().ok()?;
    (v.is_finite() && v >= 0.0).then(|| (v * 2.0).round() as u32)
}

/// A palette item's blend thread: our own `<blend>` child when present
/// (it carries the thread number), otherwise Ursa's `blendcolor`
/// attribute (color only - matched back to a catalog thread when the color
/// is an exact catalog color).
fn read_blend(item: roxmltree::Node) -> Option<BlendThread> {
    if let Some(b) = descendants_named(item, "blend").next() {
        let rgb = attr(b, "color").and_then(parse_hex)?;
        let (brand, code) = split_number(attr(b, "number").unwrap_or(""));
        return Some(BlendThread {
            brand,
            code,
            name: attr(b, "name").unwrap_or("").trim().to_string(),
            rgb,
        });
    }
    let rgb = attr(item, "blendcolor").and_then(parse_hex)?;
    Some(
        Catalog::ALL
            .into_iter()
            .find_map(|c| c.find_by_rgb(rgb))
            .map(BlendThread::from_thread)
            .unwrap_or(BlendThread {
                brand: String::new(),
                code: String::new(),
                name: "Blend".to_string(),
                rgb,
            }),
    )
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn hex_upper(rgb: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

pub fn to_oxs(chart: &Chart) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<chart>\n");
    out.push_str(
        "<format comments01=\"Open Cross Stitch (OXS) chart\" comments02=\"palette item 0 is the cloth color\"/>\n",
    );
    out.push_str(&format!(
        "<properties oxsversion=\"1.0\" software=\"Abyssal Thread\" software_version=\"{}\" chartheight=\"{}\" chartwidth=\"{}\" charttitle=\"{}\" author=\"\" copyright=\"\" instructions=\"\" stitchesperinch=\"{}\" stitchesperinch_y=\"{}\" palettecount=\"{}\"/>\n",
        env!("CARGO_PKG_VERSION"),
        chart.height,
        chart.width,
        esc(chart.name.as_deref().unwrap_or("")),
        chart.fabric.count,
        chart.fabric.rows_per_inch(),
        chart.palette.len(),
    ));
    out.push_str("<palette>\n");
    let cloth = hex_upper(chart.fabric.rgb);
    out.push_str(&format!(
        "<palette_item index=\"0\" number=\"cloth\" name=\"cloth\" color=\"{cloth}\" printcolor=\"{cloth}\" blendcolor=\"nil\" comments=\"aida\" strands=\"0\" symbol=\"\" dashpattern=\"\" bsstrands=\"0\" bscolor=\"nil\"/>\n"
    ));
    for (i, f) in chart.palette.iter().enumerate() {
        let number = format!("{} {}", f.brand, f.code).trim().to_string();
        let color = hex_upper(f.rgb);
        let blendcolor = f
            .blend
            .as_ref()
            .map_or("nil".to_string(), |b| hex_upper(b.rgb));
        let attrs = format!(
            "index=\"{}\" number=\"{}\" name=\"{}\" color=\"{color}\" printcolor=\"{color}\" blendcolor=\"{blendcolor}\" comments=\"\" strands=\"{}\" symbol=\"{}\" dashpattern=\"\" bsstrands=\"{}\" bscolor=\"{color}\"",
            i + 1,
            esc(&number),
            esc(&f.name),
            f.strands,
            esc(&f.symbol.to_string()),
            f.bs_strands,
        );
        match &f.blend {
            // Ursa's spec anticipates `blend` sub-items; readers that
            // don't know them ignore them and still get `blendcolor`.
            Some(b) => out.push_str(&format!(
                "<palette_item {attrs}>\n<blend number=\"{}\" name=\"{}\" color=\"{}\" strands=\"{}\"/>\n</palette_item>\n",
                esc(format!("{} {}", b.brand, b.code).trim()),
                esc(&b.name),
                hex_upper(b.rgb),
                f.strands,
            )),
            None => out.push_str(&format!("<palette_item {attrs}/>\n")),
        }
    }
    out.push_str("</palette>\n<fullstitches>\n");
    for y in 0..chart.height {
        for x in 0..chart.width {
            if let Some(i) = chart.get(x, y) {
                out.push_str(&format!(
                    "<stitch x=\"{x}\" y=\"{y}\" palindex=\"{}\"/>\n",
                    i + 1
                ));
            }
        }
    }
    out.push_str("</fullstitches>\n<partstitches>\n");
    let pal = |v: Option<u16>| v.map_or(0, |i| i as usize + 1);
    let mut objects = String::new();
    for (&(y, x), p) in &chart.partials {
        match p {
            Partial::Half { diagonal, floss } => {
                let dir = if *diagonal == Diagonal::Slash { 3 } else { 4 };
                let n = *floss as usize + 1;
                out.push_str(&format!(
                    "<partstitch x=\"{x}\" y=\"{y}\" palindex1=\"{n}\" palindex2=\"{n}\" direction=\"{dir}\"/>\n"
                ));
            }
            Partial::Split {
                diagonal,
                first,
                second,
            } => {
                let dir = if *diagonal == Diagonal::Backslash {
                    1
                } else {
                    2
                };
                out.push_str(&format!(
                    "<partstitch x=\"{x}\" y=\"{y}\" palindex1=\"{}\" palindex2=\"{}\" direction=\"{dir}\"/>\n",
                    pal(*first),
                    pal(*second)
                ));
            }
            Partial::Quarters(q) => {
                for corner in Corner::ALL {
                    if let Some(i) = q[corner as usize] {
                        let fx = x as f32 + if corner.is_right() { 0.75 } else { 0.25 };
                        let fy = y as f32 + if corner.is_bottom() { 0.75 } else { 0.25 };
                        objects.push_str(&format!(
                            "<object x1=\"{fx}\" y1=\"{fy}\" palindex=\"{}\" objecttype=\"quarter\"/>\n",
                            i + 1
                        ));
                    }
                }
            }
        }
    }
    out.push_str("</partstitches>\n<backstitches>\n");
    let pos = |v: u32| v as f32 / 2.0;
    for (seq, b) in chart.backstitches.iter().enumerate() {
        out.push_str(&format!(
            "<backstitch x1=\"{}\" x2=\"{}\" y1=\"{}\" y2=\"{}\" palindex=\"{}\" objecttype=\"backstitch\" sequence=\"{seq}\"/>\n",
            pos(b.from.0),
            pos(b.to.0),
            pos(b.from.1),
            pos(b.to.1),
            b.floss + 1
        ));
    }
    out.push_str("</backstitches>\n<ornaments_inc_knots_and_beads>\n");
    for k in &chart.knots {
        objects.push_str(&format!(
            "<object x1=\"{}\" y1=\"{}\" palindex=\"{}\" objecttype=\"knot\"/>\n",
            pos(k.at.0),
            pos(k.at.1),
            k.floss + 1
        ));
    }
    out.push_str(&objects);
    out.push_str("</ornaments_inc_knots_and_beads>\n<commentboxes/>\n</chart>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::Fabric;
    use crate::threads::find_dmc;

    /// Hand-written in the shape MacStitch/WinStitch export (after the
    /// spec's own examples), including parts we don't support.
    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<chart>
<format comments01="Designed to allow interchange of basic pattern data between any cross stitch style software"/>
<properties oxsversion="1.0" software="MacStitch" chartheight="3" chartwidth="4" charttitle="Frog &amp; Friends" author="" stitchesperinch="16" stitchesperinch_y="16" palettecount="2"/>
<palette>
<palette_item index="0" number="cloth" name="cloth" color="F0EADA" printcolor="F0EADA" blendcolor="nil" comments="aida" strands="2" symbol="0"/>
<palette_item index="1" number="DMC      310" name="Black" color="000000" strands="2" symbol="1" bsstrands="2" blendcolor="F9F7F1"/>
<palette_item index="2" number="DMC-703" name="Chartreuse" color="7BB547" strands="3" symbol="O"/>
</palette>
<fullstitches>
<stitch x="0" y="0" palindex="1"/>
<stitch x="3" y="2" palindex="2"/>
<stitch x="1" y="1" palindex="0"/>
<stitch x="2" y="1" palindex="9"/>
</fullstitches>
<partstitches>
<partstitch x="1" y="1" palindex1="1" palindex2="0" direction="1"/>
<partstitch x="2" y="0" palindex1="2" palindex2="2" direction="3"/>
</partstitches>
<backstitches>
<backstitch x1="0" y1="0" x2="1.5" y2="1" palindex="1" objecttype="backstitch" sequence="0"/>
<backstitch x1="1" x2="1" y1="1" y2="2" palindex="1" objecttype="daisy" sequence="0"/>
</backstitches>
<ornaments_inc_knots_and_beads>
<object x1="2" y1="1.5" palindex="2" objecttype="knot"/>
<object x1="0.8" y1="2.2" palindex="1" objecttype="quarter"/>
<object x1="1" y1="1" palindex="1" objecttype="bead"/>
</ornaments_inc_knots_and_beads>
<commentboxes/>
</chart>"#;

    #[test]
    fn imports_a_macstitch_style_chart() {
        let OxsImport { chart, warnings } = parse_oxs(SAMPLE).unwrap();
        assert_eq!((chart.width, chart.height), (4, 3));
        assert_eq!(chart.name.as_deref(), Some("Frog & Friends"));
        assert_eq!(chart.fabric.count, 16.0);
        assert_eq!(chart.fabric.rgb, [0xF0, 0xEA, 0xDA]);
        assert_eq!(chart.palette.len(), 2);
        let black = &chart.palette[0];
        assert_eq!((black.brand.as_str(), black.code.as_str()), ("DMC", "310"));
        assert_eq!(black.bs_strands, 2);
        assert_eq!(
            black.blend.as_ref().unwrap().code,
            "3865",
            "blendcolor matched to catalog"
        );
        assert_eq!(chart.palette[1].code, "703");
        assert_eq!(chart.palette[1].strands, 3);
        assert_eq!(chart.palette[1].symbol, 'O', "a valid, free symbol is kept");
        assert_ne!(black.symbol, '1', "a font-glyph-index symbol is replaced");
        assert_eq!(chart.get(0, 0), Some(0));
        assert_eq!(chart.get(3, 2), Some(1));
        assert_eq!(chart.total_stitches(), 2);
        assert_eq!(
            chart.partial(1, 1),
            Some(&Partial::Split {
                diagonal: Diagonal::Backslash,
                first: Some(0),
                second: None
            })
        );
        assert_eq!(
            chart.partial(2, 0),
            Some(&Partial::Half {
                diagonal: Diagonal::Slash,
                floss: 1
            })
        );
        assert_eq!(
            chart.partial(0, 2),
            Some(&Partial::Quarters([None, Some(0), None, None]))
        );
        assert_eq!(
            chart.backstitches,
            vec![Backstitch {
                from: (0, 0),
                to: (3, 2),
                floss: 0
            }]
        );
        assert_eq!(
            chart.knots,
            vec![Knot {
                at: (4, 3),
                floss: 1
            }]
        );
        assert!(
            warnings.iter().any(|w| w.contains("unknown palette")),
            "{warnings:?}"
        );
        assert!(
            warnings.iter().any(|w| w.contains("\"daisy\" line")),
            "{warnings:?}"
        );
        assert!(
            warnings.iter().any(|w| w.contains("\"bead\" ornament")),
            "{warnings:?}"
        );
        assert_eq!(warnings.len(), 3, "{warnings:?}");
    }

    #[test]
    fn round_trips_every_stitch_kind_through_our_own_writer() {
        let mut c = Chart::new(3, 2);
        c.name = Some("<tricky> \"name\" & co".to_string());
        c.fabric = Fabric {
            count: 18.0,
            count_y: Some(20.0),
            rgb: [1, 2, 3],
        };
        let mut black = Floss::from_thread(find_dmc("310").unwrap(), 'X');
        black.blend = Some(BlendThread::from_thread(
            Catalog::Anchor.find("403").unwrap(),
        ));
        black.bs_strands = 3;
        c.add_floss(black);
        c.add_floss(Floss::from_thread(find_dmc("B5200").unwrap(), '&'));
        c.cells = vec![Some(0), None, Some(1), None, None, None];
        c.set_partial(
            1,
            0,
            Partial::Split {
                diagonal: Diagonal::Slash,
                first: None,
                second: Some(1),
            },
        );
        c.set_partial(
            0,
            1,
            Partial::Half {
                diagonal: Diagonal::Backslash,
                floss: 0,
            },
        );
        c.set_partial(1, 1, Partial::Quarters([Some(0), None, Some(1), Some(0)]));
        c.set_partial(
            2,
            1,
            Partial::Half {
                diagonal: Diagonal::Slash,
                floss: 1,
            },
        );
        c.backstitches.push(Backstitch {
            from: (0, 0),
            to: (6, 3),
            floss: 1,
        });
        c.knots.push(Knot {
            at: (5, 1),
            floss: 0,
        });
        let xml = to_oxs(&c);
        let back = parse_oxs(&xml).unwrap();
        assert!(back.warnings.is_empty(), "{:?}", back.warnings);
        assert_eq!(back.chart, c, "\n{xml}");
    }

    #[test]
    fn grows_to_fit_stitches_when_size_is_missing() {
        let xml = r#"<chart><palette><palette_item index="1" number="DMC 310" name="Black" color="000000"/></palette>
            <fullstitches><stitch x="5" y="2" palindex="1"/></fullstitches>
            <backstitches><backstitch x1="0" y1="0" x2="8" y2="1" palindex="1"/></backstitches></chart>"#;
        let c = parse_oxs(xml).unwrap().chart;
        assert_eq!((c.width, c.height), (8, 3));
    }

    #[test]
    fn splits_thread_numbers() {
        let t = |n: &str| split_number(n);
        assert_eq!(t("DMC 310"), ("DMC".into(), "310".into()));
        assert_eq!(t("DMC-703"), ("DMC".into(), "703".into()));
        assert_eq!(t("DMC      310"), ("DMC".into(), "310".into()));
        assert_eq!(t("Artkal-S S01"), ("Artkal-S".into(), "S01".into()));
        assert_eq!(t("Perler 80-15179"), ("Perler".into(), "80-15179".into()));
        assert_eq!(t("310"), ("DMC".into(), "310".into()));
    }

    #[test]
    fn rejects_non_charts() {
        assert!(parse_oxs("<html/>").is_err());
        assert!(parse_oxs("not xml").is_err());
        assert!(parse_oxs("<chart/>").is_err());
    }
}
