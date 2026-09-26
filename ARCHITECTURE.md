# Architecture

This is the implementation reference for contributors - workspace layout,
DSL grammar, what's actually implemented vs. still stubbed, and testing
notes. If you just want to know what the project is and how to run it, see
[README.md](README.md) instead.

## Pipeline

```
Crochet DSL (.cgp) --parser--> AST --eval--> StitchGraph
                                                  |
                                    layout_ring / layout_flat_grid
                                                  |
                                             analyze_tension
                                                  |
        +---------------+---------------+--------+--------+
        |               |               |                 |
 export_svg_chart  export_color_   export_obj      print::generate_
   (2D chart)      chart_svg +    (3D armature/      pattern_pdf
                   legend text     Blender)        (tiled, printable)
```

Two pattern "modes" share this same pipeline:

- **Shaped patterns** - round/row stitch scripting (`8sc`, `(3sc, inc)*4`,
  increases/decreases, labels, custom stitch aliases). Tapered/circular 3D
  layout, tension analysis, stitch-symbol SVG charts.
- **Colorwork patterns** - a flat width x height grid of single-crochet
  stitches, one color per cell (graphgan/tapestry-crochet style), hand-painted
  from a blank canvas, imported from a photo, or generated from typed text.
  Flat-panel 3D layout, real yarn-color rendering, colored SVG charts, text
  legends with nearest-match color names, and tiled printable PDF export.

Since shaped-round stitches can also carry a per-stitch `~RRGGBB` color
(see the DSL grammar below), the two modes aren't as separate as they
sound - a shaped, tapered amigurumi can have color-changing stripes without
touching the colorwork grid at all.

Every other craft (cross stitch, knitting, quilting, diamond painting,
fuse beads, latch hook, Pixelhobby, pixel macrame, pixel art) uses a
second, simpler pipeline - no stitch graph or 3D layout, just a chart:

```
picture / text --resize + k-means (imageimport)--> ColorGrid
   --match to a color catalog, confetti cleanup--> Chart <--> .cgp / .oxs
                                                     |
       +-------------+--------------+----------------+-----------------+
       |             |              |                |                 |
  chart editor   export_chart_   render_image    materials_text /   print_crossstitch::
  (egui)         svg             (PNG)           shopping_list /    generate_cross_stitch_pdf
                                                  instructions_text
```

See "Crafts" below for how one `Chart` model serves all nine.

## Workspace layout

