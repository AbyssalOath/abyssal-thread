# `crates/export` - charts, armatures, legends, filet math

Depends on `core` only. Five independent-ish modules, re-exported flat from
`lib.rs` (`export_svg_chart`, `export_color_chart_svg`,
`export_color_grid_legend`, `export_obj`, `nearest_color_name`,
`filet_starting_chain`/`write_filet_instructions`).

## `svg.rs` - shaped-pattern stitch-symbol chart

Renders one glyph per stitch, arranged schematically by round/index (not
the literal 3D position - a chart is meant to be *read*, not geometrically
accurate). Standard US crochet-chart symbols: oval (chain), dot (slip
stitch), "+" (sc), a "T" shape with 0/1/2 diagonal yarn-over ticks
(hdc/dc/tr - not just three different heights of the same shape, a real
convention detail). Rounds draw **bottom-to-top** (`y = height - round_idx *
CELL`, roughly) because that's the order rounds are actually crocheted in -
contrast this with `colorgrid.rs` below, which draws top-to-bottom for the
opposite reason.

The one non-obvious piece: `branch_glyph`. An `Increase` produces **two**
separate graph nodes sharing one parent (see `layout`'s and `lang::eval`'s
docs) - `export_svg_chart`'s loop specifically looks ahead one stitch,
checks "is the next node also an increase with the same parent," and if so
draws *one* branch glyph (two copies of the base symbol converging on a
shared point) spanning both grid columns, advancing `i` by 2. A `Decrease`
needs no such pairing - it's already one node with two `Parent` edges, so
it draws as a single glyph with two converging lines going the other
direction. If the increase-pairing check ever fails to match (different
label, different parent, mismatched color/def_origin), it falls back to
drawing a plain, unpaired symbol - worth knowing if you ever see a
"broken-looking" chart symbol on what should be a paired increase.

## `colorgrid.rs` - colorwork SVG chart + text legend

**Row 0 draws at the top**, matching the source photo directly - the
*opposite* convention from `svg.rs`, and the module doc explains why
explicitly: this isn't "worked order" like a DSL round, it's "the top row
of the photo," and a chart is only useful if it visually matches the
picture. There's a real regression story here worth reading (module doc,
`crates/export/src/colorgrid.rs:1-16`): an earlier version borrowed the
bottom-to-top flip by analogy from `svg.rs` and silently turned every
imported image upside down, caught only by comparing rendered output
against the source photo - which is *why* the regression tests here check
exact pixel positions rather than "some legend text exists." If you ever
touch row-ordering in either export module, this is the trap to remember:
the two modules have deliberately opposite conventions for good reasons,
don't "fix" one to match the other.

`export_color_grid_legend` assigns each distinct color a letter (`A`, `B`,
...) in first-seen order (`ColorGrid::palette()`) and prints hex + nearest
color name per row.

## `color_names.rs` - nearest-match color naming

A flat `&[(&str, [u8;3])]` table plus squared-Euclidean-distance
nearest-neighbor search (`nearest_color_name`). Not authoritative - it's
explicitly "closest name in a curated list," meant to make a legend
skimmable (`B = #b43535 (~ Crimson)`), not to identify yarn colors
precisely. Worth noting the "muted/desaturated" section of the table was
added specifically because photo-derived palettes (skin tones, worn
fabric) land in that range often, and a *saturated* nearest-match (the
initial table) was a genuinely bad perceptual fit for those - a concrete
example of why curated lookup tables like this tend to need real-world
input samples to fill gaps, not just "add every named color you can think
of" up front.

`nearest_color_name` is also used outside this crate: the chart crafts
without a manufacturer color catalog (latch hook, Pixelhobby, pixel
macrame, pixel art, and quilt fabrics) name each color with it plus its
hex code ("Olive #63a03f") - see `crossstitch::Floss::free`. That's the
one reason `crates/crossstitch` depends on `export`.

## `obj.rs` - Wavefront OBJ point/line export

Deliberately minimal: one vertex per stitch position (scaled mm→m), one
line (`l a b`) per graph edge (both `Sequence` and `Parent`, undistinguished
in the OBJ itself). This is explicitly an *armature* - meant to be imported
into Blender and built into a real mesh around (e.g. a skin modifier along
the edges), not a final render. If you ever want real yarn-mesh geometry,
this is the file to extend (or replace) - it's currently the simplest
possible thing that could work.

## `filet.rs` - filet crochet math

**Not routed through `StitchGraph` at all** - operates directly on a
`ColorGrid`, and the module doc explains why: filet is an open dc+chain
*mesh* (block = solid, space = open), and `core::colorwork`'s
one-single-crochet-per-cell bridge is structurally the wrong stitch for
that. It arrived in 0.2.7 (the "Adding Filet Crochet pattern feature" commit).
It isn't drawn in `ARCHITECTURE.md`'s crochet pipeline diagram, since it
bypasses the stitch graph, but it's described under that file's
"Working - image import" and "printing" notes.

Two functions: `starting_chain(width)` (foundation chain length: `3 *
width + 1 + TURNING_CHAIN`, each column takes 3 stitches, +1 to close the
mesh, +3 for the turning chain that becomes row 1's first dc) and
`write_instructions` (run-length-encodes each row into alternating
block/space runs - `encode_row`'s `Run { filled, count }` - and prints them
as `"3 B, 2 SP"` shorthand). If you're extending filet support (e.g. a
third "special stitch" color, multi-color filet), this file and
`gui/colorwork_grid.rs`'s `FiletColors`/`show_filet_panel` are the two
places to look together - the grid painting logic and the pattern-text
generation are cleanly split, and the GUI already treats `ColorGrid` as the
single source of truth for both.
