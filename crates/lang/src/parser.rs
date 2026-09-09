use crate::ast::{Modifier, Op, Pattern, Round};
use crate::error::ParseError;
use crate::lexer::{tokenize, Spanned, Token};
use abyssal_thread_core::ColorGrid;

/// Ceiling for a single `count`/repeat-`times` literal (`999999999shell`,
/// `(...) * 999999999`). Generous for anything a real pattern would ever
/// need - even a large blanket's single round tops out in the low
/// thousands of stitches - while still turning an obvious typo or
/// malicious input into an immediate, clear parse error instead of
/// `eval`/`flatten` later trying to build billions of stitch nodes and
/// hanging or exhausting memory. See `PatternTooLarge` (checked in
/// `eval::eval`) for the complementary guard against *compounding*
/// multiplications that each individually stay under this cap.
const MAX_LITERAL_COUNT: u32 = 10_000;

fn check_count(n: u32, line: usize) -> Result<u32, ParseError> {
    if n > MAX_LITERAL_COUNT {
        Err(ParseError::CountTooLarge(n, line, MAX_LITERAL_COUNT))
    } else {
        Ok(n)
    }
}

pub fn parse(src: &str) -> Result<Pattern, ParseError> {
    let tokens = tokenize(src)?;
    let mut p = Parser { tokens, pos: 0 };
    p.parse_pattern()
}

/// Parses a single comma-separated op list with no round/pattern wrapper -
/// used to lazily parse `DEF: name = <body>` bodies as ordinary DSL syntax.
/// (This only supports plain DSL op sequences - it does NOT implement
/// CrochetPARADE's `%`/`[%,%-4]` relative-attachment grammar for raw stitch
/// geometry; see the module doc in eval.rs.)
pub fn parse_ops(src: &str) -> Result<Vec<Op>, ParseError> {
    let tokens = tokenize(src)?;
    let mut p = Parser { tokens, pos: 0 };
    let round = p.parse_round()?;
    Ok(round.ops)
}

struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }

    fn line(&self) -> usize {
        self.tokens[self.pos].line
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos].token.clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn eat_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline) {
            self.advance();
        }
    }

    fn expect(&mut self, expected: &Token, ctx: &'static str) -> Result<(), ParseError> {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(expected) {
            self.advance();
            Ok(())
        } else {
            Err(ParseError::UnexpectedEof(ctx))
        }
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let mut pattern = Pattern::default();

        loop {
            self.eat_newlines();
            match self.peek().clone() {
                Token::Eof => break,
                Token::DefLine { name, body } => {
                    pattern.definitions.push((name, body));
                    self.advance();
                }
                Token::ColorGridHeader { width, height } => {
                    self.advance();
                    let mut grid = ColorGrid::new(width as usize, height as usize, [255, 255, 255]);
                    loop {
                        self.eat_newlines();
                        if let Token::ColorRow { index, hex } = self.peek().clone() {
                            self.advance();
                            let row_idx = (index as usize).saturating_sub(1);
                            if row_idx < grid.height {
                                for (x, color) in hex.iter().enumerate().take(grid.width) {
                                    grid.set(x, row_idx, *color);
                                }
                            }
                        } else {
                            break;
                        }
                    }
                    pattern.color_grid = Some(grid);
                }
                // `PATTERN: name` header - recognized wherever it appears
                // (not just as the very first line), since DEF lines are
                // conventionally placed above it.
                Token::Ident(name) if name.eq_ignore_ascii_case("PATTERN") => {
                    self.advance();
                    self.expect(&Token::Colon, "':' after PATTERN")?;
                    if let Token::Ident(pat_name) = self.peek().clone() {
                        pattern.name = Some(pat_name);
                        self.advance();
                    }
                }
                _ => {
                    let round = self.parse_round()?;
                    pattern.rounds.push(round);
                }
            }
        }
        Ok(pattern)
    }

    fn parse_round(&mut self) -> Result<Round, ParseError> {
        let mut ops = Vec::new();
        ops.push(self.parse_op()?);
        while matches!(self.peek(), Token::Comma) {
            self.advance();
            ops.push(self.parse_op()?);
        }
        // A round ends at newline or eof.
        if !matches!(self.peek(), Token::Newline | Token::Eof) {
            return Err(ParseError::UnexpectedToken(
                format!("{:?}", self.peek()),
                self.line(),
            ));
        }
        Ok(Round { ops })
    }

    fn parse_op(&mut self) -> Result<Op, ParseError> {
        match self.peek().clone() {
            Token::At => {
                self.advance();
                if let Token::Ident(name) = self.peek().clone() {
                    self.advance();
                    return Ok(Op::AttachTo(name));
                }
                Err(ParseError::UnexpectedEof("label name after '@'"))
            }
            Token::LParen => {
                self.advance();
                let mut body = Vec::new();
                body.push(self.parse_op()?);
                while matches!(self.peek(), Token::Comma) {
                    self.advance();
                    body.push(self.parse_op()?);
                }
                self.expect(&Token::RParen, "')' to close repeat group")?;
                self.expect(&Token::Star, "'*' followed by repeat count")?;
                if let Token::Number(n) = self.peek().clone() {
                    let line = self.line();
                    self.advance();
                    Ok(Op::Repeat {
                        body,
                        times: check_count(n, line)?,
                    })
                } else {
                    Err(ParseError::UnexpectedEof("repeat count after '*'"))
                }
            }
            Token::Number(n) => {
                let line = self.line();
                self.advance();
                self.parse_stitch_term(Some(check_count(n, line)?))
            }
            Token::Ident(_) => self.parse_stitch_term(None),
            other => Err(ParseError::UnexpectedToken(
                format!("{:?}", other),
                self.line(),
            )),
        }
    }

    /// Parses `[modifier '.'] ident ['!']`, where a trailing `!` marks a label.
    fn parse_stitch_term(&mut self, count: Option<u32>) -> Result<Op, ParseError> {
        let first = match self.advance() {
            Token::Ident(s) => s,
            other => {
                return Err(ParseError::UnexpectedToken(
                    format!("{:?}", other),
                    self.line(),
                ))
            }
        };

        // `label!` - a bare identifier immediately followed by '!' is a label,
        // not a stitch (only valid when no count/modifier preceded it).
        if count.is_none() && matches!(self.peek(), Token::Bang) {
            self.advance();
            return Ok(Op::Label(first));
        }

        let (modifier, abbrev) = if matches!(self.peek(), Token::Dot) {
            let m = match first.as_str() {
                "flo" => Modifier::FrontLoopOnly,
                "blo" => Modifier::BackLoopOnly,
                "fpost" => Modifier::FrontPost,
                "bpost" => Modifier::BackPost,
                other => {
                    return Err(ParseError::UnexpectedToken(
                        format!("unknown modifier '{other}'"),
                        self.line(),
                    ))
                }
            };
            self.advance(); // consume '.'
            let base = match self.advance() {
                Token::Ident(s) => s,
                other => {
                    return Err(ParseError::UnexpectedToken(
                        format!("{:?}", other),
                        self.line(),
                    ))
                }
            };
            (Some(m), base)
        } else {
            (None, first)
        };

        let color = if let Token::HexColor(c) = self.peek().clone() {
            self.advance();
            Some(c)
        } else {
            None
        };

        Ok(Op::Stitch {
            modifier,
            abbrev,
            count: count.unwrap_or(1),
            color,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Modifier;

    #[test]
    fn parses_simple_counted_stitch() {
        let p = parse("8sc\n").unwrap();
        assert_eq!(p.rounds.len(), 1);
        assert_eq!(
            p.rounds[0].ops,
            vec![Op::Stitch {
                modifier: None,
                abbrev: "sc".into(),
                color: None,
                count: 8
            }]
        );
    }

    #[test]
    fn parses_bare_stitch_defaults_to_count_one() {
        let p = parse("sc\n").unwrap();
        assert_eq!(
            p.rounds[0].ops,
            vec![Op::Stitch {
                modifier: None,
                abbrev: "sc".into(),
                color: None,
                count: 1
            }]
        );
    }

    #[test]
    fn parses_repeat_group() {
        let p = parse("(3sc, inc) * 4\n").unwrap();
        assert_eq!(p.rounds[0].ops.len(), 1);
        match &p.rounds[0].ops[0] {
            Op::Repeat { body, times } => {
                assert_eq!(*times, 4);
                assert_eq!(body.len(), 2);
            }
            other => panic!("expected Repeat, got {other:?}"),
        }
    }

    #[test]
    fn parses_label_and_attach() {
        let p = parse("sc, anchor!, sc\n@anchor, sc\n").unwrap();
        assert_eq!(p.rounds.len(), 2);
        assert!(matches!(&p.rounds[0].ops[1], Op::Label(s) if s == "anchor"));
        assert!(matches!(&p.rounds[1].ops[0], Op::AttachTo(s) if s == "anchor"));
    }

    #[test]
    fn parses_loop_and_post_modifiers() {
        let p = parse("flo.sc, bpost.dc\n").unwrap();
        match &p.rounds[0].ops[0] {
            Op::Stitch {
                modifier, abbrev, ..
            } => {
                assert_eq!(*modifier, Some(Modifier::FrontLoopOnly));
                assert_eq!(abbrev, "sc");
            }
            other => panic!("unexpected {other:?}"),
        }
        match &p.rounds[0].ops[1] {
            Op::Stitch {
                modifier, abbrev, ..
            } => {
                assert_eq!(*modifier, Some(Modifier::BackPost));
                assert_eq!(abbrev, "dc");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_pattern_header_and_strips_comments() {
        let p = parse("PATTERN: test\n# a comment\n6sc # trailing comment too\n").unwrap();
        assert_eq!(p.name.as_deref(), Some("test"));
        assert_eq!(p.rounds.len(), 1);
        assert_eq!(
            p.rounds[0].ops,
            vec![Op::Stitch {
                modifier: None,
                abbrev: "sc".into(),
                color: None,
                count: 6
            }]
        );
    }

    #[test]
    fn captures_def_line_verbatim() {
        let p = parse("DEF: shell = 3dc, ch1\n6sc\n").unwrap();
        assert_eq!(
            p.definitions,
            vec![("shell".to_string(), "3dc, ch1".to_string())]
        );
    }

    #[test]
    fn rejects_unknown_modifier() {
        let err = parse("bogus.sc\n").unwrap_err();
        assert!(matches!(err, ParseError::UnexpectedToken(_, _)));
    }

    #[test]
    fn rejects_a_stitch_count_over_the_safety_limit() {
        let err = parse("999999999shell\n").unwrap_err();
        assert!(matches!(err, ParseError::CountTooLarge(999999999, _, _)));
    }

    #[test]
    fn rejects_a_repeat_multiplier_over_the_safety_limit() {
        let err = parse("(sc) * 999999999\n").unwrap_err();
        assert!(matches!(err, ParseError::CountTooLarge(999999999, _, _)));
    }

    #[test]
    fn accepts_a_count_right_at_the_safety_limit() {
        assert!(parse("10000sc\n").is_ok());
        let err = parse("10001sc\n").unwrap_err();
        assert!(matches!(err, ParseError::CountTooLarge(10001, _, _)));
    }

    #[test]
    fn parse_ops_reads_a_bare_op_list() {
        let ops = parse_ops("3dc, ch1").unwrap();
        assert_eq!(
            ops,
            vec![
                Op::Stitch {
                    modifier: None,
                    abbrev: "dc".into(),
                    color: None,
                    count: 3
                },
                Op::Stitch {
                    modifier: None,
                    abbrev: "ch1".into(),
                    color: None,
                    count: 1
                },
            ]
        );
    }
}
