//! Cross-stitch charts as `.cgp` text - the same file extension and
//! line-oriented style as the crochet DSL, so one "Open"/"Save" and the
//! app's text-snapshot undo/redo/autosave machinery handle both crafts.
//!
//! ```text
//! PATTERN: pepe
//! CRAFT: cross-stitch
//! FABRIC: 14 ffffff
//! # THREAD n: brand code hex symbol strands name...
//! THREAD 1: DMC 310 000000 X 2 Black
//! THREAD 2: CUSTOM - ff66aa O 2 My pink
//! # optional per-thread extras
//! BLEND 1: DMC 3865 f9f7f1 Winter White
//! BSSTRANDS 1: 2
//! XSTITCH: 4x2
//! ROW 1: . 1 1 .
//! ROW 2: 2 . . 2
//! # part stitches: column row (1-based, like ROW) ...
//! HALF: 1 1 / 2
//! SPLIT: 4 1 \ 1 .
//! QUARTER: 2 2 . 1 . 2
//! # on grid lines: 0-based line coordinates, .5 allowed
//! BACK: 0 0 2.5 1 1
//! KNOT: 1.5 0.5 2
//! ```
//!
//! The `CRAFT:` line is what marks the file as a chart rather than a
//! crochet pattern (see `is_chart_source`) - the crochet parser never sees
//! these files. Any grid craft works there (`cross-stitch`,
//! `diamond-painting`, `fuse-beads`, `latch-hook`, `pixelhobby`,
//! `pixel-macrame`, `pixel-art` - see `profile::GridCraft::cgp_name`); for
//! those, `FABRIC:`'s count is cells per inch, and an optional
//! `BOARD: 29x29` gives the pegboard/baseplate size in cells.
//! Thread references are 1-based `THREAD` numbers, `.` for none. A custom
//! (non-catalog) color uses brand `CUSTOM` and code `-`.
//!
//! - `HALF: col row diag n` - half stitch, `diag` is `/` or `\`.
//! - `SPLIT: col row diag a b` - square split along `diag` into two
//!   three-quarter-stitch triangles; for `\`, `a` is bottom-left and `b`
//!   top-right; for `/`, `a` is top-left and `b` bottom-right (see
//!   `chart::Partial::Split`).
//! - `QUARTER: col row tl tr bl br` - quarter stitches per corner.
//! - `BACK: x1 y1 x2 y2 n` - backstitch between two grid points.
//! - `KNOT: x y n` - French knot at a grid point.

use crate::chart::{Backstitch, BlendThread, Chart, Diagonal, Fabric, Floss, Knot, Partial};
use crate::profile::GridCraft;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
#[error("line {line}: {message}")]
pub struct CgpError {
    pub line: usize,
    pub message: String,
}

fn err(line: usize, message: impl Into<String>) -> CgpError {
    CgpError {
        line,
        message: message.into(),
    }
}

const CUSTOM_BRAND: &str = "CUSTOM";
const CUSTOM_CODE: &str = "-";

/// Upper bound on chart squares accepted from a file - far beyond any
/// real cross-stitch chart (1000x1000 is ~70in square at 14-count), but
/// stops a typo'd or hostile size from allocating gigabytes.
pub const MAX_CELLS: usize = 1_000_000;

/// Whether `src` is a chart `.cgp` (has a `CRAFT:` line naming a grid
/// craft, e.g. `CRAFT: cross-stitch`) rather than a crochet pattern.
pub fn is_chart_source(src: &str) -> bool {
    src.lines().any(|l| {
        l.trim()
            .strip_prefix("CRAFT:")
            .is_some_and(|v| GridCraft::from_cgp_name(v.trim()).is_some())
    })
}

/// Splits the next whitespace-delimited token off the front of `rest`.
fn next_token<'a>(rest: &mut &'a str) -> Option<&'a str> {
    let s = rest.trim_start();
    if s.is_empty() {
        return None;
    }
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    let (tok, remainder) = s.split_at(end);
    *rest = remainder;
    Some(tok)
}

