# Changelog

All notable changes to this project are documented here. Format loosely
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0] - Unreleased

Initial release. Highlights:

### Added - core pipeline
- Text DSL (`.cgp`) for shaped crochet patterns: stitch counts, repeat
  groups, increase/decrease with correct parent-slot math, front/back
  loop-only and post modifiers, labels and `@attach` points, per-stitch
  `~RRGGBB` color.
- Custom stitches via `DEF:`, in two forms: plain-DSL aliases (with cycle
  detection) and CrochetPARADE-style raw stitch geometry (`%`/`%-N`/`%+N`
  self-reference and relative-attachment brackets, e.g. picots).
- `StitchGraph` (petgraph-backed) with parent/sequence edges, gauge-aware
  ring layout (`layout_ring`) for shaped patterns, flat-grid layout
  (`layout_flat_grid`) for colorwork, and an optional mass-spring
  relaxation pass (`layout::relax`) for asymmetric patterns.
- Tension-deviation analysis against a configurable gauge (stitches/rows
  per 4 inches).
- Standard US crochet-chart symbol SVG export (real "T" shapes with
  yarn-over tick marks for hdc/dc/tr, real branching/merging symbols for
  inc/dec, layered loop/post modifier marks) - not placeholder shapes.
- OBJ export for the 3D armature (Blender-importable).
- Colorwork mode: `COLORGRID:`/`ROW` DSL block, k-means color
  quantization with farthest-point seeding, colored SVG chart + text
  legend export (nearest-match color names), and multi-page tiled
  print-ready PDF export (US Letter or A4, configurable margin).

### Added - GUI
- Five synchronized views: point-and-click Grid editor (auto-switching
  between shaped-stitch and colorwork paint-grid modes), 3D viewport,
  raw DSL text, Image Import, and Text-to-pattern.
- Image import: exact or aspect-locked resize, gauge-based sizing
  (stitches or desired finished size in inches), re-render-from-source on
  resize.
- Text-to-pattern: bundled font families (regular/bold/italic/bold-italic,
  embedded via `include_bytes!`), per-line-centered rendering, shares the
  image-import pipeline end to end.
- Shared "recently used colors" strip between the colorwork palette and
  the shaped grid's per-stitch color picker.
- Undo/redo (in-memory, DSL-text snapshots).
- Autosave and crash recovery: the live pattern is continuously autosaved;
  a non-empty autosave from a previous session is offered back on next
  launch.
- Accessibility: grid cells carry a combined hover tooltip (tension state,
  color hex, custom-stitch origin) as a text fallback for the
  background-color/text-color tension/yarn-color encoding.
- Debounced gauge/relax controls: a drag gesture triggers one recompile on
  release rather than one per pixel dragged.

### Added - round-trip fidelity & diagnostics
- `StitchNode`/`GridCell` track `def_origin` (which `DEF` produced a
  stitch, if any). The Grid tab surfaces this as a banner, per-cell
  styling/tooltip, and a specific status-bar note the moment an edit
  flattens a custom stitch to raw stitches on save.
- `StitchGraph::warnings` surfaces non-fatal issues (currently: a raw
  `DEF` body's out-of-range `%-N`/`%+N`/`@N` reference) via CLI stderr and
  a hoverable indicator in the GUI status bar, instead of silently
  dropping the affected attachment edge with no trace.

### Added - safety & hardening
- Parse-time cap on any single stitch-count/repeat-`*N` literal (10,000),
  and an eval-time cap on total pattern stitch count (500,000) that also
  catches compounding multiplications (a `DEF` body's own repeat times a
  large invocation count) that individually stay under the literal cap -
  both return a clear parse error instead of hanging or exhausting memory.
- `imageimport::resize_preserving_aspect` guards against a degenerate
  (0-width/0-height) source image, which would otherwise divide by zero
  and (via Rust's saturating float-to-int cast) silently attempt a
  multi-billion-pixel resize instead of failing cleanly.

### Added - project infrastructure
- CI (`.github/workflows/ci.yml`): `cargo test --workspace`,
  `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --
  -D warnings`, and a `cargo-audit` check against the RustSec advisory
  database, on every push and PR.
- Release automation (`.github/workflows/release.yml`): tagged (`v*`)
  builds for Windows/macOS/Linux.
- `ARCHITECTURE.md` for implementation details, separated from the
  now-visitor-focused `README.md`.
- `LICENSE` file, `SECURITY.md`, and a bug-report issue template
  (`.github/ISSUE_TEMPLATE/bug_report.md`).
- Package metadata (`description`, `repository`, `readme`, `keywords`,
  `categories`) in `Cargo.toml`.

### Known gaps (see ARCHITECTURE.md for the full list)
- Shaped-pattern PDF/print export doesn't exist (colorwork-only so far).
- No point-and-click `DEF`-authoring UI - custom stitches are DSL text only.
- Font selection is a small curated bundle, not full OS font enumeration.
