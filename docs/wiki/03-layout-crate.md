# `crates/layout` - 3D placement, relaxation, tension analysis

Single file (`crates/layout/src/lib.rs`), depends only on `core`. Everything
here operates on a `&mut StitchGraph` whose `position`/`tension` fields
start `None` and get filled in by these functions - nothing here builds new
graph structure, it only annotates an existing one.

## `Gauge` - the one input every function here takes

```rust
pub struct Gauge { pub sts_per_4in: f32, pub rows_per_4in: f32 }
```

`StitchKind::baseline_width_mm`/`baseline_height_mm` (in `core`) encode
*relative* proportions for one hardcoded reference gauge
(`REFERENCE_STS_PER_4IN = 16.0`) - a dc is always wider than a sc, in a
fixed ratio. `Gauge::width_scale`/`height_scale` compute a multiplier
(`REFERENCE / actual`, clamped to `0.1..=10.0` so a stray zero/huge gauge
value from a UI field can't collapse or blow up the layout) that rescales
those baselines to an actual swatch. Every function below takes a `Gauge`
and must be called with the **same** `Gauge` value it was laid out with, or
tension analysis compares against the wrong baseline - this is a real
footgun if you're threading gauge through new code, see the doc comment on
`analyze_tension`.

## `layout_ring` - closed-form circular/tapered placement

For each round: radius comes from `circumference = n * avg_stitch_width`
solved for `r = circumference / 2π` (`Vec3::on_circle`, from `core`), height
is the cumulative sum of every prior round's tallest stitch. This is
**not** a simulation - it's a direct formula, exact for a perfectly even,
symmetric round, and it's *why* `relax` (below) exists as a separate pass:
`layout_ring` alone can't represent an asymmetric round (uneven
increase/decrease placement, an off-center `@label` attachment) - every
stitch in a round always lands exactly evenly spaced on a circle regardless
of where its graph neighbors actually are.

## `layout_flat_grid` - plain rectangular placement for colorwork

No trigonometry at all: `col * cell_mm`, `row * cell_mm`, centered around
the origin. Deliberately **not** `layout_ring` with a huge radius - a
colorwork panel is worked flat, not tapered into a tube, and forcing it
through circular placement would misrepresent it. This is the direct
consequence of `core::colorwork`'s bridge modeling colorwork rows as
`Parent`-edge-down flat rows rather than closed loops (see
[01-core-crate.md](01-core-crate.md)).

## `relax` - mass-spring relaxation pass

The one genuinely interesting algorithm in this crate. Two spring types,
built once from the graph's edges (edges don't change during relaxation,
only positions do):
- `Sequence` edges → springs at rest length = `baseline_width_mm` (scaled)
- `Parent` edges → springs at rest length = `baseline_height_mm` (scaled)

Plus a **same-round repulsion** term (`REPULSION_RADIUS_MM = 6.0`) so
stitches don't collapse onto each other - this is the part a pure
spring-only simulation lacks and would let happen on an asymmetric pattern.

Per iteration: compute every spring's length error, apply a fraction of the
correction (`SPRING_STIFFNESS = 0.2`, so it settles gradually across
`iterations` instead of overshooting/oscillating), same for repulsion pairs,
sum all displacements into a `HashMap<NodeIndex, Vec3>`, clamp each node's
total displacement to `MAX_STEP_MM` (2.0), apply.

**The performance-worth-understanding part**: repulsion is scoped to
same-round pairs only, and rather than an O(n²) all-pairs scan, it's found
via `RepulsionGrid` - a uniform spatial hash keyed by `(cell_x, cell_y,
cell_z)` with cell size = the repulsion radius, so any two stitches within
that radius are guaranteed to land in the same cell or an adjacent one.
`candidate_pairs` checks same-cell pairs plus a fixed set of 13 "forward"
neighbor offsets (not all 26 - that would double-count every cross-cell
pair, once from each side). This is a standard technique
("spatial hashing"/"uniform grid broad-phase") worth recognizing anywhere
you need approximate nearest-neighbor / collision queries without a full
spatial tree - the `FORWARD_NEIGHBORS` half-neighborhood trick specifically
is the reusable idea (traversal order that visits each unordered pair
exactly once).

Rebuilt fresh every iteration rather than maintained incrementally - the
doc comment argues this is still cheap since positions only move by
`MAX_STEP_MM` per iteration and grid rebuild is O(n) either way. If you're
ever profiling this, that's the assumption to check first.

## `analyze_tension`

Walks `Sequence` edges, compares actual (post-layout) distance against
`baseline_width_mm` (scaled by the gauge), flags a deviation beyond
`TENSION_THRESHOLD = 0.15` (±15%, matching CrochetPARADE's documented
threshold) as `Loose`/`Stretched`, else `Normal`. This is genuinely simple
once you see it - all the interesting work already happened in
`layout_ring`/`relax`; this is just "did the layout end up further apart or
closer together than a stitch of this type should realistically be."

## Where to hook in if you're extending this crate

- **A new layout mode** (e.g. a proper mesh/drape simulation): add a new
  function alongside `layout_ring`/`layout_flat_grid` that fills in
  `node.position` for every node - nothing downstream cares how positions
  got set, only that they're `Some`.
- **A new shaping-aware measurement**: follow `analyze_tension`'s shape -
  walk one edge type, compare against a baseline, write a result field back
  onto the node.
- **Swapping the wireframe/software-rasterizer viewport for a real GPU mesh
  renderer**: noted as the natural upgrade in `gui/viewport.rs`'s module doc
  and `crates/app/Cargo.toml`'s commented-out `three-d` dependency - this
  crate's output (positions) is exactly what such a renderer would consume,
  no changes needed here.
