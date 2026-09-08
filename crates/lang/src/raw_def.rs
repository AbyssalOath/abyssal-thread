//! CrochetPARADE-style *raw* stitch-geometry `DEF` bodies - as opposed to
//! the plain "alias" `DEF` bodies (`parser::parse_ops`), which are just
//! ordinary DSL op sequences spliced in place N times. A raw body
//! additionally supports:
//!
//! - `%` - a "self" reference to the running placement cursor *at the
//!   point this term appears* - i.e. whatever was most recently placed
//!   before this term, the same node an omitted/default attachment would
//!   have used.
//! - `%-N` / `%+N` - N stitches before/after that same cursor point, in
//!   the body's own placement order (counting every individual stitch
//!   placed so far in this invocation, including earlier terms' repeated
//!   counts - e.g. `3ch` contributes 3 positions, not 1).
//! - `ss@1[%,%-4]` - `@N` (a *number*, unlike the ordinary DSL's `@label`
//!   identifier) sends this stitch's own primary parent attachment N
//!   positions back from the cursor instead of the normal positional
//!   parent. The bracketed list then adds *extra* `StitchEdge::Parent`
//!   edges into each resolved reference, on top of that primary one - a
//!   picot's closing slip stitch is genuinely worked into more than one
//!   earlier point (closing a loop), which a single parent edge can't
//!   express by itself.
//!
//! This is this project's own reading of CrochetPARADE's documented
//! syntax, worked out from the one example in the README
//! (`ss@1[%,%-4]`) - the exact original grammar isn't reproduced here.
//! The goal is a small, self-consistent grammar that can express genuinely
//! novel internal stitch geometry (closed loops back onto earlier points),
//! not a byte-for-byte CrochetPARADE parser.
//!
//! A `DEF` body is treated as raw (parsed here, expanded by
//! `eval::expand_raw_def`) rather than an alias (parsed by
//! `parser::parse_ops`, expanded by simple text-level splicing in
//! `eval::flatten`) purely by checking whether its text contains a `%` -
//! see `looks_like_raw_body`. Plain alias bodies never need `%`, so this
//! is an unambiguous switch.
//!
//! Scope: raw bodies may only use builtin stitch abbreviations (`ch`,
//! `sc`, etc.) - not other `DEF` names, so custom stitches can't be
//! nested inside raw geometry - and don't special-case `inc`/`dec`
//! parent-slot math the way top-level rounds do; they're for defining
//! novel single-stitch-chain geometry (picots, bobbles, closed loops),
//! not shaping. Use ordinary top-level `inc`/`dec` for that. No
//! modifiers (`flo.`/`fpost.` etc.) either - same restriction the alias
//! form already has for invocations.

use crate::error::ParseError;

/// A reference to another stitch's position, relative to the cursor at the
/// point the reference is written. See the module doc for exact semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawAttachRef {
    /// `%` - the cursor itself (equivalent to `Back(0)`).
    SelfRef,
    /// `%-N` - N stitches before the cursor.
    Back(u32),
    /// `%+N` - N stitches after the cursor. Only resolvable once that many
    /// further stitches have actually been placed later in the same body;
    /// `eval::expand_raw_def` still adds no edge if the body ends before
    /// that position is ever reached (out-of-range references stay
    /// non-fatal), but it now records a `StitchGraph::warnings` entry
    /// naming the def and the specific reference, rather than dropping it
    /// with no trace at all.
    Forward(u32),
}

/// One operation inside a raw `DEF` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawOp {
    /// `3ch` - one or more ordinary stitches, placed positionally exactly
    /// like top-level DSL stitches (each one consumes a parent slot from
    /// the enclosing round the normal way, unless it's the invocation's
    /// very first stitch and an explicit `@label` was given on the
    /// invocation itself).
    Stitch { abbrev: String, count: u32 },
    /// `ss@1[%,%-4]` - a single stitch (raw bodies don't support a count
    /// prefix on this form) whose primary parent is `back` positions
    /// behind the cursor (`None` = fall back to the normal positional
    /// parent, same as a plain `Stitch`), plus extra `Parent` edges into
    /// every resolved entry in `refs`.
    RelativeStitch { abbrev: String, back: Option<u32>, refs: Vec<RawAttachRef> },
}

/// Whether a `DEF` body should be parsed as raw stitch geometry rather than
/// a plain-DSL alias - see the module doc. Alias bodies never contain `%`.
pub fn looks_like_raw_body(body: &str) -> bool {
    body.contains('%')
}

