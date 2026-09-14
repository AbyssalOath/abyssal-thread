# `crates/app/src/gui/` - the `eframe`/`egui` GUI

This is where most of your future feature work will probably happen. Read
`gui/mod.rs`'s module doc first (`crates/app/src/gui/mod.rs:1-10`) - one
sentence captures the whole architecture:

> Single source of truth is `dsl_source` (the DSL text). The grid editor
> and 3D viewport are *views* over the compiled `StitchGraph`: editing a
> grid cell mutates the grid's in-memory representation, re-serializes it
> back to DSL text, and recompiles from scratch.

Every edit anywhere in the GUI ultimately becomes: mutate some in-memory
state → serialize to DSL text → `recompile_from_dsl()` (parse → eval →
layout → tension, exactly the CLI's pipeline) → every view re-derives
itself from the fresh `StitchGraph`/`Pattern`. There is no "partial update"
path - this trades some efficiency (a full recompile per edit) for a much
simpler mental model (every view is always consistent with `dsl_source`,
full stop). If a bug ever looks like "the grid and the 3D view disagree,"
the first thing to check is whether something skipped `recompile_from_dsl`.

## `GoblinApp` - the root state, and `eframe::App::update`

`GoblinApp` (in `mod.rs`) holds everything: `dsl_source`, the compiled
`pattern`/`graph`, `grid`/`colorwork` (the two mutually-exclusive editor
states - see below), undo/redo history, every export/print path field, the
update-checker channels. `eframe::App::update` is called once per frame and
is a big top-to-bottom match on `self.view_mode` plus some always-run
housekeeping (recovery-prompt check, undo/redo keybinds, polling the
update-check channels). This is standard **immediate-mode GUI** structure -
see
[09-rust-patterns-glossary.md](09-rust-patterns-glossary.md#immediate-mode-guis)
if that idea is new: there's no persistent widget tree, `update()` *is* the
UI, called 60 times a second, and every widget's current value is read
directly off `GoblinApp`'s fields each frame.

**Shaped vs. colorwork is one `Option`:** `self.colorwork:
Option<colorwork_grid::ColorworkGridState>`. `Some` ⇒ colorwork mode (paint
grid, no tension), `None` ⇒ shaped mode (stitch-abbreviation grid,
`self.grid: grid::GridState`, tension shown). `recompile_from_dsl` is the
one place that decides which branch to take, based on
`pattern.color_grid.is_some()` - same test as `lang::eval::eval` uses.

### The recompile path in detail (`recompile_from_dsl`, `mod.rs`)

```
parse(dsl_source) -> Pattern
  -> eval(pattern) -> StitchGraph
    if colorwork: layout_flat_grid, update self.colorwork in place
    else:         layout_ring, relax, analyze_tension, self.grid = GridState::from_graph(&g)
  -> autosave to a fixed temp path (best-effort, errors swallowed)
```

One subtlety worth internalizing: for colorwork, the code explicitly
**updates the existing `ColorworkGridState` in place**
(`existing.grid = color_grid.clone()`) rather than replacing it wholesale.
The comment at `mod.rs:307-320` explains why - replacing wholesale used to
reset `selected` (the currently-picked palette color) to 0 on *every single
paint stroke*, because every paint calls `sync_dsl_from_colorwork` →
`recompile_from_dsl`. This is a good concrete lesson: **when a "rebuild
state from source of truth" step runs on every edit, any transient UI state
that lives on the rebuilt struct will get clobbered unless you specifically
preserve it** - the fix here is "mutate in place, sync only the
grid-derived fields," not "never rebuild."

### Undo/redo

Plain `Vec<String>` history/future stacks of `dsl_source` snapshots
(`HISTORY_LIMIT = 50`) - not a diff/patch system, just whole-text
snapshots, which is fine because patterns are small text. `push_history`
clears `future` (standard undo-stack behavior: any new edit invalidates the
redo stack). The DSL text editor has one wrinkle:
`dsl_focus_snapshot` captures the text *when the text box gains focus*, so
"Apply" pushes the pre-edit text, not whatever's already in the box -
otherwise every apply would push identical before/after text onto history.

### Crash recovery / autosave

Every successful `recompile_from_dsl` writes `dsl_source` to a fixed
per-user temp path (`autosave_path()`, includes `$USER`/`$USERNAME` so two
accounts on a shared machine don't collide) - deliberately **not** tied to
undo/redo, since undo/redo is in-memory only and dies with the process. On
`GoblinApp::default()`, if that autosave file is non-empty and differs from
the fresh blank-canvas default, `pending_recovery` gets set, and `update()`
renders a modal recovery prompt *before anything else*, returning early so
nothing underneath can be touched until the user picks Recover/Discard.

## `grid.rs` - the shaped-pattern stitch grid

One clickable cell per stitch node, one row per round. Two things worth
understanding:

**Increase-pairing for display.** `build_cells` walks a round's nodes and
merges a same-parent pair of `Increase` children into one `GridCell` with
`nodes: Vec<NodeIndex>` holding both - this mirrors `export::svg`'s
`branch_glyph` pairing exactly (see
[04-export-crate.md](04-export-crate.md)), same underlying reason: an
`inc` is two graph nodes but should read as *one* clickable "increase" in
any UI. The merge additionally requires matching `color` and `def_origin`
before pairing - worth noting as defensive: an increase's two children
*could* in principle differ (e.g. a partially-edited custom stitch), and
silently picking one property over the other would lose real information,
so the code just declines to merge in that case and shows two separate
cells instead.

**`to_dsl`/`round_to_dsl`/`cells_to_dsl` - the lossy round-trip.** Every
edit re-serializes the *entire* grid back to DSL text from scratch. This
recompresses repeated stitches into `(...)  * N` groups where it can
(`round_to_dsl` tries every divisor of the round length as a candidate
block size), but it does **not** recover original `DEF:` invocations or
irregular/nested repeat groups - a pattern that came in with `2shell` comes
back out as flattened raw stitches the moment *any* cell in the pattern is
edited. This is a deliberate, documented trade-off (see
`StitchNode::def_origin`'s doc comment and
`GridState::def_derived_names`): **auto-reconstructing `Ndefname`
invocations from edited cells would need per-invocation boundary/count
tracking, and getting it wrong risks silently emitting the wrong stitch
count** - worse than an honest, loudly-flagged flatten. If you're ever
tempted to "fix" this by trying to reconstruct invocations, read that
reasoning first; it was a deliberate call, not an oversight.

`PendingEdit` lives on `GoblinApp` (passed by `&mut`), not inside
`GridState` - because `GridState` gets wholesale replaced every recompile,
but the "add/edit stitch" popup needs to survive across frames while the
user is still filling it in. This is a recurring shape in this codebase:
transient per-widget UI state that must outlive a full-state rebuild lives
one level up from the state that gets rebuilt.

## `colorwork_grid.rs` - the paint grid

**Read the top-of-file performance note before touching this file.** The
first version rendered one interactive egui widget *per cell*
(`allocate_exact_size` + `Sense::click_and_drag()` in a nested loop) - for
a 200×150 grid that's 30,000 widgets re-evaluated and hit-tested every
single frame at 60fps, which pegged a CPU core. The fix, and the pattern to
follow for any future "big 2D grid of pixel-like things" UI: render the
whole grid as **one GPU texture** (`ColorImage` → `TextureHandle`, rebuilt
only when `texture_dirty` is set - most frames do zero uploads), draw it
with one `Painter::image` call, and handle click/drag through **one**
interactive region, mapping pointer position to a cell index by plain
arithmetic (`(local.x / CELL_SIZE) as usize`) rather than per-cell hit
testing. This turns an O(width × height) widget cost into O(1) regardless
of grid size. If you ever add a new large-grid UI anywhere in this app,
this is the template.

Other things worth knowing:
- **Bucket fill** (`flood_fill`) is a plain iterative stack-based
  4-connected flood fill - no recursion (would blow the stack on a large
  region), explicit `Vec` as a stack instead.
- **`resize_canvas`** has a real behavioral fork: if `source_image` is
  `Some` (this pattern came from Image Import or Text Import), resizing
  *re-renders from the original photo/text* at the new size - the actual
  fix for a real bug where resizing used to just crop/pad the
  already-quantized grid, leaving old content pinned in a corner. If
  `source_image` is `None` (hand-painted/blank canvas), there's no source
  to re-render from, so it falls back to crop/pad, which is the correct
  and only option in that case.
- **Filet mode** (`FiletColors`, `show_filet_panel`) locks the palette to
  exactly two colors and swaps the free-palette UI for foundation-chain +
  written-instructions display (via `export::filet`, see
  [04-export-crate.md](04-export-crate.md)) - but the underlying `grid:
  ColorGrid` and paint mechanics are unchanged; filet is a thin UI/export
  layer on top of the same paint grid, not a separate code path.

## `viewport.rs` - the 3D inspection view

A hand-rolled software 3D pipeline drawn with `egui::Painter` - no GPU mesh
renderer. Worth reading end to end once if you've never seen a minimal
3D camera pipeline written out explicitly:

1. `camera_position` - spherical coordinates (yaw/pitch/distance around a
   `target` point) → a world-space eye position.
2. `to_camera_space` - builds a right/up/forward orthonormal basis from the
   camera, projects a world point into that basis (this *is* the "view
   matrix" step, just written as basis vectors + dot products instead of a
   4×4 matrix multiply).
3. `screen_from_cam` - perspective divide (`x/z`, `y/z`, scaled by a
   field-of-view-derived factor) → 2D screen coordinates. Returns `None` if
   `cam.z <= 1.0` (behind or too close to the camera) - this is the "near
   plane clip" every real 3D pipeline needs, just reduced to one `if`.

Two render modes: **wireframe** (points + colored line segments per edge)
and **shaded** (`build_faces` stitches each round's stitches into 4-vertex
quads against their parents, computes a face normal via cross product,
dots it against a fixed light direction for flat shading, then draws
farthest-to-nearest - the **painter's algorithm**, the simplest possible
correct depth-sorting technique when you don't have a real depth buffer).
`build_faces`'s wrap-around-detection (skipping the last→first stitch pair
when it's a dramatically longer gap than the round's typical spacing) is
the mechanism that tells a closed round (shaped pattern) from an open flat
row (colorwork) using only geometry, no explicit flag - worth studying as
an example of inferring structure from data rather than threading a new
boolean through everything upstream.

## `image_import.rs` / `text_import.rs` - photo/text → colorwork

These two share almost all their machinery - `text_import.rs`'s module doc
says it outright: rendered text becomes a `DynamicImage` and flows through
the *exact same* resize/quantize/gauge-sizing pipeline a loaded photo does.
If you're adding a third "source" (e.g. an SVG import, a QR code
generator), the shape to copy is: produce a `DynamicImage`, reuse
`recompute()`'s resize→quantize/threshold call, reuse `GridImportPayload`
to hand off to the paint grid. Don't reimplement the sizing/gauge math a
third time.

Both hold a `size_mode: Stitches | Inches` toggle and matching
`apply_gauge_size`/`sync_inches_from_stitches` pair - converting between
"exact stitch count" and "desired finished size at this gauge" is a
recurring UI need across three different files (`image_import.rs`,
`text_import.rs`, `colorwork_grid.rs`) and each keeps its **own copy**
rather than sharing a type - a deliberate simplicity trade-off noted in
`colorwork_grid.rs`'s own `SizeMode` doc comment ("kept as a separate copy
rather than a shared type so this module stays self-contained"). If you're
about to add a fourth copy, that's a signal it might be worth actually
extracting - three duplicates is the point where "just copy it" often
stops paying for itself.

`text_import.rs`'s `render_text_image` centers **each line independently**
(not the whole text block as one unit) - measuring each line's width via
`imageproc::drawing::text_size` and centering it individually within the
canvas. This is what makes "MERCI POUR LE" / "VENIN" (two lines of very
different length) both read as centered.

## `def_builder.rs` - point-and-click custom-stitch authoring

Two independent sub-UIs behind a mode toggle (`BuilderMode::Alias` /
`RawGeometry`), each assembling a `DEF:` line without the user hand-typing
DSL syntax. The alias side is straightforward (ordered list of
`(count, abbrev)` pairs → `format_def_line`). The raw-geometry side is the
more interesting one: rather than a node/edge graph editor, it's a form
that tracks a running **cursor position** (`raw_op_position_count` - a
`3ch` occupies 3 positions, a relative stitch occupies exactly 1, matching
`raw_def.rs`'s own counting rule *exactly*) and lets you build `%`/`%-N`
extra-reference lists by picking "self / N before / N after" from a
dropdown instead of typing `%-N` by hand.

The critical correctness property, worth remembering if you ever touch
this file: **the assembled body is live-validated against the real
`raw_def::parse_raw_def` parser and `looks_like_raw_body` classifier before
the Insert button is even enabled** (`show_raw`'s `round_trips`/`is_raw`
checks). This closes a real footgun: a relative stitch with an overridden
parent but *no* extra `[...]` refs never actually contains a `%` character,
which means `looks_like_raw_body` would silently misclassify it as a plain
alias body at parse time - the UI catches and flags this *before* letting
you insert something that would compile into something other than what you
built. **Whenever a UI assembles text that a separate parser will later
re-read, validate by actually running that parser against the UI's output
before allowing the action that commits it** - don't trust the UI-side
construction logic to match the real grammar by inspection alone.

## `fonts.rs` / `recent_colors.rs` - small self-contained utilities

`fonts.rs`: `FONT_FAMILIES` is a `&'static [FontFamily]` of `include_bytes!`-embedded
TTFs (bundled, so the shipped binary needs no separate `fonts/` folder -
see the module doc for the Google Fonts sourcing/licensing note if you add
a family). `bytes_for(bold, italic)` picks one of the four embedded
variants by pattern-matching `(bool, bool)` - a clean small example of using
a tuple match instead of nested `if`s for a small fixed combination space.
`enumerate_system_font_families`/`resolve_system_font_path` wrap `font-kit`
for the "or pick an installed system font" option, deliberately tolerant of
empty results (a machine with unusual/no fonts just shows an empty
dropdown, not a crash).

`recent_colors.rs`: a small MRU (`most-recently-used`) list, `MAX_RECENT =
12`, shared *by value* (each caller owns its own copy passed by `&mut`, not
a global) between the colorwork palette and the shaped grid's per-stitch
color popup. `record` is the whole algorithm: `retain` to remove any
existing occurrence, `insert(0, ...)` to put it back at the front,
`truncate` to the cap. Worth knowing as the standard shape of "recently
used X" lists generally (browser history, recent files, etc.) - dedupe by
value, always re-promote to front on reuse, cap by truncation from the
back.

## Where a new GUI feature usually plugs in

1. Add a new `ViewMode` variant if it's a whole new tab, or a field on
   `GoblinApp` if it's new state within an existing tab.
2. If it produces or edits pattern data, decide: does it feed
   `dsl_source` directly (most things should), or does it live in a
   separate `Option<...>` alongside `colorwork` for a genuinely different
   "mode"?
3. Any edit path should end in a call to `recompile_from_dsl` (directly, or
   via `sync_dsl_from_grid`/`sync_dsl_from_colorwork`) - don't mutate
   `self.graph`/`self.grid` and expect other views to notice; they only
   refresh on recompile.
4. If it's a big 2D grid of small elements, use the texture-based rendering
   pattern from `colorwork_grid.rs`, not per-cell widgets.