```
abyssal-thread/
├── Cargo.toml              workspace root
├── .cargo/
│   └── audit.toml          cargo-audit config: accepted/ignored advisories,
│                            each with a written justification (see
│                            "Dependency security" below) - cargo-audit only
│                            reads this path (or ~/.cargo/audit.toml), NOT a
│                            repo-root audit.toml, despite that being an easy
│                            file to put there by mistake
├── crates/
│   ├── core/               StitchKind, StitchGraph (petgraph), Vec3,
│   │                       ColorGrid, StitchGraph::from_color_grid,
│   │                       Craft (every craft, for the New Pattern picker)
│   ├── lang/                lexer, parser, AST, eval (DSL -> StitchGraph),
│   │                       custom-stitch alias + raw-geometry expansion
│   │                       (raw_def.rs), COLORGRID: block parsing +
│   │                       color_grid_to_dsl serializer
│   ├── layout/             layout_ring (tapered/circular) + layout_flat_grid
│   │                       (flat colorwork panels) + relax (mass-spring)
│   │                       + tension analysis
│   ├── export/             SVG chart export (shaped + colorwork), OBJ
│   │                       export, nearest-match color naming
│   ├── imageimport/        image loading, exact/aspect-locked resize,
│   │                       k-means color quantization (farthest-point seeding)
│   ├── crossstitch/        the chart crafts (named for the first one):
│   │                       chart.rs (Chart model - cells, part stitches,
│   │                       backstitch, knots, blends, boards, row gauge;
│   │                       confetti cleanup, shopping list), profile.rs
│   │                       (per-craft GridCraft settings), threads.rs +
│   │                       data/*.tsv (DMC, Anchor, Diamond Dotz, Perler,
│   │                       Hama, Artkal catalogs), color.rs (CIEDE2000),
│   │                       cgp.rs / oxs.rs (file formats), knit.rs
│   │                       (row instructions, float checks), quilt.rs
│   │                       (cutting list, yardage, assembly), export.rs
│   │                       (SVG, materials text, instructions)
│   └── app/                CLI + GUI binary
│       ├── assets/fonts/   bundled .ttf files (see "Text tab" below),
│       │                   embedded into the binary via include_bytes!
│       └── src/
│           ├── print.rs    multi-page tiled pattern PDF generation
│           │               + hand-off to the OS's default PDF viewer
│           ├── print_shaped.rs  same, for shaped (non-colorwork) patterns
│           ├── print_crossstitch.rs  same, for every chart craft: cover
│           │               page (info, key, shopping list), symbol-chart
│           │               pages, instructions pages
│           └── gui/        mod.rs (view tabs, undo/redo, DSL sync,
│                           autosave/crash recovery), grid.rs (shaped
│                           stitch grid), colorwork_grid.rs (paint grid:
│                           palette, fill/bucket, resize), viewport.rs
│                           (3D), image_import.rs (photo import +
│                           GridImportPayload), text_import.rs (font
│                           dropdown + bold/italic, per-line centering),
│                           fonts.rs (bundled FontFamily registry +
│                           OS font enumeration via font-kit),
│                           def_builder.rs (point-and-click DEF builder,
│                           alias + raw-geometry forms), recent_colors.rs (shared
│                           color-picker history), new_pattern.rs (the
│                           craft picker), crossstitch_grid.rs (chart
│                           editor + Materials tab for every chart craft)
├── .github/
│   ├── workflows/           ci.yml (check/test/build + fmt/clippy/audit as
│   │                        separate jobs, on every push and PR),
│   │                        release.yml (tagged builds for Win/macOS/Linux)
│   └── ISSUE_TEMPLATE/      bug_report.md
├── examples/
│   ├── sphere.cgp                  amigurumi sphere (inc/dec shaping)
│   ├── motif_with_attachment.cgp   labels, @attach, DEF line
│   └── shell_stitch.cgp            custom stitch alias expansion
├── screenshots/             GUI demo GIFs, referenced from README.md
├── LICENSE
├── SECURITY.md              private vulnerability reporting instructions
└── WHAT_TO_TEST.md          tester-facing "what to poke at", separate from
                              this file and README.md's polished pitch
```

## Try it

CLI (crochet shaped patterns only - `build` refuses chart `.cgp` files
with a clear message):
```bash
cargo run -p abyssal-thread -- build examples/sphere.cgp --svg sphere.svg --obj sphere.obj --tension
```
Parses the pattern, builds the stitch graph, lays it out in 3D, prints a
tension report, and writes both a chart and a Blender-importable OBJ armature.

GUI:
```bash
cargo run -p abyssal-thread
```
Opens with the New Pattern picker: choose a craft, then start from a
blank pattern, a picture, typed text, or a file (closing the picker leaves
a blank crochet colorwork canvas). For crochet, tabs across the top switch
between the Grid editor, 3D viewport, DSL text view, Image Import, and
Text - all five stay in sync with each other automatically. For chart
crafts the tabs are Chart, Source (the `.cgp` text), Image import, Text
and Materials.

## DSL grammar implemented so far

Shaped patterns:

| Syntax | Meaning |
|---|---|
| `8sc` | count + stitch abbreviation (`sc`, `hdc`, `dc`, `tr`, `ch`, `ss`, `inc`, `dec`) |
| `2sc, inc` | comma-separated sequence within a round |
| `(3sc, inc) * 4` | repeat group |
| `flo.sc` / `blo.sc` | front/back loop only |
| `fpost.dc` / `bpost.dc` | front/back post |
| `dc~ff0000` | per-stitch color (any stitch term, any round) |
| `anchor!` | label the next stitch as an attachment point |
| `@anchor` | attach the next stitch into a labeled point |
| `PATTERN: name` | optional header (recognized anywhere in the file) |
| `# comment` | line comments |
| `DEF: name = ops...` | custom stitch, alias form - `Nname` expands to `N` copies of the alias's own ops, with cycle detection |
| `DEF: name = 3ch, ss@1[%,%-4]` | custom stitch, *raw geometry* form - see `raw_def.rs`'s module doc for the full `%`/`%-N`/`%+N`/`@N` grammar |

Colorwork patterns:

| Syntax | Meaning |
|---|---|
| `COLORGRID: WxH` | starts a colorwork block, `W` stitches wide x `H` rows |
| `ROW n: hex hex ...` | one row's colors as bare 6-digit hex (no leading `#` - that already means "comment" elsewhere in the grammar) |

