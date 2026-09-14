# `print.rs` / `print_shaped.rs` - PDF generation

Two parallel files: `print.rs` for colorwork patterns, `print_shaped.rs`
for shaped patterns. Both build a multi-page tiled PDF via `printpdf`
(pinned at 0.7 - see
[10-troubleshooting.md](10-troubleshooting.md#printpdf-pinned-at-07) for
why) and both reuse the *same* tiling math (`print::compute_tiling`,
`print::PageSize`) - the tiling problem ("how many cells fit on a page,
how many pages do I need") only cares about a width/height in cells, not
what's actually drawn in each cell, so it's factored out once and shared.

## Why a PDF at all (no direct "print" API)

There is no cross-platform Rust API for "send this to whatever printer the
user has" without separate native code per OS (Windows print spooler, macOS
print panel, CUPS on Linux). The pattern both files use - and the one worth
remembering any time "printing" comes up in a cross-platform Rust app - is:
**generate a real file in a standard format, then hand it to the OS's own
default viewer for that format via `opener::open`**, and let that viewer's
already-existing, already-correct Print button do the actual printing. This
sidesteps writing (and maintaining) three different native printing
integrations entirely.

## `compute_tiling` - the pure math, tested in isolation

```rust
pub fn compute_tiling(grid_width, grid_height, cell_mm, page: PageSize, margin_mm) -> TilingPlan
```

Deliberately kept as pure arithmetic with no PDF library calls at all - the
doc comment calls this out directly: it's "the part most likely to have an
off-by-one bug," so it gets its own tests
(`tiling_matches_known_good_layouts`, pinned against *visually verified*
real PDF output, and `tiling_covers_every_cell_exactly_once_with_no_gaps`,
a general invariant check) that don't need to open or inspect a PDF file at
all. **This is a broadly useful pattern**: when a function has a
error-prone numeric core wrapped in a much larger side-effecting operation
(here: writing an actual PDF), extract the numeric core as its own
pure function and test *that* directly - you get fast, precise tests for
the part most likely to be wrong, without needing to parse your own output
format to verify it.

`usable_w`/`usable_h` subtract margin and label-strip space before dividing
by `cell_mm` - this is the part that would be easy to get subtly wrong (an
extra `+1` or forgetting the footer strip), which is exactly why it's
pinned against real, eyeballed output rather than derived a second time
independently in the test.

## `generate_pattern_pdf` (colorwork) vs. `generate_shaped_pattern_pdf` (shaped)

Both loop `for py in 0..pages_y { for px in 0..pages_x { ... } }`, draw a
tile's worth of cells, grid lines, axis-reference numbers (every
`AXIS_LABEL_INTERVAL = 10` cells, so tiles can be aligned and taped
together), and a footer captioning exactly which stitches/rows/rounds that
page covers. The **content of a cell** differs (colorwork: a filled
rectangle in the cell's actual color; shaped: the stitch's abbreviation
text, tinted by tension or explicit color via `stitch_print_rgb`) - that's
the entire difference between the two files' main loops.

**The one deliberate convention difference, worth remembering**: colorwork
draws row 0 at the top (matches the source photo - same reasoning as
`export::colorgrid`, see [04-export-crate.md](04-export-crate.md)); shaped
draws round 0 at the **bottom** (matches real bottom-up crochet working
order, same reasoning as `export::svg`). `print_shaped.rs`'s module doc
even names its own local variable `rft` ("row from top") specifically to
keep top-down pagination indexing separate from bottom-up round numbering
inside the same loop - a good small naming trick when a function has to
juggle two different "which direction is index 0" conventions at once.

## `print_via_system_default` / `print_shaped_via_system_default`

Thin wrappers: generate to a sanitized temp filename
(`sanitize_filename`/the inline equivalent in `print_shaped.rs` - both
replace anything non-alphanumeric with `_`, since a pattern name is
free-text and might contain filesystem-unsafe characters), then
`opener::open(&path)`. This is literally what "Print..." does in the GUI -
there's no separate print dialog logic anywhere else, it's this one call.

## The filet instructions integration

`generate_pattern_pdf` takes an `Option<&str>` `instructions` parameter -
when `Some` (filet mode, the text from
`export::filet::write_instructions`), `add_instructions_pages` paginates it
onto extra pages after the legend page, computing
`lines_per_page` from a fixed `INSTRUCTIONS_LINE_HEIGHT_MM` the same way
`compute_tiling` computes cells-per-page. If you extend filet support with
more written content (e.g. a stitch-count summary line, per-color totals),
this is the function to extend - it already handles arbitrary-length text
spilling across as many pages as needed.

## The "select Color, not Grayscale" note

Both PDFs (well, colorwork's does; it's the one with an actual color
legend) print a small red reminder line on the first page: printer drivers
commonly default to Grayscale/Black & White, and there is **no way for a
PDF to override that** - it's a print-time dialog choice, not a document
property. The comment in `print.rs` notes this was verified by checking
the PDF's actual content-stream color operators directly (real distinct
R/G/B fill operators), not just "it looks colored on screen" - worth
remembering as a debugging technique if a print-output bug ever comes up:
when the *rendered* PDF looks right but the *printed* output doesn't,
check what's actually in the print dialog/driver settings before assuming
the file itself is wrong.
