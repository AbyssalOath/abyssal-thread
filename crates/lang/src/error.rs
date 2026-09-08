#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unexpected character '{0}' at line {1}")]
    UnexpectedChar(char, usize),
    #[error("unexpected token {0:?} at line {1}")]
    UnexpectedToken(String, usize),
    #[error("unexpected end of input, expected {0}")]
    UnexpectedEof(&'static str),
    #[error("invalid number literal '{0}' at line {1}")]
    InvalidNumber(String, usize),
    #[error("attachment label '{0}' referenced before it was defined")]
    UnknownLabel(String),
    #[error("custom stitch definition '{0}' is cyclic (refers to itself, directly or indirectly)")]
    CyclicDefinition(String),
    #[error(
        "count {0} at line {1} exceeds the maximum of {2} for a single stitch/repeat count - \
         split it into multiple smaller repeats or invocations"
    )]
    CountTooLarge(u32, usize, u32),
    #[error(
        "pattern would produce at least {0} stitches, over the safety limit of {1} - check for \
         an unintentionally large repeat count or custom-stitch invocation"
    )]
    PatternTooLarge(usize, usize),
}
