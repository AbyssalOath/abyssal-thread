/// Optional loop/post modifier applied to a base stitch abbreviation,
/// written as `flo.sc`, `blo.sc`, `fpost.dc`, `bpost.dc` in the DSL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modifier {
    FrontLoopOnly,
    BackLoopOnly,
    FrontPost,
    BackPost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// `8sc`, `inc`, `flo.sc` etc. `count` defaults to 1 when omitted.
    Stitch {
        modifier: Option<Modifier>,
        abbrev: String,
        count: u32,
        color: Option<[u8; 3]>,
    },
    /// `(3sc, inc) * 4`
    Repeat { body: Vec<Op>, times: u32 },
    /// `label!` marks the *next* stitch (or the group it precedes) as an
    /// explicit attachment point other rounds can refer to.
    Label(String),
    /// `@label` - attach the next stitch into the labeled point instead of
    /// positionally into the previous round.
    AttachTo(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Round {
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, Default)]
pub struct Pattern {
    pub name: Option<String>,
    pub rounds: Vec<Round>,
    /// Raw, *unexpanded* bodies of `DEF: name = ...` custom stitch
    /// definitions, keyed by name. Both supported forms - a plain-DSL
    /// "alias" body and CrochetPARADE-style raw stitch geometry (`%`
    /// self-reference, `ss@1[%,%-4]`-style relative-attachment brackets) -
    /// are expanded from this raw text lazily, at `eval::eval` time; see
    /// `raw_def`'s module doc for the raw-geometry grammar and
    /// `eval::expand_raw_def` for how it's turned into graph nodes/edges.
    pub definitions: Vec<(String, String)>,
    /// Present when the pattern is a `COLORGRID:`/`ROW` colorwork block
    /// (see lexer::Token::ColorGridHeader/ColorRow). Mutually exclusive
    /// with `rounds` in practice - a pattern is either shaped-DSL rounds or
    /// an image-derived color grid, not both (see eval::eval).
    pub color_grid: Option<abyssal_thread_core::ColorGrid>,
}
