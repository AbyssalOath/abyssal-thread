# `crates/crossstitch` - the chart crafts

Named for the first craft it supported, but it now holds the model for
**every non-crochet craft**: cross stitch, knitting, quilting, diamond
painting, fuse beads, latch hook, Pixelhobby, pixel macrame and pixel art.
Depends on `core` (for `ColorGrid` and `Craft`) and `export` (only for
`nearest_color_name`), plus `roxmltree` for reading `.oxs`. No GUI and no
PDF code in here - those live in `app` (`gui/crossstitch_grid.rs`,
`print_crossstitch.rs`, see [07-app-gui.md](07-app-gui.md) and
[08-print-pdf.md](08-print-pdf.md)).

The big idea: **these nine crafts are all "a grid of colored cells."** They
differ in what a cell is called, how big it is, which colors you can buy,
which extra tools make sense, and how materials are sold. So there's one
data model (`Chart`) plus one table of per-craft settings (`GridCraft` in
`profile.rs`), instead of nine separate code paths. Adding a tenth grid
craft is mostly adding a row to each `match` in `profile.rs`.

```
picture/text --(imageimport: resize + k-means)--> ColorGrid
    --Chart::from_color_grid (catalog match) + remove_confetti--> Chart
Chart <--cgp.rs / oxs.rs--> file text
Chart --export.rs / knit.rs / quilt.rs--> SVG, materials text, instructions
```

## `chart.rs` - `Chart`, the one model

```rust
pub struct Chart {
    pub name: Option<String>,
    pub craft: GridCraft,              // which profile applies
    pub board: Option<(usize, usize)>, // pegboard / baseplate / quilt block size
    pub worked_in_round: bool,         // knitting only
    pub width: usize, pub height: usize,
    pub fabric: Fabric,                // cells per inch (+ row gauge), background color
    pub palette: Vec<Floss>,           // the colors, each with a chart symbol
    pub cells: Vec<Option<u16>>,       // full cells: None = empty, Some(i) = palette[i]
    pub partials: BTreeMap<(usize, usize), Partial>, // part stitches, keyed (y, x)
    pub backstitches: Vec<Backstitch>,
    pub knots: Vec<Knot>,
}
```

Things worth noticing:

- **`Option<u16>` cells, not colors.** A cell points *into* the palette.
  That's what makes "remove this color" (`remove_floss`) and "re-match
  every color to Hama beads" (`rematch_colors`) cheap: rewrite the
  palette and remap indices, never touch colors cell by cell. Both go
  through one helper, `remap_floss`, which also walks partials,
  backstitches and knots - if you add another kind of thing that
  references a palette index, add it there or it'll silently point at the
  wrong color after a removal.
- **`partials` is a sparse `BTreeMap`**, not a second `Vec` the size of the
  grid - most charts have few part stitches. Keyed `(y, x)` (row first) so
  iterating it goes in row order, and `range((row0, 0)..(row1, 0))` finds
  the visible rows' part stitches without scanning everything (the editor
  and PDF both do this). A square holds either a full cell *or* partials,
  never both - `set` and `set_partial` each clear the other.
- **Grid-line coordinates are in half-square units** (`Backstitch::from`,
  `Knot::at`): `(0,0)` is the chart's top-left corner, `(1,1)` the middle
  of the first square. This matches OXS, which allows `.5` positions and
  nothing finer, and keeps equality exact (integers, not `f32`).
- **`Partial::Split` does double duty**: a cross-stitch 3/4 stitch *and* a
  quilting half-square triangle are both "a square split along a diagonal
  into two colored triangles." Same data, same editor tool, same PDF
  drawing - only the words differ.
- **`Fabric::count_y`** is the row gauge. `None` means square cells (every
  craft except knitting). Everything that turns cells into inches or
  pixels asks `Fabric::rows_per_inch()` / `cell_aspect()` rather than
  assuming square.

### Confetti cleanup (`remove_confetti`)

Photo imports scatter lone cells of in-between colors along edges -
"confetti," each one a separate thread start/stop in cross stitch. The
algorithm is a nice small example of **decide-then-apply**: it labels
4-connected same-value regions (flood fill with an explicit stack), and
for every region smaller than `min_size` it counts the values of its
8-neighbors *in the original chart* and picks the most common one. All
decisions are collected into a `changes` list and applied at the end, so
the result doesn't depend on which region got visited first. If you
mutated in place during the scan, an early change could alter a later
region's neighbor counts - a classic order-dependence bug.

### Materials: `usage`, `shopping_list`

`usage()` counts every stitch kind per palette entry. `shopping_list()`
then branches by craft: cross stitch converts thread *length* into skeins
(each stitch kind uses a different length - see the constants near
`thread_mm_for`, and a blend counts toward both of its threads); pack
crafts (beads, drills, Pixelhobby) divide counts by a pack size plus a
spare allowance from the profile; knitting reports each yarn's share of
the stitches; quilting hands off to `quilt.rs`. `ShoppingItem::buy_text`
turns any of those into one display string, so the GUI and PDF don't need
to know which kind they're showing.