A pattern is either shaped rounds or a colorwork grid, not both - `eval()`
detects a `COLORGRID:` block and builds the graph via
`StitchGraph::from_color_grid` instead of the round/repeat logic entirely.

Chart crafts use their own line-based format in the same `.cgp` files -
`CRAFT:`, `FABRIC:`, `THREAD n:`, `XSTITCH: WxH`, one `ROW n:` of thread
numbers per row, plus optional `BOARD:`, `ROUND:`, `BLEND n:`,
`BSSTRANDS n:`, `HALF:`, `SPLIT:`, `QUARTER:`, `BACK:` and `KNOT:` lines.
The full grammar is in `crates/crossstitch/src/cgp.rs`'s module doc; a
`CRAFT:` line naming a chart craft is what routes a file there instead of
the crochet parser.

Each newline starts a new round/row in shaped mode. `inc` and `dec` consume
parent slots from the previous round the way real crochet shaping does
(`inc` shares one parent between two children, `dec` merges two parents
into one child) - see `crates/lang/src/eval.rs`.

**Safety limits:** a single `count`/repeat-`*N` literal is capped at 10,000
(`parser::MAX_LITERAL_COUNT`), and the overall pattern is capped at 500,000
stitches total (`eval::MAX_TOTAL_STITCHES`) - both return a clear parse
error rather than letting an obvious typo (`999999999shell`) or a
compounding multiplication (a `DEF` body's own repeat times a large
invocation count) hang or exhaust memory. Both limits have been exercised
against realistic large patterns (a 90,000-stitch multi-round pattern, a
499,900-stitch pattern right at the boundary, 300 custom-stitch invocations
at scale) to confirm they don't false-positive-reject legitimate large
patterns - see "Testing" below.

## Crafts

`abyssal_thread_core::Craft` lists every craft; `Craft::is_available`
gates which ones the GUI's New Pattern picker (`gui/new_pattern.rs`)
offers (all ten, currently). The craft isn't an app-wide mode - it's
derived from the loaded pattern on every recompile (a `.cgp` whose
`CRAFT:` line names a chart craft is that craft, an `.oxs` opens as cross
stitch, anything else is crochet), so opening a file never needs a mode
switch first.

**Cross stitch** (`crates/crossstitch`, `gui/crossstitch_grid.rs`,
`print_crossstitch.rs`): for a cross-stitch pattern, `dsl_source` holds the
chart's `.cgp` text (format documented in `crossstitch/src/cgp.rs`), which
is what keeps undo/redo/autosave identical across crafts. Image/Text import
reuse the crochet pipeline (resize + k-means) and then snap each color to
the nearest DMC or Anchor floss by CIEDE2000, optionally leaving the
background unstitched and merging "confetti" (patches smaller than N
squares) into their surroundings (`Chart::remove_confetti`).

Beyond full crosses the chart holds part stitches per square
(`Partial::Half` / `Split` (3/4 stitches, matching OXS part-stitch
directions 1-4) / `Quarters`), backstitch lines and French knots on grid
points in half-square units, and blended threads. All of it round-trips
through both `.oxs` (mapping documented in `oxs.rs`) and `.cgp`
(`HALF:`/`SPLIT:`/`QUARTER:`/`BACK:`/`KNOT:`/`BLEND n:`/`BSSTRANDS n:`
lines, documented in `cgp.rs`); OXS items we don't model (beads, daisy
and bugle lines, buttons...) are reported as import warnings. Skein
estimates add up thread per physical skein across stitch kinds, so a
blend counts toward both of its threads. The catalogs' provenance is in
`crossstitch/src/threads.rs`.

**Other grid crafts** - diamond painting, fuse beads, latch hook,
Pixelhobby, pixel macrame and pixel art - share the same `Chart` model
and editor. `Chart::craft` (a `profile::GridCraft`, written as the `.cgp`
`CRAFT:` value) selects a per-craft profile in `crossstitch/src/profile.rs`:
cell name, cell-size presets (`Fabric::count` is cells per inch for every
craft), color catalogs (DMC/Diamond Dotz drills, Perler/Hama/Artkal beads;
crafts with no catalog use free colors named by `nearest_color_name`),
board presets (`Chart::board`, `.cgp` `BOARD: WxH` - pegboards,
Pixelhobby baseplates) and packaging (bags/pixelsquares plus spare
allowance) that `Chart::shopping_list` turns into a buy list. Part
stitches, backstitch, knots and hoop sizing stay cross-stitch-only. The
editor can switch a chart between grid crafts, re-matching colors to the
new craft's catalog (`Chart::rematch_colors`), and exports PNG images;
OXS saving is limited to cross stitch and diamond painting, the crafts
the OXS spec defines. Bead/drill catalog provenance is in
`crossstitch/src/threads.rs`; Pixelhobby has no open color dataset, so
it uses free colors.

