use serde::{Deserialize, Serialize};

/// The kind of stitch a node represents. `Custom` covers user-defined
/// stitches (e.g. a picot defined via `DEF: p = ...` in the language crate) -
/// the geometry for those is resolved by the definition's expansion, not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StitchKind {
    Chain,
    SlipStitch,
    SingleCrochet,
    HalfDoubleCrochet,
    DoubleCrochet,
    TrebleCrochet,
    Increase(Box<StitchKind>),
    Decrease(Box<StitchKind>),
    FrontLoopOnly(Box<StitchKind>),
    BackLoopOnly(Box<StitchKind>),
    FrontPost(Box<StitchKind>),
    BackPost(Box<StitchKind>),
    Custom(String),
}

impl StitchKind {
    /// Parse the short abbreviations used in the DSL (sc, hdc, dc, tr, ch, ss, inc, dec ...).
    /// Returns `Custom(name)` for anything unrecognized so user-defined stitches still work.
    pub fn from_abbrev(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "ch" => StitchKind::Chain,
            "ss" | "sl" | "slst" => StitchKind::SlipStitch,
            "sc" => StitchKind::SingleCrochet,
            "hdc" => StitchKind::HalfDoubleCrochet,
            "dc" => StitchKind::DoubleCrochet,
            "tr" => StitchKind::TrebleCrochet,
            "inc" => StitchKind::Increase(Box::new(StitchKind::SingleCrochet)),
            "dec" => StitchKind::Decrease(Box::new(StitchKind::SingleCrochet)),
            other => StitchKind::Custom(other.to_string()),
        }
    }

    /// Approximate real-world stitch width in millimeters, used as the
    /// baseline for layout spacing and tension-deviation analysis.
    /// These are rough worsted-weight-yarn defaults; making this
    /// gauge-configurable (stitches/10cm from a swatch) is an obvious next step.
    pub fn baseline_width_mm(&self) -> f32 {
        match self {
            StitchKind::Chain => 5.0,
            StitchKind::SlipStitch => 3.0,
            StitchKind::SingleCrochet => 6.0,
            StitchKind::HalfDoubleCrochet => 7.0,
            StitchKind::DoubleCrochet => 8.0,
            StitchKind::TrebleCrochet => 9.0,
            StitchKind::Increase(inner) | StitchKind::Decrease(inner) => inner.baseline_width_mm(),
            StitchKind::FrontLoopOnly(inner)
            | StitchKind::BackLoopOnly(inner)
            | StitchKind::FrontPost(inner)
            | StitchKind::BackPost(inner) => inner.baseline_width_mm(),
            StitchKind::Custom(_) => 6.0,
        }
    }

    pub fn baseline_height_mm(&self) -> f32 {
        match self {
            StitchKind::Chain => 5.0,
            StitchKind::SlipStitch => 2.0,
            StitchKind::SingleCrochet => 5.0,
            StitchKind::HalfDoubleCrochet => 7.0,
            StitchKind::DoubleCrochet => 9.0,
            StitchKind::TrebleCrochet => 12.0,
            StitchKind::Increase(inner) | StitchKind::Decrease(inner) => inner.baseline_height_mm(),
            StitchKind::FrontLoopOnly(inner)
            | StitchKind::BackLoopOnly(inner)
            | StitchKind::FrontPost(inner)
            | StitchKind::BackPost(inner) => inner.baseline_height_mm(),
            StitchKind::Custom(_) => 5.0,
        }
    }

    /// The standard US crochet-chart symbol glyph for this stitch's *base*
    /// shape, used by the SVG exporter (`export_svg_chart`): oval for
    /// chain, dot for slip stitch, "+" for single crochet, and a "T" shape
    /// (rendered with zero/one/two yarn-over tick marks by the exporter)
    /// for half double/double/treble crochet. Loop-only/post modifiers
    /// don't get their own entry here - see `modifier_symbol()`. Nor does
    /// increase/decrease - see `is_increase`/`is_decrease`'s doc comment.
    pub fn chart_symbol(&self) -> &'static str {
        match self {
            StitchKind::Chain => "oval",
            StitchKind::SlipStitch => "dot",
            StitchKind::SingleCrochet => "plus",
            StitchKind::HalfDoubleCrochet => "t-small",
            StitchKind::DoubleCrochet => "t",
            StitchKind::TrebleCrochet => "t-tall",
            // Recurse to the inner kind's *base* shape, matching the
            // pattern `baseline_width_mm`/`baseline_height_mm`/`base_abbrev`
            // already use for these wrapper kinds - a back-post double
            // crochet is still, first and foremost, a "T" with one
            // yarn-over tick; the post modifier is layered on top of that
            // via `modifier_symbol()` below, not a wholesale replacement of
            // the base stitch's own symbol. Increase/decrease get the same
            // treatment for the same reason: "2 dc in one stitch" is two
            // real dc "T" symbols branching from one base, not a generic V
            // standing in for whatever the base stitch happened to be -
            // see `is_increase`/`is_decrease`.
            StitchKind::Increase(inner)
            | StitchKind::Decrease(inner)
            | StitchKind::FrontLoopOnly(inner)
            | StitchKind::BackLoopOnly(inner)
            | StitchKind::FrontPost(inner)
            | StitchKind::BackPost(inner) => inner.chart_symbol(),
            StitchKind::Custom(_) => "custom",
        }
    }

    /// Whether this is an increase (one parent stitch, two children) -
    /// tells the SVG exporter to draw two real copies of `chart_symbol()`'s
    /// base shape branching from a shared point (`export::svg::branch_glyph`)
    /// instead of treating "inc" as its own opaque symbol the way a plain
    /// generic V/inverted-V glyph would. The real convention charts use for
    /// "2 stitches worked into 1" is two full copies of whatever the base
    /// stitch's symbol actually is, sharing a base - not a shape that hides
    /// which stitch is being doubled.
    pub fn is_increase(&self) -> bool {
        matches!(self, StitchKind::Increase(_))
    }

    /// Mirrors `is_increase` for decrease (two parents, one child - two
    /// base-shape copies merging into a shared point instead of branching
    /// from one).
    pub fn is_decrease(&self) -> bool {
        matches!(self, StitchKind::Decrease(_))
    }

    /// A small additional mark drawn on top of `chart_symbol()`'s base
    /// shape for a loop-only/post modifier. Real crochet charts layer these
    /// onto the base stitch symbol rather than swapping it out, which is
    /// why this is a separate method instead of folding into
    /// `chart_symbol()` itself (see that method's doc comment). `None` for
    /// any kind with no modifier.
    pub fn modifier_symbol(&self) -> Option<&'static str> {
        match self {
            StitchKind::FrontLoopOnly(_) => Some("flo"),
            StitchKind::BackLoopOnly(_) => Some("blo"),
            StitchKind::FrontPost(_) => Some("fpost"),
            StitchKind::BackPost(_) => Some("bpost"),
            _ => None,
        }
    }

    /// Inverse of `from_abbrev` for the stitch's *base* kind, ignoring any
    /// loop/post modifier wrapper - used by the GUI grid editor to redraw
    /// DSL text from an edited grid cell. See also `modifier_prefix`.
    pub fn base_abbrev(&self) -> String {
        match self {
            StitchKind::Chain => "ch".to_string(),
            StitchKind::SlipStitch => "ss".to_string(),
            StitchKind::SingleCrochet => "sc".to_string(),
            StitchKind::HalfDoubleCrochet => "hdc".to_string(),
            StitchKind::DoubleCrochet => "dc".to_string(),
            StitchKind::TrebleCrochet => "tr".to_string(),
            StitchKind::Increase(_) => "inc".to_string(),
            StitchKind::Decrease(_) => "dec".to_string(),
            StitchKind::FrontLoopOnly(inner)
            | StitchKind::BackLoopOnly(inner)
            | StitchKind::FrontPost(inner)
            | StitchKind::BackPost(inner) => inner.base_abbrev(),
            StitchKind::Custom(name) => name.clone(),
        }
    }

    /// The `flo`/`blo`/`fpost`/`bpost` DSL prefix for this kind, if any.
    pub fn modifier_prefix(&self) -> Option<&'static str> {
        match self {
            StitchKind::FrontLoopOnly(_) => Some("flo"),
            StitchKind::BackLoopOnly(_) => Some("blo"),
            StitchKind::FrontPost(_) => Some("fpost"),
            StitchKind::BackPost(_) => Some("bpost"),
            _ => None,
        }
    }

    /// Short display text for grid-editor cells and palette buttons - the
    /// same text you'd type in the DSL (e.g. "flo.sc", "inc", "dc").
    pub fn display_label(&self) -> String {
        match self.modifier_prefix() {
            Some(prefix) => format!("{prefix}.{}", self.base_abbrev()),
            None => self.base_abbrev(),
        }
    }
}