## `profile.rs` - `GridCraft`, the per-craft table

`GridCraft` is a plain enum, and every craft-specific question is a method
with a `match`: `cell_word()` ("stitch"/"bead"/"square"), `sizes()`
(presets like "14-count Aida" or "DK (22 sts x 30 rows / 4 in)"),
`catalogs()`, `has_part_stitches()`, `has_triangles()`, `boards()`,
`packaging()`, `default_skip_background()`, and so on. The module doc
lists where the physical numbers came from (Pixelhobby plate sizes, the
Craft Yarn Council's yarn-weight ranges, etc.).

This is a deliberate alternative to a `trait Craft` with nine `impl`
blocks: with an enum, "what does every craft do for X" is one `match` you
can read top to bottom, and the compiler forces you to handle every craft
when you add a new method. The trait version spreads that across nine
places. For a closed set of variants that you control, enum + `match`
usually reads better.

## `threads.rs` + `data/*.tsv` - color catalogs

Each catalog is a tab-separated file (`code<TAB>HEX<TAB>name`) embedded
with `include_str!` and parsed once, lazily, into a `static` via
`OnceLock` (see [09-rust-patterns-glossary.md](09-rust-patterns-glossary.md)).
`Catalog::nearest` ranks by **CIEDE2000** (`color.rs`), not RGB distance.
The module doc records where every data file came from and its license.

Brand strings are single tokens (`Perler-Mini`, not `Perler Mini`)
because `.cgp` `THREAD` lines are split on whitespace - a small example of
a file-format constraint leaking into data naming, documented where it
matters rather than worked around later.

## `color.rs` - sRGB → Lab, CIEDE2000

A straight implementation of the standard formulas, computed in `f64`
internally (the hue-angle wraparound terms lose visible precision in
`f32`). Tested against the published reference pairs from Sharma, Wu &
Dalal (2005). If you ever touch it, keep those tests: a formula like this
can be subtly wrong while still producing plausible-looking numbers,
which is exactly what a test against known answers catches.

## `cgp.rs` and `oxs.rs` - file formats

- **`.cgp` (`cgp.rs`)** - the app's own line-based text format, same
  file extension as crochet DSL. A `CRAFT:` line naming a chart craft is
  what routes a file here (`is_chart_source`) instead of to the crochet
  parser. The module doc is the grammar reference. Worth studying: the
  parser collects things it can't check yet (part stitches, blends,
  backstitches - they need the chart size and palette) into a `Pending`
  list with their line numbers, then validates them after everything is
  read, so errors still point at the right line.
- **`.oxs` (`oxs.rs`)** - Ursa Software's XML interchange format
  (MacStitch/WinStitch). The module doc maps each OXS element to our
  model, per the published spec. Reading is lenient (writers disagree on
  details, so e.g. `symbol` is only kept when it's unambiguous); anything
  we don't model becomes a warning string rather than being dropped
  silently. Writing is plain `format!` with XML escaping - no XML-writer
  dependency needed for a format this simple.

Both formats have exact round-trip tests (`to_x` then `parse_x` gives back
an equal `Chart`) covering every stitch kind. That's the cheapest way to
keep a serializer and parser in agreement: if you add a field to `Chart`,
add it to the round-trip test chart and the test tells you which side you
forgot.

## `knit.rs` and `quilt.rs` - craft-specific instructions

- **`knit.rs`** reads each row in *working order* (`row_in_working_order`:
  right-to-left on right-side rows, left-to-right on wrong-side rows when
  worked flat), run-length-encodes it (`runs`), and writes "Row 2 (WS): 7
  MC, 2 A, ...". `longest_float` finds how far each yarn is carried
  behind the work between uses - the thing that snags in stranded
  knitting.
- **`quilt.rs`** turns the chart into a quilting plan: counts plain
  squares per fabric, pairs up half-square triangles by fabric pair (two
  units per pair of cut squares), converts pieces into strips across the
  fabric width and then yards (rounded up to 1/8 yd), and adds backing,
  binding and batting. `fmt_eighths` prints measurements the way a
  quilting ruler reads ("2 7/8").

Both are pure functions over a `Chart` with no GUI or PDF knowledge;
`export::instructions_text` picks the right one, and the editor, Materials
tab, text export and PDF all call that.

## `export.rs` - SVG, info lines, materials text

`info_lines` is the shared "design summary" (size, fabric, hoop, boards,
gauge...) worded per craft, so the Materials tab, the PDF cover page and
the text export can't drift apart. `export_chart_svg` draws cells as
`cw` x `ch` rectangles (non-square for knitting) plus part stitches,
backstitch and knots. The drawing-geometry helpers (`split_triangle`,
`half_stitch_ends`, `quarter_origin`) are shared with the editor and the
PDF, so all three agree on which triangle is "first."