fn hex(rgb: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

fn brand_code<'a>(brand: &'a str, code: &'a str) -> (&'a str, &'a str) {
    if brand.is_empty() && code.is_empty() {
        (CUSTOM_BRAND, CUSTOM_CODE)
    } else {
        (brand, code)
    }
}

fn thread_ref(v: Option<u16>) -> String {
    v.map_or(".".to_string(), |i| (i + 1).to_string())
}

/// Half-square units -> "3" / "3.5".
fn line_coord(v: u32) -> String {
    if v.is_multiple_of(2) {
        (v / 2).to_string()
    } else {
        format!("{}.5", v / 2)
    }
}

pub fn to_cgp(chart: &Chart) -> String {
    let mut out = String::new();
    if let Some(name) = &chart.name {
        out.push_str(&format!("PATTERN: {name}\n"));
    }
    out.push_str(&format!("CRAFT: {}\n", chart.craft.cgp_name()));
    match chart.fabric.count_y {
        Some(rows) => out.push_str(&format!(
            "FABRIC: {} {} {rows}\n",
            chart.fabric.count,
            hex(chart.fabric.rgb)
        )),
        None => out.push_str(&format!(
            "FABRIC: {} {}\n",
            chart.fabric.count,
            hex(chart.fabric.rgb)
        )),
    }
    if chart.worked_in_round {
        out.push_str("ROUND: yes\n");
    }
    for (i, f) in chart.palette.iter().enumerate() {
        let (brand, code) = brand_code(&f.brand, &f.code);
        out.push_str(&format!(
            "THREAD {}: {brand} {code} {} {} {} {}\n",
            i + 1,
            hex(f.rgb),
            f.symbol,
            f.strands,
            f.name
        ));
        if let Some(b) = &f.blend {
            let (brand, code) = brand_code(&b.brand, &b.code);
            out.push_str(&format!(
                "BLEND {}: {brand} {code} {} {}\n",
                i + 1,
                hex(b.rgb),
                b.name
            ));
        }
        if f.bs_strands != 1 {
            out.push_str(&format!("BSSTRANDS {}: {}\n", i + 1, f.bs_strands));
        }
    }
    if let Some((bw, bh)) = chart.board {
        out.push_str(&format!("BOARD: {bw}x{bh}\n"));
    }
    out.push_str(&format!("XSTITCH: {}x{}\n", chart.width, chart.height));
    for y in 0..chart.height {
        let row: Vec<String> = (0..chart.width)
            .map(|x| thread_ref(chart.get(x, y)))
            .collect();
        out.push_str(&format!("ROW {}: {}\n", y + 1, row.join(" ")));
    }
    for (&(y, x), p) in &chart.partials {
        let (col, row) = (x + 1, y + 1);
        out.push_str(&match p {
            Partial::Half { diagonal, floss } => {
                format!("HALF: {col} {row} {} {}\n", diagonal.as_char(), floss + 1)
            }
            Partial::Split {
                diagonal,
                first,
                second,
            } => format!(
                "SPLIT: {col} {row} {} {} {}\n",
                diagonal.as_char(),
                thread_ref(*first),
                thread_ref(*second)
            ),
            Partial::Quarters(q) => {
                format!("QUARTER: {col} {row} {}\n", q.map(thread_ref).join(" "))
            }
        });
    }
    for b in &chart.backstitches {
        out.push_str(&format!(
            "BACK: {} {} {} {} {}\n",
            line_coord(b.from.0),
            line_coord(b.from.1),
            line_coord(b.to.0),
            line_coord(b.to.1),
            b.floss + 1
        ));
    }
    for k in &chart.knots {
        out.push_str(&format!(
            "KNOT: {} {} {}\n",
            line_coord(k.at.0),
            line_coord(k.at.1),
            k.floss + 1
        ));
    }
    out
}