**Knitting** (colorwork charts) and **quilting** (pixel / half-square-
triangle quilts) are grid crafts too, with extra model support:

- `Fabric::count_y` is a separate row gauge, so cells can be non-square
  (`Fabric::cell_aspect`). The editor, PDF (`print::compute_tiling_rect`),
  SVG and PNG all draw `cw` x `ch` cells, finished size uses both gauges,
  and picture imports add rows so a photo keeps its shape at the gauge.
  `.cgp` stores it as a third `FABRIC:` token, OXS as `stitchesperinch_y`.
- Knitting charts number the way they're worked (`Chart::row_number` /
  `col_number`: row 1 at the bottom, stitch 1 at the right), with heavy
  every-10 lines counted the same way (`is_heavy_col_line` /
  `is_heavy_row_line`) and row numbers on the right edge of the PDF.
  `Chart::worked_in_round` (`.cgp` `ROUND: yes`) switches the reading
  direction; `knit.rs` writes row-by-row instructions and flags long
  floats and 3+ color rows for stranded work. Empty cells are the main
  color; "materials" are each yarn's share of the stitches.
- Quilt cells are finished squares; `Partial::Split` doubles as a
  half-square triangle (the editor's triangle tool). `quilt.rs` turns the
  chart into a cutting list (1/4 in seams; HSTs two at a time at + 7/8
  in), yardage from strips across 40 in of usable width, backing, binding
  and batting, and row-by-row assembly; `Chart::board` optionally groups
  squares into blocks.

Written instructions (`export::instructions_text`) appear in the editor,
the Materials tab, the exported legend text and as extra PDF pages.

## What's real vs. stubbed

**Working - shaped patterns:** DSL lexer/parser/eval, stitch graph with
parent+sequence edges, labels/attachment points, increase/decrease
parent-slot logic, custom stitch expansion in both the alias form (`DEF:`
with cycle detection) and CrochetPARADE-style raw geometry (`%`/`%-N`/
`%+N`/`@N`, `raw_def.rs`), per-stitch `~RRGGBB` color independent of the
colorwork grid, gauge-scaled ring-based 3D layout plus an optional
mass-spring relaxation pass (`layout::relax`) for asymmetric patterns,
tension-deviation analysis, standard-US-symbol SVG chart export
(`branch_glyph` for real inc/dec symbols, not a generic V), OBJ export, CLI.

**Working - colorwork patterns:** `COLORGRID:`/`ROW` DSL block with a
full round-trip serializer, flat-panel 3D layout, real per-stitch yarn
color (shown in both the SVG chart and the 3D viewport), text legend
export with hex codes and nearest-match color names (e.g. "Brick",
"Dusty Rose" - curated palette, not authoritative naming).

**Working - image import:** load a photo, resize to an exact target
stitch grid (independent width/height, no forced aspect ratio unless
explicitly locked), k-means color quantization with deterministic
farthest-point seeding (reliably finds small/rare color regions, e.g. a
few dozen accent-color pixels on a mostly-uniform background - naive
seeding missed these), gauge-based sizing (set width/height directly in
stitches, or set a desired finished size in inches and let gauge -
stitches/rows per 4 inches - compute the stitch grid for you, with a
live "approx finished size at this gauge" readout either way). "Send to
Grid editor" carries the original source photo (not just the already-
quantized grid) into the paint-grid tab, so resizing there later
re-renders from source instead of cropping/padding a frozen snapshot.
Hardened against a degenerate (0x0) source image - `resize_preserving_aspect`
clamps before dividing, rather than letting a NaN/infinity silently
saturating-cast into a multi-billion-pixel resize target.

