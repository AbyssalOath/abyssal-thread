# Abyssal Thread - Personal Learning Wiki

Not for the repo's public docs (see `ARCHITECTURE.md`/`README.md` for that -
this is the contributor-facing reference already in the project). This is
*your* deep-dive notes for learning Rust through this specific codebase and
getting comfortable enough to build features yourself.

## Map of this wiki

| File | What's in it |
|---|---|
| [01-core-crate.md](01-core-crate.md) | `StitchKind`, `StitchGraph`, `Vec3`, `ColorGrid`, `Craft` - the shared data model everything else builds on |
| [02-lang-crate.md](02-lang-crate.md) | The DSL: lexer → parser → AST → eval, custom stitches, raw geometry |
| [03-layout-crate.md](03-layout-crate.md) | 3D placement (`layout_ring`, `layout_flat_grid`), mass-spring relaxation, tension analysis |
| [04-export-crate.md](04-export-crate.md) | SVG charts, OBJ export, color naming, filet crochet math |
| [05-imageimport-crate.md](05-imageimport-crate.md) | Photo → colorwork grid: resize, k-means quantization, thresholding |
| [06-app-cli-and-update.md](06-app-cli-and-update.md) | `main.rs` CLI, the self-updater |
| [07-app-gui.md](07-app-gui.md) | The `eframe`/`egui` GUI: `GoblinApp`, the New Pattern picker, the grid and chart editors, 3D viewport, image/text import |
| [08-print-pdf.md](08-print-pdf.md) | PDF generation/tiling for crochet (both modes) and the chart crafts |
| [09-rust-patterns-glossary.md](09-rust-patterns-glossary.md) | Rust language idioms this codebase uses, explained - for learning Rust itself |
| [10-troubleshooting.md](10-troubleshooting.md) | Build issues, safety limits, known gaps, "why does X behave like that" |
| [11-crossstitch-crate.md](11-crossstitch-crate.md) | The chart crafts (cross stitch, knitting, quilting, beads, diamond painting...): `Chart`, per-craft profiles, color catalogs, `.cgp`/`.oxs` formats |

## The one-paragraph mental model

A pattern is either **shaped** (rounds of stitch abbreviations, `8sc`,
`(3sc, inc) * 4`, worked in the round like an amigurumi) or **colorwork** (a
flat width×height grid of single-crochet, one color per cell, like a
graphgan). Both compile down to the *same* `StitchGraph` type (a `petgraph`
directed graph, one node per stitch, `Parent`/`Sequence` edges) - that's the
one data structure the rest of the pipeline (layout, tension, export, print)
is written against, so it never needs to know which mode produced the graph.

```
.cgp text --lexer/parser--> AST --eval--> StitchGraph --layout--> (3D positions)
                                                            --tension analysis-->
                                                            --export--> SVG/OBJ/PDF/legend
```

That's **crochet**. Every other craft (cross stitch, knitting, quilting,
diamond painting, fuse beads, latch hook, Pixelhobby, pixel macrame, pixel
art) is "a grid of colored cells" and uses a separate, simpler model - a
`Chart` from `crates/crossstitch`, with no stitch graph or 3D layout at
all. A `.cgp` file whose `CRAFT:` line names one of those crafts goes to
the chart parser instead of the crochet one; see
[11-crossstitch-crate.md](11-crossstitch-crate.md).

Crate dependencies (from each crate's `Cargo.toml` - nothing depends on
`app`, and there are no cycles):

```
core                          no workspace dependencies
lang, layout, export,
imageimport                   core only
crossstitch                   core + export (for nearest_color_name)
app (CLI + GUI)               all of the above
```

## How to actually use this wiki

Each crate file is written to be read standalone, with file:line references
back into the real source (which can move - if a reference looks stale,
`grep` the symbol name, don't trust the line number blindly). Start with
`01-core-crate.md` if you want the foundations, or jump straight to whichever
crate you're about to touch. `09-rust-patterns-glossary.md` is worth a
read-through once, independent of any crate - it explains *why* the code is
shaped the way it is in Rust terms (why `Box<StitchKind>` for recursive
enums, why channels instead of `async`, why traits instead of methods for
`Vec3::add`, etc.), which is the actual Rust-learning content.

## Quick orientation commands

```bash
# Run the GUI
cargo run -p abyssal-thread

# Run the CLI on the bundled example
cargo run -p abyssal-thread -- build examples/sphere.cgp --svg /tmp/sphere.svg --tension

# Run every test in the workspace (do this before/after any change)
cargo test --workspace

# Lint the way CI does
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```