/// "29x29" -> (29, 29); both at least 1.
fn parse_size(v: &str) -> Option<(usize, usize)> {
    let (w, h) = v.split_once(['x', 'X'])?;
    let (w, h): (usize, usize) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (w >= 1 && h >= 1).then_some((w, h))
}

/// Things that can only be validated once the chart size and palette are
/// known, kept with their line number for error messages.
enum Pending {
    Partial(usize, usize, Partial),
    Back(Backstitch),
    Knot(Knot),
    Blend(usize, BlendThread),
    BsStrands(usize, u8),
}

/// Parses a line's value as whitespace-separated tokens, erroring if the
/// count is wrong.
fn tokens(value: &str, line: usize, n: usize, usage: &str) -> Result<Vec<String>, CgpError> {
    let t: Vec<String> = value.split_whitespace().map(str::to_string).collect();
    if t.len() != n {
        return Err(err(line, format!("expected `{usage}`")));
    }
    Ok(t)
}

fn parse_thread_ref(tok: &str, line: usize) -> Result<Option<u16>, CgpError> {
    if tok == "." {
        return Ok(None);
    }
    tok.parse::<u16>()
        .ok()
        .filter(|v| *v >= 1)
        .map(|v| Some(v - 1))
        .ok_or_else(|| {
            err(
                line,
                format!("bad thread reference `{tok}` (use a THREAD number or `.`)"),
            )
        })
}

fn parse_required_ref(tok: &str, line: usize) -> Result<u16, CgpError> {
    parse_thread_ref(tok, line)?
        .ok_or_else(|| err(line, "a thread number is required here, not `.`"))
}

fn parse_square(col: &str, row: &str, line: usize) -> Result<(usize, usize), CgpError> {
    let parse = |t: &str| t.parse::<usize>().ok().filter(|v| *v >= 1);
    match (parse(col), parse(row)) {
        (Some(c), Some(r)) => Ok((c - 1, r - 1)),
        _ => Err(err(
            line,
            format!("bad square `{col} {row}` (1-based column and row)"),
        )),
    }
}

/// "3" / "3.5" -> half-square units.
fn parse_line_coord(tok: &str, line: usize) -> Result<u32, CgpError> {
    let v: f32 = tok
        .parse()
        .ok()
        .filter(|v: &f32| v.is_finite() && *v >= 0.0)
        .ok_or_else(|| err(line, format!("bad grid coordinate `{tok}`")))?;
    let halves = v * 2.0;
    if (halves - halves.round()).abs() > 1e-3 {
        return Err(err(
            line,
            format!("grid coordinate `{tok}` must be a whole or half number"),
        ));
    }
    Ok(halves.round() as u32)
}

fn parse_diagonal(tok: &str, line: usize) -> Result<Diagonal, CgpError> {
    let mut c = tok.chars();
    match (c.next().and_then(Diagonal::from_char), c.next()) {
        (Some(d), None) => Ok(d),
        _ => Err(err(
            line,
            format!("diagonal must be `/` or `\\`, got `{tok}`"),
        )),
    }
}

