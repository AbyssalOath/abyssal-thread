# `crates/imageimport` - photo → colorwork grid

Single file, depends on `core` + the `image` crate. Two independent steps,
matching how Stitch Fiddle's "convert picture" flow works:

```
DynamicImage --resize (exact or aspect-locked)--> RgbImage --quantize/threshold--> ColorGrid
```

## Resize functions

`resize_exact(img, w, h, filter)` always hits exactly the requested
dimensions - the direct fix for "stuck with whatever size auto-detect
picked." `resize_preserving_aspect` derives the omitted dimension from the
source's aspect ratio when only one of width/height is given (used when a
"lock aspect" toggle is on).

**Worth studying**: the 0×0-source guard in `resize_preserving_aspect`.
Naively, `(w as f32) * src_h / src_w` with `src_w = 0` produces
`NaN`/`infinity`, and Rust's saturating float→int cast turns that into
`u32::MAX` - not a panic, a *silent* attempt to resize to several billion
pixels (a hang or OOM). The fix is `img.width().max(1)` before any division.
This is a good example of a class of Rust footgun: saturating casts (`as
u32` on a float) are memory-safe (no UB) but not *logic*-safe - they
convert "this computation went wrong" into "this computation produced a
huge-but-valid number," which is worse to debug than a panic. Guard the
input, don't rely on the cast to fail loudly.

`ResizeFilter::Nearest` (blocky, for logos/text) vs. `Smooth` (`Triangle`
filter, for photos) - a straight pass-through to `image::imageops::FilterType`.

## `quantize` - k-means color reduction

Standard k-means: assign each pixel to its nearest centroid, recompute
centroids as the mean of their assigned pixels, repeat (`ITERATIONS = 12`,
fixed rather than convergence-checked - target grids are small enough that
this is instant regardless).

**The one genuinely interesting piece: `farthest_point_seed`.** Naive
k-means seeding (e.g. evenly-spaced pixel indices) reliably *misses* small,
rare color regions - a few dozen accent-color pixels on a mostly-uniform
background can end up with every seed landing in the dominant color and
never recovering, because nothing ever "notices" the rare region exists.
`farthest_point_seed` instead does **greedy max-min seeding**: start from
one pixel, then repeatedly pick whichever *remaining* pixel is farthest
(by squared color distance) from every centroid chosen so far. This
guarantees that if there's a small distinct-colored region anywhere in the
image, at least one seed lands in it once that region becomes the
"farthest available" pixel - an actual real regression this project hit
(see `quantize_recovers_a_rare_color_on_a_dominant_background`'s test
comment) and a genuinely useful pattern to remember any time you're seeding
clusters and rare categories matter: **farthest-point / max-min seeding
beats naive/random seeding whenever you need small clusters to survive.**

Deterministic (starts from `pixels[0]`, no RNG) - same input always
produces the same palette, which matters for a tool people iterate on
repeatedly.

## `threshold` - two-color (filet) reduction

Not k-means at all - a literal per-pixel luminance cutoff (`0.299R + 0.587G
+ 0.114B`, the standard ITU-R BT.601 "perceived brightness" luma weights,
so pure blue doesn't read as brighter than pure red at the same threshold).
`invert` swaps which side of the cutoff counts as "filled." This is
deliberately *not* clustering - dragging the fill-threshold slider needs to
visibly grow/shrink the filled region in a predictable direction, which
k-means re-clustering wouldn't give you (the two output colors could
reshuffle unpredictably as the slider moves). Used for filet-crochet mode
(see `04-export-crate.md`'s `filet.rs` section) and nowhere else.

## Reused by the chart crafts

The chart crafts (see [11-crossstitch-crate.md](11-crossstitch-crate.md))
don't have their own image pipeline - `gui/crossstitch_grid.rs`'s
`chart_from_image` calls `quantize` exactly as crochet colorwork does, and
only then matches each resulting color to a real product (DMC floss,
Perler beads...) with CIEDE2000. Doing k-means first and catalog-matching
second keeps this crate craft-agnostic, and means the catalog match runs
once per *distinct* color (a handful) instead of once per pixel. One side
effect worth knowing: two k-means colors can land on the same catalog
color and merge, so a chart can end up with fewer colors than you asked
for. For knitting, the resize step asks for extra rows (by the row/stitch
gauge ratio) so a picture keeps its proportions when stitches are wider
than they are tall.

## Testing pattern worth noting

Every function here has a matching "handles the degenerate case" test
(`quantize_handles_an_empty_image_without_panicking`,
`resize_preserving_aspect_handles_a_degenerate_zero_size_source`) *alongside*
its "does the real job correctly" test. If you add a new image-processing
function, this crate's test file is a good template: one test for the
happy path, one for "what if the input is empty/zero-sized/degenerate."