**Working - text-to-pattern:** type words, pick a font three ways:
a bundled-family dropdown (embedded via `include_bytes!`, no file to hunt down),
a manual "browse for a font file" picker, or a live dropdown of every
font already installed on the system (`font-kit`, `gui/fonts.rs::enumerate_system_font_families`),
toggle Bold/Italic (selects one of a family's four embedded variants,
not a synthesized fake bold/italic), pick text/background color. Each
line is measured and centered independently, not the block as a whole.
Shares the exact same resize/quantize/gauge-sizing/"send to grid"
pipeline as image import - the rendered text is just another
`DynamicImage` as far as the rest of the app is concerned.

**Working - GUI (crochet):** five synchronized views (Grid / 3D / DSL /
Image Import / Text) sharing one underlying pattern; the app starts with
the New Pattern picker over a blank paintable colorwork canvas (what you
get if you close the picker) rather than a placeholder shaped pattern. The Grid tab auto-switches between a shaped stitch-abbreviation
editor and a Stitch-Fiddle-style click/click-drag paint grid depending on
which kind of pattern is loaded. The paint grid renders as a single GPU
texture with one interactive region (not one widget per cell - an earlier
per-cell-widget version locked up at tens of thousands of cells), has its
own Canvas Size section (same gauge/inches controls as Image Import) so
you can start a pattern from a blank canvas and size it without ever
touching the DSL or Image Import tabs, and supports "Fill All" (flood the
whole canvas with the selected color in one click) plus a Pixel/Bucket
paint-mode toggle (bucket fill flood-fills the connected same-color
region under the cursor). Palette supports adding (color picker) and
removing (right-click a swatch) colors, and shares a "recent colors" strip
(`gui/recent_colors.rs`) with the shaped grid's per-stitch color picker,
so a color picked in either editor is a one-click reuse in the other.
Resizing a pattern that came from a photo or from text re-renders from the
original source at the new size instead of cropping/padding; resizing a
hand-painted canvas crops/pads since there's no source to re-render from.
Export buttons (SVG/OBJ/legend/PDF) and a Print button live in the top
toolbar and adapt to whichever pattern type is loaded. Gauge/relax
`DragValue` controls debounce a drag gesture to one recompile on release
rather than one per pixel dragged, so adjusting them stays responsive on
larger patterns. Grid cells carry a combined hover tooltip (tension state,
color hex, custom-stitch origin as text) since tension/color are otherwise
both pure-hue channels stacked on one small widget.

**Working - custom-stitch round-trip fidelity:** every stitch tracks
`def_origin` - which `DEF` (if any) produced it, the innermost one for
nesting. The Grid tab shows a banner naming every custom stitch currently
in the pattern, italicizes derived cells (hover for detail), and appends a
specific status-bar note naming exactly which custom stitch(es) got
flattened to raw stitches the moment an edit anywhere actually does that.
Deliberately does *not* attempt to auto-reconstruct `Ndefname` invocations
on serialization - that would need per-invocation boundary/count tracking,
and getting it wrong on a partially-edited invocation risks silently
emitting the *wrong* stitch count, worse than an honest, loudly-flagged
flatten. See `StitchNode::def_origin`'s doc comment for the full reasoning.

**Working - DEF authoring UI:** the DSL tab has a point-and-click builder
(`gui/def_builder.rs`) covering both `DEF:` forms via a mode toggle:

- *Alias* - name it, add stitches from a dropdown in order, insert.
- *Raw geometry* (`%`/`%-N`/`%+N`/`@N` relative attachment, see
  `raw_def.rs`) - rather than a node/edge graph editor, this is a form:
  stitches are placed in order behind a running "cursor position" list
  (so `3ch` shows as occupying positions `0-2`, matching `raw_def.rs`'s
  own counting rule), and marking a stitch "relative/closing" exposes an
  optional primary-parent override plus a list of extra closing-edge
  references (self/N-before/N-after) built by picking a kind and a
  position rather than typing `%-N` by hand. The assembled body is
  live-validated against the real `raw_def::parse_raw_def` parser and
  `looks_like_raw_body` classifier before "Insert" is enabled, so nothing
  that wouldn't parse back in - or would silently misclassify as an alias
  for lacking a `%` anywhere - can be inserted.

**Working - crash recovery:** the live pattern is autosaved to a fixed
temp-directory path on every successful compile. On the next launch, if a
non-empty autosave differs from the fresh blank-canvas default, the app
offers to recover it before rendering anything else - independent of (and
a backstop for) the in-memory-only undo/redo history, which doesn't
survive a crash or force-quit.

