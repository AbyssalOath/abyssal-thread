use crate::error::ParseError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Ident(String),
    Number(u32),
    Comma,
    Star,
    Dot,
    LParen,
    RParen,
    Bang,
    At,
    Colon,
    Equals,
    /// A full `DEF: name = ...` line, captured verbatim after `=`.
    DefLine {
        name: String,
        body: String,
    },
    /// `COLORGRID: WxH` header, starting an image-derived colorwork block.
    ColorGridHeader {
        width: u32,
        height: u32,
    },
    /// `ROW n: <hex> <hex> ...` - one row of a colorwork block. `hex`
    /// entries are bare 6-digit hex colors (no leading `#`, deliberately -
    /// `#` already means "comment to end of line" in this grammar).
    ColorRow {
        index: u32,
        hex: Vec<[u8; 3]>,
    },
    HexColor([u8; 3]),
    Newline,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Spanned {
    pub token: Token,
    pub line: usize,
}

pub fn tokenize(src: &str) -> Result<Vec<Spanned>, ParseError> {
    let mut tokens = Vec::new();
    for (line_no, raw_line) in src.lines().enumerate() {
        let line = line_no + 1;
        let trimmed = raw_line.trim();

        // Strip comments (`# ...`) before anything else.
        let content = match trimmed.split_once('#') {
            Some((before, _)) => before.trim(),
            None => trimmed,
        };

        if content.is_empty() {
            tokens.push(Spanned {
                token: Token::Newline,
                line,
            });
            continue;
        }

        // `COLORGRID: WxH` header - starts an image-derived colorwork
        // block (see ast::Pattern::color_grid). Checked before comment
        // stripping would matter is irrelevant here since neither this nor
        // `ROW` lines use `#` at all - hex colors are written bare (no
        // leading `#`) specifically to avoid colliding with the comment syntax.
        if let Some(rest) = content.strip_prefix("COLORGRID:") {
            let dims = rest.trim();
            let (w_str, h_str) = dims
                .split_once('x')
                .ok_or(ParseError::UnexpectedEof("'WxH' after COLORGRID:"))?;
            let width: u32 = w_str
                .trim()
                .parse()
                .map_err(|_| ParseError::InvalidNumber(w_str.trim().to_string(), line))?;
            let height: u32 = h_str
                .trim()
                .parse()
                .map_err(|_| ParseError::InvalidNumber(h_str.trim().to_string(), line))?;
            tokens.push(Spanned {
                token: Token::ColorGridHeader { width, height },
                line,
            });
            tokens.push(Spanned {
                token: Token::Newline,
                line,
            });
            continue;
        }

        // `ROW n: <hex> <hex> ...` - one row of a colorwork block.
        if content.starts_with("ROW")
            && content[3..]
                .trim_start()
                .starts_with(|c: char| c.is_ascii_digit())
        {
            let rest = content[3..].trim_start();
            let (num_str, hex_str) = rest
                .split_once(':')
                .ok_or(ParseError::UnexpectedEof("':' after ROW n"))?;
            let index: u32 = num_str
                .trim()
                .parse()
                .map_err(|_| ParseError::InvalidNumber(num_str.trim().to_string(), line))?;
            let mut hex = Vec::new();
            for tok in hex_str.split_whitespace() {
                if tok.len() != 6 {
                    return Err(ParseError::InvalidNumber(tok.to_string(), line));
                }
                let value = u32::from_str_radix(tok, 16)
                    .map_err(|_| ParseError::InvalidNumber(tok.to_string(), line))?;
                hex.push([
                    ((value >> 16) & 0xFF) as u8,
                    ((value >> 8) & 0xFF) as u8,
                    (value & 0xFF) as u8,
                ]);
            }
            tokens.push(Spanned {
                token: Token::ColorRow { index, hex },
                line,
            });
            tokens.push(Spanned {
                token: Token::Newline,
                line,
            });
            continue;
        }

        // `DEF: name = <raw body>` is captured as a single token; the body
        // is stored unexpanded (see ast::Pattern::definitions).
        if let Some(rest) = content.strip_prefix("DEF:") {
            let (name, body) = rest
                .split_once('=')
                .ok_or(ParseError::UnexpectedEof("'=' in DEF line"))?;
            tokens.push(Spanned {
                token: Token::DefLine {
                    name: name.trim().to_string(),
                    body: body.trim().to_string(),
                },
                line,
            });
            tokens.push(Spanned {
                token: Token::Newline,
                line,
            });
            continue;
        }

        let mut chars = content.chars().peekable();
        while let Some(&c) = chars.peek() {
            match c {
                ' ' | '\t' => {
                    chars.next();
                }
                ',' => {
                    chars.next();
                    tokens.push(Spanned {
                        token: Token::Comma,
                        line,
                    });
                }
                '*' => {
                    chars.next();
                    tokens.push(Spanned {
                        token: Token::Star,
                        line,
                    });
                }
                '.' => {
                    chars.next();
                    tokens.push(Spanned {
                        token: Token::Dot,
                        line,
                    });
                }
                '(' => {
                    chars.next();
                    tokens.push(Spanned {
                        token: Token::LParen,
                        line,
                    });
                }
                ')' => {
                    chars.next();
                    tokens.push(Spanned {
                        token: Token::RParen,
                        line,
                    });
                }
                '!' => {
                    chars.next();
                    tokens.push(Spanned {
                        token: Token::Bang,
                        line,
                    });
                }
                '@' => {
                    chars.next();
                    tokens.push(Spanned {
                        token: Token::At,
                        line,
                    });
                }
                ':' => {
                    chars.next();
                    tokens.push(Spanned {
                        token: Token::Colon,
                        line,
                    });
                }
                '=' => {
                    chars.next();
                    tokens.push(Spanned {
                        token: Token::Equals,
                        line,
                    });
                }
                '~' => {
                    chars.next();
                    let mut hex = String::new();
                    for _ in 0..6 {
                        match chars.peek() {
                            Some(&d) if d.is_ascii_hexdigit() => {
                                hex.push(d);
                                chars.next();
                            }
                            _ => break,
                        }
                    }
                    if hex.len() != 6 {
                        return Err(ParseError::InvalidNumber(hex, line));
                    }
                    let value = u32::from_str_radix(&hex, 16)
                        .map_err(|_| ParseError::InvalidNumber(hex.clone(), line))?;
                    tokens.push(Spanned {
                        token: Token::HexColor([
                            ((value >> 16) & 0xFF) as u8,
                            ((value >> 8) & 0xFF) as u8,
                            (value & 0xFF) as u8,
                        ]),
                        line,
                    });
                }
                c if c.is_ascii_digit() => {
                    let mut num = String::new();
                    while let Some(&d) = chars.peek() {
                        if d.is_ascii_digit() {
                            num.push(d);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    let n = num
                        .parse::<u32>()
                        .map_err(|_| ParseError::InvalidNumber(num.clone(), line))?;
                    tokens.push(Spanned {
                        token: Token::Number(n),
                        line,
                    });
                }
                c if c.is_alphabetic() || c == '_' => {
                    let mut ident = String::new();
                    while let Some(&d) = chars.peek() {
                        if d.is_alphanumeric() || d == '_' {
                            ident.push(d);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    tokens.push(Spanned {
                        token: Token::Ident(ident),
                        line,
                    });
                }
                other => return Err(ParseError::UnexpectedChar(other, line)),
            }
        }
        tokens.push(Spanned {
            token: Token::Newline,
            line,
        });
    }
    tokens.push(Spanned {
        token: Token::Eof,
        line: 0,
    });
    Ok(tokens)
}
