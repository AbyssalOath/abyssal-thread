# Changelog

All notable changes to this project are documented here. Format loosely
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.1]

### Changed
- `printpdf` reverted to 0.7 after a 0.9.1 upgrade attempt turned out to
  fix nothing (0.9.1 still depends on the same vulnerable `lopdf` version)
  while requiring a real API migration - see "Security" below for how the
  underlying advisory is actually handled instead.
- `eframe`/`egui` bumped to 0.29.1, fixing `RUSTSEC-2026-0257`
  (`webbrowser` argument injection) for real. AccessKit (accessibility
  support) disabled as part of this bump rather than carried forward - see
  ARCHITECTURE.md's "Stubbed / TODO" for why.
- `.cargo/audit.toml` (moved from a repo-root `audit.toml`, which
  `cargo-audit` never actually reads - it only reads `.cargo/audit.toml`
  or `~/.cargo/audit.toml`). CI's `audit` job simplified back down to a
  plain `cargo audit` now that the file is where it'll actually be found,
  rather than duplicating the ignore list as `--ignore` flags in `ci.yml`.

### Fixed
- Updated `rand` to address an unsoundness issue with custom loggers using
  `rand::rng()`.
- `ci.yml` restored `fmt`/`clippy -D warnings`/`cargo-audit` as separate
  jobs and `libgtk-3-dev` in the Linux dependency install step - both had
  silently dropped out of the workflow file at some point despite
  ARCHITECTURE.md/README.md continuing to document them as enforced.

### Added
- Test coverage for the three features that shipped without it in 0.2.0:
  `print_shaped.rs` (`stitch_print_rgb` coverage, end-to-end PDF-generation
  smoke tests), `def_builder.rs` (name validation and DEF-line formatting
  pulled into pure, directly-testable functions), and `fonts.rs`
  (deterministic bundled-font-data checks plus environment-tolerant
  `font-kit` enumeration tests).
- Exercised the parser/eval safety limits (10,000-per-literal,
  500,000-total) against realistic large patterns: a 90,000-stitch
  multi-round pattern, a 499,900-stitch pattern right at the boundary, and
  300 custom-stitch invocations at scale - confirming they don't
  false-positive-reject legitimate large patterns, not just that they
  catch runaway ones.
- `LICENSE`, `SECURITY.md` (private vulnerability reporting via GitHub's
  advisory feature), a bug-report issue template
  (`.github/ISSUE_TEMPLATE/bug_report.md`), and `WHAT_TO_TEST.md` (a
  tester-facing "what to poke at" note, kept separate from the polished
  README).
- Package metadata (`description`, `repository`, `readme`, `keywords`,
  `categories`) in `Cargo.toml`.

### Security
- `RUSTSEC-2026-0187` (`lopdf` stack overflow via deeply-nested PDF
  objects, pulled in transitively through `printpdf` 0.7): accepted via a
  justified entry in `.cargo/audit.toml` rather than fixed. This app only
  *writes* PDFs from scratch (`print.rs`/`print_shaped.rs`) - nothing
  calls `lopdf::Document::load`/`load_mem` on file input, so the
  vulnerable (parsing-only) code path isn't reachable through anything
  this codebase does. No upgrade path exists that fixes the advisory
  without an unverified, likely-incompatible `printpdf` rewrite-level
  migration (see "Changed" above and ARCHITECTURE.md's TODO list).

## [0.2.0]

### Added - GUI
- Shaped-pattern print/PDF export (`print_shaped.rs`): multi-page tiled
  PDF showing each stitch's abbreviation, colored by tension state or by
  an explicit per-stitch `~RRGGBB` color when set, with round-numbered
  axis labels (round 0 at the bottom, matching real working order rather
  than colorwork's top-down photo convention) and a tension/color key
  page. "Export PDF..."/"Print..." now work for both pattern modes.
- Point-and-click custom-stitch builder (`gui/def_builder.rs`) in the DSL
  tab - covers the alias form of `DEF:` (name + ordered list of stitches);
  the raw-geometry form still requires hand-written DSL.
- Full OS font enumeration for the Text tab (`font-kit`), alongside the
  existing bundled-family dropdown and manual "browse for a font file"
  option - three ways to pick a font now, not one.

## [0.1.0]

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

### Known gaps (at the time of this release, see ARCHITECTURE.md for the
current list)
- No LICENSE file yet (added in 0.2.1).
- Shaped-pattern PDF/print export doesn't exist yet (added in 0.2.0).
- No point-and-click `DEF`-authoring UI (partially added in 0.2.0 - alias
  form only; raw-geometry form still has none).
- Font selection is a small curated bundle only, no OS font enumeration
  (enumeration added in 0.2.0).
- No test coverage for `print_shaped.rs`/`def_builder.rs`/`fonts.rs`
  (these didn't exist yet at 0.1.0; coverage added alongside them in 0.2.1
  once they did).