pub fn parse_cgp(src: &str) -> Result<Chart, CgpError> {
    let mut name = None;
    let mut craft: Option<GridCraft> = None;
    let mut fabric: Option<Fabric> = None;
    let mut board: Option<(usize, usize)> = None;
    let mut worked_in_round = false;
    let mut palette: Vec<(usize, Floss)> = Vec::new();
    let mut size: Option<(usize, usize)> = None;
    let mut rows: Vec<(usize, usize, Vec<Option<u16>>)> = Vec::new();
    let mut pending: Vec<(usize, Pending)> = Vec::new();

    for (i, raw) in src.lines().enumerate() {
        let line_no = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| err(line_no, format!("expected `KEY: value`, got `{line}`")))?;
        let value = value.trim();
        let mut key_parts = key.split_whitespace();
        let keyword = key_parts.next().unwrap_or("");
        let key_num = key_parts.next();
        let numbered = |what: &str| {
            key_num
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|n| *n >= 1)
                .ok_or_else(|| {
                    err(
                        line_no,
                        format!("{what} needs a number, e.g. `{what} 1: ...`"),
                    )
                })
        };
        match keyword {
            "PATTERN" => name = Some(value.to_string()).filter(|s| !s.is_empty()),
            "CRAFT" => {
                craft =
                    Some(GridCraft::from_cgp_name(value).ok_or_else(|| {
                        err(line_no, format!("not a chart craft (CRAFT: {value})"))
                    })?);
            }
            "ROUND" => {
                worked_in_round = match value.to_ascii_lowercase().as_str() {
                    "yes" | "true" => true,
                    "no" | "false" => false,
                    _ => {
                        return Err(err(
                            line_no,
                            format!("expected `ROUND: yes` or `no`, got `{value}`"),
                        ))
                    }
                };
            }
            "BOARD" => {
                board = Some(parse_size(value).ok_or_else(|| {
                    err(
                        line_no,
                        format!("expected `BOARD: WIDTHxHEIGHT`, got `{value}`"),
                    )
                })?);
            }
            "FABRIC" => {
                let mut parts = value.split_whitespace();
                let count: f32 = parts
                    .next()
                    .and_then(|c| c.parse().ok())
                    .filter(|c: &f32| c.is_finite() && *c > 0.0)
                    .ok_or_else(|| {
                        err(
                            line_no,
                            "FABRIC needs a positive count, e.g. `FABRIC: 14 ffffff`",
                        )
                    })?;
                let rgb = match parts.next() {
                    Some(h) => parse_hex(h)
                        .ok_or_else(|| err(line_no, format!("bad fabric color `{h}`")))?,
                    None => [255, 255, 255],
                };
                let count_y = match parts.next() {
                    Some(r) => Some(
                        r.parse::<f32>()
                            .ok()
                            .filter(|r| r.is_finite() && *r > 0.0)
                            .ok_or_else(|| err(line_no, format!("bad row gauge `{r}`")))?,
                    ),
                    None => None,
                };
                fabric = Some(Fabric {
                    count,
                    count_y,
                    rgb,
                });
            }
            "THREAD" => {
                let n = numbered("THREAD")?;
                let mut rest = value;
                let mut next = |what: &str| {
                    next_token(&mut rest)
                        .ok_or_else(|| err(line_no, format!("THREAD {n} is missing its {what}")))
                };
                let brand = next("brand")?;
                let code = next("code")?;
                let rgb_s = next("color")?;
                let symbol_s = next("symbol")?;
                let strands_s = next("strand count")?;
                // Everything after the strand count is the name, spaces and all.
                let fname = rest.trim().to_string();
                let rgb = parse_hex(rgb_s)
                    .ok_or_else(|| err(line_no, format!("bad thread color `{rgb_s}`")))?;
                let mut sym_chars = symbol_s.chars();
                let symbol = match (sym_chars.next(), sym_chars.next()) {
                    (Some(c), None) => c,
                    _ => {
                        return Err(err(
                            line_no,
                            format!("symbol must be one character, got `{symbol_s}`"),
                        ))
                    }
                };
                let strands: u8 = strands_s
                    .parse()
                    .ok()
                    .filter(|s| (1..=6).contains(s))
                    .ok_or_else(|| {
                        err(line_no, format!("strands must be 1-6, got `{strands_s}`"))
                    })?;
                let (brand, code) =
                    if brand.eq_ignore_ascii_case(CUSTOM_BRAND) && code == CUSTOM_CODE {
                        (String::new(), String::new())
                    } else {
                        (brand.to_string(), code.to_string())
                    };
                if palette.iter().any(|(m, _)| *m == n) {
                    return Err(err(line_no, format!("THREAD {n} is defined twice")));
                }
                palette.push((
                    n,
                    Floss {
                        brand,
                        code,
                        name: fname,
                        rgb,
                        symbol,
                        strands,
                        bs_strands: 1,
                        blend: None,
                    },
                ));
            }
            "BLEND" => {
                let n = numbered("BLEND")?;
                let mut rest = value;
                let mut next = |what: &str| {
                    next_token(&mut rest)
                        .ok_or_else(|| err(line_no, format!("BLEND {n} is missing its {what}")))
                };
                let brand = next("brand")?;
                let code = next("code")?;
                let rgb_s = next("color")?;
                let bname = rest.trim().to_string();
                let rgb = parse_hex(rgb_s)
                    .ok_or_else(|| err(line_no, format!("bad blend color `{rgb_s}`")))?;
                let (brand, code) =
                    if brand.eq_ignore_ascii_case(CUSTOM_BRAND) && code == CUSTOM_CODE {
                        (String::new(), String::new())
                    } else {
                        (brand.to_string(), code.to_string())
                    };
                pending.push((
                    line_no,
                    Pending::Blend(
                        n,
                        BlendThread {
                            brand,
                            code,
                            name: bname,
                            rgb,
                        },
                    ),
                ));
            }
            "BSSTRANDS" => {
                let n = numbered("BSSTRANDS")?;
                let s: u8 = value
                    .parse()
                    .ok()
                    .filter(|s| (1..=6).contains(s))
                    .ok_or_else(|| err(line_no, format!("strands must be 1-6, got `{value}`")))?;
                pending.push((line_no, Pending::BsStrands(n, s)));
            }
            "XSTITCH" => {
                let (w, h) = parse_size(value).ok_or_else(|| {
                    err(
                        line_no,
                        format!("expected `XSTITCH: WIDTHxHEIGHT`, got `{value}`"),
                    )
                })?;
                if w.saturating_mul(h) > MAX_CELLS {
                    return Err(err(
                        line_no,
                        format!("chart too large ({w}x{h}; max {MAX_CELLS} squares)"),
                    ));
                }
                size = Some((w, h));
            }
            "ROW" => {
                let n = numbered("ROW")?;
                let cells = value
                    .split_whitespace()
                    .map(|tok| parse_thread_ref(tok, line_no))
                    .collect::<Result<Vec<_>, _>>()?;
                rows.push((line_no, n, cells));
            }
            "HALF" => {
                let t = tokens(value, line_no, 4, "HALF: col row / n")?;
                let (x, y) = parse_square(&t[0], &t[1], line_no)?;
                let diagonal = parse_diagonal(&t[2], line_no)?;
                let floss = parse_required_ref(&t[3], line_no)?;
                pending.push((
                    line_no,
                    Pending::Partial(x, y, Partial::Half { diagonal, floss }),
                ));
            }
            "SPLIT" => {
                let t = tokens(value, line_no, 5, "SPLIT: col row / a b")?;
                let (x, y) = parse_square(&t[0], &t[1], line_no)?;
                let diagonal = parse_diagonal(&t[2], line_no)?;
                let first = parse_thread_ref(&t[3], line_no)?;
                let second = parse_thread_ref(&t[4], line_no)?;
                pending.push((
                    line_no,
                    Pending::Partial(
                        x,
                        y,
                        Partial::Split {
                            diagonal,
                            first,
                            second,
                        },
                    ),
                ));
            }
            "QUARTER" => {
                let t = tokens(value, line_no, 6, "QUARTER: col row tl tr bl br")?;
                let (x, y) = parse_square(&t[0], &t[1], line_no)?;
                let mut q = [None; 4];
                for (slot, tok) in q.iter_mut().zip(&t[2..]) {
                    *slot = parse_thread_ref(tok, line_no)?;
                }
                pending.push((line_no, Pending::Partial(x, y, Partial::Quarters(q))));
            }
            "BACK" => {
                let t = tokens(value, line_no, 5, "BACK: x1 y1 x2 y2 n")?;
                let from = (
                    parse_line_coord(&t[0], line_no)?,
                    parse_line_coord(&t[1], line_no)?,
                );
                let to = (
                    parse_line_coord(&t[2], line_no)?,
                    parse_line_coord(&t[3], line_no)?,
                );
                let floss = parse_required_ref(&t[4], line_no)?;
                pending.push((line_no, Pending::Back(Backstitch { from, to, floss })));
            }
            "KNOT" => {
                let t = tokens(value, line_no, 3, "KNOT: x y n")?;
                let at = (
                    parse_line_coord(&t[0], line_no)?,
                    parse_line_coord(&t[1], line_no)?,
                );
                let floss = parse_required_ref(&t[2], line_no)?;
                pending.push((line_no, Pending::Knot(Knot { at, floss })));
            }
            other => return Err(err(line_no, format!("unknown keyword `{other}`"))),
        }
    }

    let craft =
        craft.ok_or_else(|| err(1, "missing `CRAFT:` line (e.g. `CRAFT: cross-stitch`)"))?;
    let (width, height) = size.ok_or_else(|| err(1, "missing `XSTITCH: WIDTHxHEIGHT`"))?;

    // THREAD numbers are 1-based and must be contiguous so references
    // index straight into the palette.
    palette.sort_by_key(|(n, _)| *n);
    for (expected, (n, _)) in palette.iter().enumerate() {
        if *n != expected + 1 {
            return Err(err(
                1,
                format!(
                    "THREAD numbers must run 1, 2, 3, ... (missing THREAD {})",
                    expected + 1
                ),
            ));
        }
    }
    let mut chart = Chart::new(width, height);
    chart.name = name;
    chart.craft = craft;
    chart.board = board;
    chart.worked_in_round = worked_in_round;
    chart.fabric = fabric.unwrap_or(Fabric {
        count: craft.default_per_inch(),
        count_y: craft.default_per_inch_y(),
        ..Fabric::default()
    });
    chart.palette = palette.into_iter().map(|(_, f)| f).collect();
    let n_floss = chart.palette.len();
    let check_ref = |i: u16, line: usize| {
        if (i as usize) < n_floss {
            Ok(())
        } else {
            Err(err(line, format!("THREAD {} isn't defined", i + 1)))
        }
    };

    for (line_no, n, cells) in rows {
        if n > height {
            return Err(err(
                line_no,
                format!("ROW {n} is past the chart height {height}"),
            ));
        }
        if cells.len() != width {
            return Err(err(
                line_no,
                format!("ROW {n} has {} squares, expected {width}", cells.len()),
            ));
        }
        for (x, c) in cells.into_iter().enumerate() {
            if let Some(i) = c {
                check_ref(i, line_no)?;
            }
            chart.set(x, n - 1, c);
        }
    }

    let on_grid = |(x, y): (u32, u32)| x as usize <= 2 * width && y as usize <= 2 * height;
    for (line_no, item) in pending {
        match item {
            Pending::Blend(n, b) => {
                let f = chart
                    .palette
                    .get_mut(n - 1)
                    .ok_or_else(|| err(line_no, format!("BLEND {n}: THREAD {n} isn't defined")))?;
                f.blend = Some(b);
            }
            Pending::BsStrands(n, s) => {
                let f = chart.palette.get_mut(n - 1).ok_or_else(|| {
                    err(line_no, format!("BSSTRANDS {n}: THREAD {n} isn't defined"))
                })?;
                f.bs_strands = s;
            }
            Pending::Partial(x, y, p) => {
                if x >= width || y >= height {
                    return Err(err(
                        line_no,
                        format!(
                            "square {} {} is outside the {width}x{height} chart",
                            x + 1,
                            y + 1
                        ),
                    ));
                }
                for i in p.flosses() {
                    check_ref(i, line_no)?;
                }
                if chart.get(x, y).is_some() {
                    return Err(err(
                        line_no,
                        format!("square {} {} already has a full cross", x + 1, y + 1),
                    ));
                }
                chart.set_partial(x, y, p);
            }
            Pending::Back(b) => {
                if !on_grid(b.from) || !on_grid(b.to) {
                    return Err(err(line_no, "backstitch runs outside the chart"));
                }
                check_ref(b.floss, line_no)?;
                chart.backstitches.push(b);
            }
            Pending::Knot(k) => {
                if !on_grid(k.at) {
                    return Err(err(line_no, "knot is outside the chart"));
                }
                check_ref(k.floss, line_no)?;
                chart.knots.push(k);
            }
        }
    }
    Ok(chart)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threads::{find_dmc, Catalog};

    fn sample() -> Chart {
        let mut c = Chart::new(4, 2);
        c.name = Some("pepe test".to_string());
        c.fabric = Fabric {
            count: 16.0,
            count_y: None,
            rgb: [250, 240, 230],
        };
        let mut black = Floss::from_thread(find_dmc("310").unwrap(), 'X');
        black.blend = Some(BlendThread::from_thread(find_dmc("3865").unwrap()));
        black.bs_strands = 2;
        c.add_floss(black);
        c.add_floss(Floss {
            strands: 3,
            ..Floss::custom("My pink", [255, 102, 170], '#')
        });
        c.add_floss(Floss::from_thread(
            Catalog::Anchor.find("403").unwrap(),
            'O',
        ));
        c.cells = vec![None, Some(0), Some(0), None, Some(1), None, None, Some(1)];
        c.set_partial(
            0,
            0,
            Partial::Half {
                diagonal: Diagonal::Backslash,
                floss: 2,
            },
        );
        c.set_partial(
            3,
            0,
            Partial::Split {
                diagonal: Diagonal::Slash,
                first: Some(0),
                second: None,
            },
        );
        c.set_partial(1, 1, Partial::Quarters([None, Some(1), None, Some(2)]));
        c.backstitches.push(Backstitch {
            from: (0, 0),
            to: (5, 2),
            floss: 0,
        });
        c.knots.push(Knot {
            at: (3, 1),
            floss: 2,
        });
        c
    }

    #[test]
    fn round_trips_exactly() {
        let c = sample();
        let text = to_cgp(&c);
        assert!(is_chart_source(&text));
        assert_eq!(parse_cgp(&text).unwrap(), c, "\n{text}");
    }

    #[test]
    fn other_crafts_round_trip_with_their_board() {
        let mut c = Chart::new_for(GridCraft::FuseBeads);
        c.name = Some("beads".to_string());
        c.add_floss(Floss::from_thread(
            Catalog::Perler.find("80-15179").unwrap(),
            'X',
        ));
        c.add_floss(Floss::free([12, 34, 56], 'O'));
        c.set(3, 4, Some(0));
        c.set(5, 6, Some(1));
        let text = to_cgp(&c);
        assert!(
            text.contains("CRAFT: fuse-beads\nBOARD: 29x29\n") || text.contains("BOARD: 29x29"),
            "{text}"
        );
        assert!(is_chart_source(&text));
        assert_eq!(parse_cgp(&text).unwrap(), c, "\n{text}");

        // Missing FABRIC falls back to the craft's default cell size.
        let pixel = parse_cgp("CRAFT: pixel-art\nXSTITCH: 2x2\n").unwrap();
        assert_eq!(pixel.craft, GridCraft::PixelArt);
        assert_eq!(pixel.fabric.count, GridCraft::PixelArt.default_per_inch());
        assert!(parse_cgp("CRAFT: crochet\nXSTITCH: 2x2\n").is_err());
    }

    #[test]
    fn knitting_row_gauge_and_round_flag_round_trip() {
        let mut c = Chart::new_for(GridCraft::Knitting);
        c.worked_in_round = true;
        c.add_floss(Floss::free([1, 2, 3], 'X'));
        c.set(0, 0, Some(0));
        let text = to_cgp(&c);
        assert!(
            text.contains("FABRIC: 5.5 ffffff 7.5\nROUND: yes\n"),
            "{text}"
        );
        assert_eq!(parse_cgp(&text).unwrap(), c);
        let q = Chart::new_for(GridCraft::Quilt);
        assert_eq!(parse_cgp(&to_cgp(&q)).unwrap(), q);
        assert!(parse_cgp("CRAFT: knitting\nROUND: maybe\nXSTITCH: 1x1\n").is_err());
    }

    #[test]
    fn documented_example_parses() {
        let src = "PATTERN: pepe\nCRAFT: cross-stitch\nFABRIC: 14 ffffff\n\
                   # THREAD n: brand code hex symbol strands name...\n\
                   THREAD 1: DMC 310 000000 X 2 Black\n\
                   THREAD 2: CUSTOM - ff66aa O 2 My pink\n\
                   BLEND 1: DMC 3865 f9f7f1 Winter White\nBSSTRANDS 1: 2\n\
                   XSTITCH: 4x2\nROW 1: . 1 1 .\nROW 2: 2 . . 2\n\
                   HALF: 1 1 / 2\nSPLIT: 4 1 \\ 1 .\nQUARTER: 2 2 . 1 . 2\n\
                   BACK: 0 0 2.5 1 1\nKNOT: 1.5 0.5 2\n";
        let c = parse_cgp(src).unwrap();
        assert_eq!((c.width, c.height), (4, 2));
        assert_eq!(c.palette[1].brand, "");
        assert_eq!(c.palette[0].blend.as_ref().unwrap().code, "3865");
        assert_eq!(c.palette[0].bs_strands, 2);
        assert_eq!(c.get(0, 1), Some(1));
        assert_eq!(c.total_stitches(), 4);
        assert_eq!(c.partials.len(), 3);
        assert_eq!(c.backstitches[0].to, (5, 2));
        assert_eq!(c.knots[0].at, (3, 1));
    }

    #[test]
    fn crochet_patterns_are_not_detected_as_cross_stitch() {
        assert!(!is_chart_source("PATTERN: x\n6sc\n"));
        assert!(!is_chart_source("COLORGRID: 1x1\nROW 1: ffffff\n"));
    }

    #[test]
    fn reports_errors_with_line_numbers() {
        let bad_width = "CRAFT: cross-stitch\nXSTITCH: 2x1\nROW 1: . . .\n";
        assert_eq!(parse_cgp(bad_width).unwrap_err().line, 3);
        let undefined = "CRAFT: cross-stitch\nXSTITCH: 1x1\nROW 1: 1\n";
        assert!(parse_cgp(undefined)
            .unwrap_err()
            .message
            .contains("isn't defined"));
        let gap = "CRAFT: cross-stitch\nTHREAD 2: DMC 310 000000 X 2 Black\nXSTITCH: 1x1\n";
        assert!(parse_cgp(gap)
            .unwrap_err()
            .message
            .contains("missing THREAD 1"));
        let huge = "CRAFT: cross-stitch\nXSTITCH: 100000x100000\n";
        assert!(parse_cgp(huge).unwrap_err().message.contains("too large"));
        let head = "CRAFT: cross-stitch\nTHREAD 1: DMC 310 000000 X 2 Black\nXSTITCH: 2x2\n";
        for (bad, want) in [
            ("HALF: 3 1 / 1", "outside"),
            ("HALF: 1 1 | 1", "diagonal"),
            ("BACK: 0 0 0.25 1 1", "half number"),
            ("BACK: 0 0 9 1 1", "outside"),
            ("KNOT: 1 1 2", "isn't defined"),
            ("QUARTER: 1 1 . .", "expected"),
            ("BLEND 2: DMC 3865 f9f7f1 x", "isn't defined"),
        ] {
            let e = parse_cgp(&format!("{head}{bad}\n")).unwrap_err();
            assert_eq!(e.line, 4, "{bad}");
            assert!(e.message.contains(want), "{bad}: {}", e.message);
        }
    }
}
