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

## Workspace layout

```
abyssal-thread/
├── Cargo.toml              workspace root
├── crates/
│   ├── core/               StitchKind, StitchGraph (petgraph), Vec3,
│   │                       ColorGrid, StitchGraph::from_color_grid
│   ├── lang/               lexer, parser, AST, eval (DSL -> StitchGraph),
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
│   └── app/                CLI + GUI binary
│       ├── assets/fonts/   bundled .ttf files (see "Text tab" below),
│       │                   embedded into the binary via include_bytes!
│       └── src/
│           ├── print.rs    multi-page tiled pattern PDF generation
│           │               + hand-off to the OS's default PDF viewer
│           ├── print_shaped.rs  same, for shaped (non-colorwork) patterns
│           └── gui/        mod.rs (view tabs, undo/redo, DSL sync,
│                           autosave/crash recovery), grid.rs (shaped
│                           stitch grid), colorwork_grid.rs (paint grid:
│                           palette, fill/bucket, resize), viewport.rs
│                           (3D), image_import.rs (photo import +
│                           GridImportPayload), text_import.rs (font
│                           dropdown + bold/italic, per-line centering),
│                           fonts.rs (bundled FontFamily registry),
│                           OS font enumeration via font-kit),
│                           def_builder.rs (point-and-click DEF alias
│                           builder),
│                           recent_colors.rs (shared color-picker history)
├── .github/workflows/      ci.yml (test/fmt/clippy/audit on push),
│                           release.yml (tagged builds for Win/macOS/Linux)
└── examples/
    ├── sphere.cgp                  amigurumi sphere (inc/dec shaping)
    ├── motif_with_attachment.cgp   labels, @attach, DEF line
    └── shell_stitch.cgp            custom stitch alias expansion
```

## Try it

CLI (shaped patterns):
```bash
cargo run -p abyssal-thread -- build examples/sphere.cgp --svg sphere.svg --obj sphere.obj --tension
```
Parses the pattern, builds the stitch graph, lays it out in 3D, prints a
tension report, and writes both a chart and a Blender-importable OBJ armature.

GUI:
```bash
cargo run -p abyssal-thread
```
Opens directly into a blank paintable colorwork canvas. Tabs across the top
switch between the Grid editor, 3D viewport, DSL text view, Image Import, and
Text - all five stay in sync with each other automatically.

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

Each newline starts a new round/row in shaped mode. `inc` and `dec` consume
parent slots from the previous round the way real crochet shaping does
(`inc` shares one parent between two children, `dec` merges two parents
into one child) - see `crates/lang/src/eval.rs`.

**Safety limits:** a single `count`/repeat-`*N` literal is capped at 10,000
(`parser::MAX_LITERAL_COUNT`), and the overall pattern is capped at 500,000
stitches total (`eval::MAX_TOTAL_STITCHES`) - both return a clear parse
error rather than letting an obvious typo (`999999999shell`) or a
compounding multiplication (a `DEF` body's own repeat times a large
invocation count) hang or exhaust memory.

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

**Working - GUI:** five synchronized views (Grid / 3D / DSL / Image
Import / Text) sharing one underlying pattern; the app starts directly
in a blank paintable colorwork canvas rather than a placeholder shaped
pattern. The Grid tab auto-switches between a shaped stitch-abbreviation
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
(`gui/def_builder.rs`) for the *alias* form of `DEF:` - name it, add
stitches from a dropdown in order, insert. Deliberately doesn't cover the
raw-geometry form (`%`/`%-N` relative attachment) - building that
visually would need a node/edge graph editor, not a list of dropdowns.

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
level, not just visually).
Shaped patterns print too now (`print_shaped.rs`): each stitch's
abbreviation, colored by tension state or an explicit per-stitch color
when one's set, with round 0 at the *bottom* of the chart (matching
real bottom-up working order, the opposite of colorwork's top-down photo
convention) and a tension/color key page instead of a hex legend.

**Stubbed / TODO:**

- **`cargo audit`, `fmt`, and `clippy` are real CI gates** (`ci.yml`,
  `-D warnings` on clippy) but haven't been exercised against a large
  real-world pattern set yet - the safety limits above (10,000/500,000)
  are reasoned estimates, not load-tested numbers.
- **No test coverage yet for the three newest features** - `print_shaped.rs`,
  `def_builder.rs`, and the `font-kit` enumeration/resolution functions
  in `fonts.rs` all shipped without `#[cfg(test)]` cases, unlike
  everything else in "Testing" below.

## Testing

`cargo test --workspace` runs unit tests across the parser (stitch
counts, repeats, modifiers, labels/attach, custom-stitch expansion, cycle
detection, the count-cap safety limits), eval (increase/decrease
parent-slot math, colorwork graph construction, raw-geometry expansion,
def_origin tagging, the compounding-blowup guard), layout (flat-grid
placement, relax's spring/repulsion behavior including the spatial-grid
rewrite), export (color naming, SVG row-order regression tests), image
import (resize/quantization regression tests, including the
farthest-point-seeding fix and the degenerate-image guard), and print
(tiling-math regression tests - exact page counts pinned against
visually-verified output, plus a "every cell covered exactly once, no
gaps" invariant check) - plus golden tests in `crates/lang/tests/golden.rs`
that compile the bundled example `.cgp` patterns end-to-end and check the
resulting stitch graph's shape.

CI (`.github/workflows/ci.yml`) runs `cargo test`, `cargo fmt --all --
--check`, `cargo clippy --workspace --all-targets -- -D warnings`, and a
`cargo-audit` check against the RustSec advisory database on every push
and PR.

## Suggested next milestones

1. Shaped-pattern PDF/print export, to match what colorwork already has.
2. Test coverage for the three newest features (see the new Stubbed/TODO
   bullet above) - they're the only untested corner of the codebase now.
3. Add `libfontconfig1-dev`/`libfreetype6-dev` to the Linux job in
   `ci.yml` and `release.yml` - `font-kit` needs them at build time and
   CI will start failing on Linux the moment this merges if they're missing.
4. Exercise the safety limits (10,000 literal / 500,000 total stitches)
   against real large patterns to confirm they're generous enough in
   practice, not just in theory.