/// Parses a raw `DEF` body (e.g. `3ch, ss@1[%,%-4]`) into a flat op list.
/// Deliberately hand-rolled rather than routed through `lexer::tokenize` -
/// `%`, `@N` (a *number*, not the ordinary DSL's `@label` identifier), and
/// `[...]` brackets are unique to this grammar and would otherwise have to
/// be bolted onto the shared lexer's `Token` enum for every ordinary-DSL
/// caller to also account for.
pub fn parse_raw_def(body: &str) -> Result<Vec<RawOp>, ParseError> {
    split_top_level_commas(body)
        .into_iter()
        .map(|term| parse_raw_term(term.trim()))
        .collect()
}

/// Splits on `,` at bracket-depth 0 only, so `ss@1[%,%-4]`'s internal comma
/// doesn't get mistaken for a term separator.
fn split_top_level_commas(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in body.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&body[start..]);
    out
}

fn parse_raw_term(term: &str) -> Result<RawOp, ParseError> {
    if let Some(at_idx) = term.find('@') {
        // `ss@1[%,%-4]` (or just `ss@1` with no bracket - a relative
        // primary parent but no extra closing edges).
        let (abbrev_part, rest) = term.split_at(at_idx);
        let rest = &rest[1..]; // skip '@'
        let abbrev = abbrev_part.trim().to_string();
        if abbrev.is_empty() {
            return Err(ParseError::UnexpectedEof("stitch abbreviation before '@' in raw DEF body"));
        }
        let (back_str, refs_str) = match rest.find('[') {
            Some(bracket_idx) => {
                let (n, bracket_rest) = rest.split_at(bracket_idx);
                let inner = bracket_rest
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .ok_or(ParseError::UnexpectedEof("closing ']' in raw DEF body"))?;
                (n, Some(inner))
            }
            None => (rest, None),
        };
        let back = if back_str.trim().is_empty() {
            None
        } else {
            Some(
                back_str
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| ParseError::InvalidNumber(back_str.trim().to_string(), 0))?,
            )
        };
        let refs = match refs_str {
            Some(inner) => inner
                .split(',')
                .map(|r| parse_ref(r.trim()))
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };
        Ok(RawOp::RelativeStitch { abbrev, back, refs })
    } else {
        // Plain `3ch`/`ch` term - same count-prefix grammar as the
        // ordinary DSL, but parsed directly here rather than via
        // `lexer`/`parser`.
        let digits_end = term.find(|c: char| !c.is_ascii_digit()).unwrap_or(term.len());
        let (count_str, abbrev) = term.split_at(digits_end);
        let count = if count_str.is_empty() {
            1
        } else {
            count_str
                .parse::<u32>()
                .map_err(|_| ParseError::InvalidNumber(count_str.to_string(), 0))?
        };
        let abbrev = abbrev.trim().to_string();
        if abbrev.is_empty() {
            return Err(ParseError::UnexpectedEof("stitch abbreviation in raw DEF body"));
        }
        Ok(RawOp::Stitch { abbrev, count })
    }
}

fn parse_ref(s: &str) -> Result<RawAttachRef, ParseError> {
    let s = s.trim();
    if s == "%" {
        Ok(RawAttachRef::SelfRef)
    } else if let Some(n) = s.strip_prefix("%-") {
        Ok(RawAttachRef::Back(
            n.parse().map_err(|_| ParseError::InvalidNumber(s.to_string(), 0))?,
        ))
    } else if let Some(n) = s.strip_prefix("%+") {
        Ok(RawAttachRef::Forward(
            n.parse().map_err(|_| ParseError::InvalidNumber(s.to_string(), 0))?,
        ))
    } else {
        Err(ParseError::UnexpectedToken(format!("expected '%'/'%-N'/'%+N', got '{s}'"), 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_raw_vs_alias_bodies() {
        assert!(!looks_like_raw_body("3dc, ch1"));
        assert!(looks_like_raw_body("3ch, ss@1[%,%-4]"));
    }

    #[test]
    fn parses_a_picot_body() {
        let ops = parse_raw_def("3ch, ss@1[%,%-4]").unwrap();
        assert_eq!(
            ops,
            vec![
                RawOp::Stitch { abbrev: "ch".to_string(), count: 3 },
                RawOp::RelativeStitch {
                    abbrev: "ss".to_string(),
                    back: Some(1),
                    refs: vec![RawAttachRef::SelfRef, RawAttachRef::Back(4)],
                },
            ]
        );
    }

    #[test]
    fn parses_plain_stitch_with_no_count() {
        assert_eq!(parse_raw_def("ch").unwrap(), vec![RawOp::Stitch { abbrev: "ch".into(), count: 1 }]);
    }

    #[test]
    fn parses_relative_stitch_with_no_bracket() {
        let ops = parse_raw_def("ss@1").unwrap();
        assert_eq!(ops, vec![RawOp::RelativeStitch { abbrev: "ss".to_string(), back: Some(1), refs: vec![] }]);
    }
}
