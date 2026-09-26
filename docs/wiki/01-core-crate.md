# `crates/core` - the shared data model

No dependencies on any other workspace crate. Everything else depends on
this. If you're adding a wholly new *kind* of data the pattern needs to
carry (a new stitch property, a new grid concept), it probably starts here.

## `stitch.rs` - `StitchKind`

```rust
pub enum StitchKind {
    Chain, SlipStitch, SingleCrochet, HalfDoubleCrochet, DoubleCrochet, TrebleCrochet,
    Increase(Box<StitchKind>),
    Decrease(Box<StitchKind>),
    FrontLoopOnly(Box<StitchKind>),
    BackLoopOnly(Box<StitchKind>),
    FrontPost(Box<StitchKind>),
    BackPost(Box<StitchKind>),
    Custom(String),
}
```

This is a **recursive enum** - `Increase`/`Decrease`/the loop-and-post
modifiers all wrap an *inner* `StitchKind` rather than being flat variants.
That's what lets `fpost.dc` and `flo.sc` exist as a single value instead of
a second "modifier" field bolted onto every stitch: `FrontPost(Box::new(DoubleCrochet))`
*is* "a double crochet, worked as a front-post." The `Box` is required -
see [09-rust-patterns-glossary.md](09-rust-patterns-glossary.md#recursive-enums-need-box)
for why Rust forces that.

Every method on `StitchKind` recurses into the wrapped kind for anything
that's about the *base* stitch (`chart_symbol`, `base_abbrev`,
`baseline_width_mm`/`baseline_height_mm`), and has its own separate check
for the wrapper itself (`modifier_symbol`, `modifier_prefix`,
`is_increase`/`is_decrease`). This split exists because real crochet charts
layer a modifier mark *on top of* the base stitch's symbol rather than
replacing it - `fpost.dc` draws as a "T" (dc's symbol) plus a curl mark, not
some entirely different glyph. If you add a new modifier, follow this same
pattern: recurse for the base-stitch behavior, add a parallel case for the
modifier-specific behavior.

`from_abbrev` is the DSL → `StitchKind` boundary - anything not in its match
arms becomes `Custom(name)`, which is what lets user-defined stitch names
(`DEF: shell = ...`) round-trip without `core` needing to know about `lang`'s
`DEF` mechanism at all. `core` never imports `lang` - this is how that
one-way dependency stays clean.

`baseline_width_mm`/`baseline_height_mm` are hardcoded worsted-weight-yarn
guesses, the "gauge" every downstream measurement (3D spacing, tension
deviation) is relative to. `layout::Gauge` (see
[03-layout-crate.md](03-layout-crate.md)) scales these up/down for an actual
swatch gauge rather than replacing them.

## `graph.rs` - `StitchGraph`

The central type. A thin domain-specific wrapper around
`petgraph::graph::DiGraph<StitchNode, StitchEdge>`:

```rust
pub struct StitchGraph {
    pub graph: DiGraph<StitchNode, StitchEdge>,
    pub labels: HashMap<String, NodeIndex>,
    pub rounds: Vec<Vec<NodeIndex>>,   // stitch order, grouped by round
    pub warnings: Vec<String>,
}
```

- **`StitchNode`** - one stitch instance: `kind`, `round`, `index_in_round`,
  optional `label`, `position` (filled by layout), `tension` (filled by
  tension analysis), `color` (colorwork/`~hex`), `def_origin` (which `DEF`
  produced it, for round-trip warnings - see `def_origin`'s own doc comment
  at `crates/core/src/graph.rs:24`, it's genuinely one of the more
  interesting design notes in the codebase).
- **`StitchEdge`** - just two variants: `Sequence` (worked immediately
  after, same round - left/right neighbor) and `Parent` (worked *into* -
  the previous round's stitch this one attaches to). Every 3D-layout spring,
  every chart-drawing decision, every tension check is really just "walk
  `Sequence` edges" or "walk `Parent` edges."
- **`rounds`** is redundant with what you could derive by filtering
  `graph.node_weights()` by `.round`, but it's kept as an explicit
  `Vec<Vec<NodeIndex>>` because *stitch order within a round* matters
  constantly (chart drawing, grid-cell building, layout angle-per-stitch)
  and petgraph doesn't guarantee node iteration order. This is a classic
  "graph library gives you connectivity, you still need your own ordering
  index on top" pattern.
- **`warnings`** - non-fatal issues (currently only: a raw `DEF` body's
  `%-N`/`%+N` reference that resolved out of range). The pattern here is
  "attach a warning to the *value* being built rather than making the
  builder function return `Result<T, SomeWarningType>` alongside its real
  error type" - keeps the `ParseError` type focused on things that actually
  abort compilation, while nothing gets silently dropped either.

Key methods: `add_stitch` (pushes into both `graph` and `rounds`, keeping
them in sync - this is the only place a node should be created, so if you
add a new way to build a `StitchGraph`, route it through this),
`parent_of`/`sequence_neighbors` (edge-direction-aware queries, used
everywhere downstream instead of raw `petgraph` calls).

## `colorwork.rs` - the shaped/colorwork bridge

`StitchGraph::from_color_grid` is the *only* place a `ColorGrid` becomes a
`StitchGraph`: one `SingleCrochet` per cell, `Sequence` edges left-to-right
within a row, `Parent` edges straight down to the same column in the row
above. Row 0 has no parents (it's the foundation row). This is
intentionally the simplest possible bridge - see the module doc for why a
colorwork "stitch" is always plain `sc` (real graphgan/tapestry crochet
*is* single crochet throughout; a filet mesh is a different structure
entirely and deliberately does **not** go through this - see
`export::filet`, [04-export-crate.md](04-export-crate.md)).

## `geometry.rs` - `Vec3`

Minimal hand-rolled 3D vector math (`+`, `-` via `std::ops` trait impls,
`dot`, `cross`, `on_circle` for ring placement, `normalized` with a
NaN-avoiding zero-length guard). No `nalgebra`/`glam` dependency - deliberate,
to keep `core` dependency-light. `on_circle` is the one function worth
knowing by name: it's the entire trigonometry behind `layout_ring`'s
placement (see [03-layout-crate.md](03-layout-crate.md)).

## `craft.rs` - `Craft`

```rust
pub enum Craft { Crochet, CrossStitch, Knitting, DiamondPainting, FuseBeads,
                 PixelMacrame, LatchHook, Quilt, PixelHobby, PixelArt }
```

Every craft the app knows about, with a display `label()`, a one-line
`blurb()` for the New Pattern picker's cards, and `is_available()` (the
switch that let the picker show crafts as "coming soon" while they were
being built - all ten are available now). It lives in `core`, not in the
GUI or the chart crate, because it's shared vocabulary: the GUI picks one,
and `crossstitch::GridCraft::craft()` maps each chart craft back to it.
Crochet is the only `Craft` without a `GridCraft` - that's how the app
tells "use the crochet pipeline" from "use the chart pipeline."

## `colorgrid.rs` - `ColorGrid`

```rust
pub struct ColorGrid { pub width: usize, pub height: usize, pub cells: Vec<[u8; 3]> }
```

Row-major flat `Vec`, row 0 first. Deliberately **not** unified with
`StitchGraph`/`StitchKind` - see the module doc's reasoning: the DSL has no
per-stitch color syntax for a *grid*, colorwork is its own simple type. If
you ever want the DSL to express `sc(#fdd83f)`-style per-stitch color
folded into `StitchNode` directly, that's the noted "natural next step" -
but it isn't done today, and `ColorGrid` staying separate is why colorwork
and shaped patterns can have such different tooling (paint grid vs.
stitch-abbreviation grid) without either one leaking into the other's code
path.

`ColorGrid` is also the hand-off point for the chart crafts: picture and
text import produce a `ColorGrid` exactly as they do for crochet
colorwork, and `crossstitch::Chart::from_color_grid` takes it from there
(see [11-crossstitch-crate.md](11-crossstitch-crate.md)).