**Working - printing:** "Export PDF..." and "Print..." generate a
multi-page pattern PDF tiled across a chosen page size (US Letter or A4,
with a configurable margin) - each page carries global row/column
reference numbers along its edges and a footer stating exactly which
stitches/rows it covers, so pages can be lined up and taped together, with
a final legend page for colors. "Print" hands the PDF to the OS's default
viewer (there's no cross-platform Rust API for talking to a printer
directly without separate native code per OS, so this - generate a real
PDF, let the OS's own Print dialog handle it - is the standard, reliable
approach). Includes an on-page reminder to select Color rather than
Grayscale/Black & White in the print dialog, since that's a very common
printer default and the most likely explanation if a printed pattern loses
its color (the PDF's actual color data was verified directly at the byte
level, not just visually). Shaped patterns print too (`print_shaped.rs`):
each stitch's abbreviation, colored by tension state or an explicit
per-stitch color when one's set, with round 0 at the *bottom* of the
chart (matching real bottom-up working order, the opposite of colorwork's
top-down photo convention) and a tension/color key page instead of a hex
legend.

**Working - chart crafts:** everything described under "Crafts" above:
the chart editor (full/part stitches, backstitch, knots, fill, erase,
triangle tool for quilts), picture/text import with catalog matching and
confetti cleanup, craft switching with color re-matching, board/block
splitting, `.cgp` and `.oxs` round-trips, SVG/PNG export, Materials tab,
PDF with cover page, key, shopping list and instructions pages. Covered by
unit tests in `crates/crossstitch` and `crates/app` (see "Testing"), and
PDFs were rendered and checked visually during development - but the GUI
side of the chart crafts hasn't had real-world use yet.

**Working - dependency security process:** `.cargo/audit.toml` lists
every currently-accepted advisory with a written justification per entry
(unmaintained-but-low-risk-transitive, or - for the one real vulnerability,
`RUSTSEC-2026-0187` in `lopdf` via `printpdf` 0.7 - a specific reachability
argument: this app only *writes* PDFs from scratch, nothing calls
`lopdf::Document::load`/`load_mem` on file input, so the unbounded-recursion
parsing bug the advisory describes has no code path to trigger through.
`cargo audit` (both locally and in CI's `audit` job) reads this file
automatically - no separate `--ignore` flags needed, and no advisory is
silently allowed without a reason recorded next to it. All five accepted
entries were re-checked against the current RustSec advisory database on
2026-09-10: the four unmaintained-crate entries (`derivative`, `instant`,
`paste`, `ttf-parser`) still have no patched version to move to, and
`RUSTSEC-2026-0187`'s status changed (see the `printpdf` TODO entry below)
but still has no drop-in fix - so the accepted list is unchanged, just
re-dated. The chart crafts added one dependency, `roxmltree` (read-only
XML parsing for `.oxs`, no dependencies of its own, no advisories); chart
files are also bounded (`cgp::MAX_CELLS`, one million cells) so a crafted
`.cgp`/`.oxs` can't request a gigantic allocation.

**Stubbed / TODO:**

- **Chart-craft gaps:** knitting charts are colorwork only (no knit/purl,
  cable or decrease symbols, no yarn yardage estimate - just each yarn's
  share); quilts have no borders or sashing, and only the two-at-a-time
  HST method; Pixelhobby, latch hook, pixel macrame and pixel art have
  no manufacturer color catalogs (no open Pixelhobby dataset exists), so
  they use named free colors - a user-loadable catalog CSV would fix that;
  pixel macrame has no cord-length estimate; OXS beads, daisy/bugle lines
  and other ornaments aren't modeled (reported on import); OXS save is
  cross stitch and diamond painting only. Pack sizes, quilt yardage
  assumptions and knitting gauge presets are common values, not
  universal ones - the app says so where it uses them.
- **DMC color data licensing:** `crossstitch/data/dmc.tsv` comes from an
  MIT-licensed package that itself took it from an unlicensed project
  (details in `threads.rs`); Cstitch's GPL-3.0-or-later DMC list is a
  cleaner-licensed alternative if that matters.

- **`printpdf` is pinned at 0.7, not upgraded.** `printpdf` 0.9.x still
  depends on the same vulnerable `lopdf` version the advisory above is
  about (upgrading to it would mean a real API migration for zero
  security benefit). Reviewed again 2026-09-10: `lopdf` 0.42.0+ does fix
  `RUSTSEC-2026-0187`, and `printpdf` 0.12.8 (current latest) does pull a
  fixed `lopdf` (`^0.44`) - so a fix exists upstream now, unlike when this
  entry was first written. But `printpdf` 0.12.x has also grown an
  `html`/`svg`/`azul-layout`-based rendering path alongside its original
  drawing API, and it's still unverified whether the specific low-level
  imperative calls `print.rs`/`print_shaped.rs` use (`PdfDocument::empty`,
  per-layer `add_line`/`use_text`, etc.) survived four major-version jumps
  in a compatible form - that needs an actual migration attempt with
  compilation and visual/byte-level PDF verification (see "Testing"
  below), not a blind version bump. Revisit as a dedicated pass.
- **No accessibility (AccessKit) support** - deliberately disabled, not
  merely absent. `accesskit` pulled in a vulnerable/unmaintained
  dependency chain on Linux (`quick-xml` et al.) with no corresponding
  accessibility work actually built on top of it yet, so it was dropped
  entirely rather than carried as dead weight with a live CVE surface.
  Revisit if/when real screen-reader/assistive-tech support becomes a
  goal - re-adding it is straightforward (it's an `eframe` feature flag),
  the tradeoff was purely "unused now" vs. "security exposure now."

## Testing

`cargo test --workspace` runs unit tests across the parser (stitch
counts, repeats, modifiers, labels/attach, custom-stitch expansion, cycle
detection, the count-cap safety limits), eval (increase/decrease
parent-slot math, colorwork graph construction, raw-geometry expansion,
def_origin tagging, the compounding-blowup guard, and the three
realistic-large-pattern exercises described above), layout (flat-grid
placement, relax's spring/repulsion behavior including the spatial-grid
rewrite), export (color naming, SVG row-order regression tests), image
import (resize/quantization regression tests, including the
farthest-point-seeding fix and the degenerate-image guard), print
(tiling-math regression tests - exact page counts pinned against
visually-verified output, plus a "every cell covered exactly once, no
gaps" invariant check), print_shaped (`stitch_print_rgb` coverage plus
end-to-end smoke tests that generate real PDF files and check for
non-trivial output), def_builder (name validation and both alias/raw-geometry
DEF-line formatting extracted into pure functions, plus round-trip tests
through the real `raw_def::parse_raw_def` parser and the full DSL parser
for each form), crossstitch (CIEDE2000 against Sharma et al.'s published
reference pairs, every catalog parsing with unique codes, exact `.cgp` and
`.oxs` round-trips of every stitch kind and every craft, OXS import of a
MacStitch-style file, confetti cleanup, hoop fit, skein/pack/yardage
math, knitting row instructions and float detection, quilt cutting and
HST pairing), the chart editor's tool logic (3/4, quarter, backstitch,
knot, erase, quilt triangle - driven with synthetic pointer input, no
window), print_crossstitch (a full multi-page PDF with every stitch kind,
backstitch clipping at page edges, instruction wrapping), and fonts (deterministic bundled-`FontFamily` data checks - valid
TTF/OTF headers, correct `bytes_for` variant selection, unique names -
plus environment-tolerant tests for the `font-kit` system enumeration
functions that don't assume anything about what's actually installed on
the machine running them) - plus golden tests in
`crates/lang/tests/golden.rs` that compile the bundled example `.cgp`
patterns end-to-end and check the resulting stitch graph's shape.

CI (`.github/workflows/ci.yml`) runs `cargo test` (plus `check`/`build`),
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --
-D warnings`, and `cargo audit` (reading `.cargo/audit.toml` automatically)
as four separate jobs on every push and PR.

## Suggested next milestones

1. Attempt the `printpdf` 0.7 -> 0.12 migration now that a fixed `lopdf`
   is actually reachable through it - see the `Stubbed / TODO` entry
   above. This is a real, compile-and-verify migration (new drawing API,
   possibly a different crate feature set), not a version-number edit.
2. More bundled font families (`gui/fonts.rs`) if the current curated set
   feels limiting - system-font enumeration via `font-kit` already covers
   "use whatever's installed" for anyone who wants that instead.
3. Periodically review `.cargo/audit.toml`'s accepted-advisory list - each
   entry there is a decision made under specific conditions (no available
   fix, or a specific reachability argument); worth re-checking those
   conditions still hold rather than letting the list grow stale.
