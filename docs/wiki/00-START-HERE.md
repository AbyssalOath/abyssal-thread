# Abyssal Thread - Personal Learning Wiki

Not for the repo's public docs (see `ARCHITECTURE.md`/`README.md` for that -
this is the contributor-facing reference already in the project). This is
*your* deep-dive notes for learning Rust through this specific codebase and
getting comfortable enough to build features yourself.

## Map of this wiki

| File | What's in it |
|---|---|
| [01-core-crate.md](01-core-crate.md) | `StitchKind`, `StitchGraph`, `Vec3`, `ColorGrid` - the shared data model everything else builds on |
| [02-lang-crate.md](02-lang-crate.md) | The DSL: lexer → parser → AST → eval, custom stitches, raw geometry |
| [03-layout-crate.md](03-layout-crate.md) | 3D placement (`layout_ring`, `layout_flat_grid`), mass-spring relaxation, tension analysis |
| [04-export-crate.md](04-export-crate.md) | SVG charts, OBJ export, color naming, filet crochet math |
| [05-imageimport-crate.md](05-imageimport-crate.md) | Photo → colorwork grid: resize, k-means quantization, thresholding |
| [06-app-cli-and-update.md](06-app-cli-and-update.md) | `main.rs` CLI, the self-updater |
| [07-app-gui.md](07-app-gui.md) | The `eframe`/`egui` GUI: `GoblinApp`, the grid editors, 3D viewport, image/text import |
| [08-print-pdf.md](08-print-pdf.md) | PDF generation/tiling for both pattern modes |
| [09-rust-patterns-glossary.md](09-rust-patterns-glossary.md) | Rust language idioms this codebase uses, explained - for learning Rust itself |
| [10-troubleshooting.md](10-troubleshooting.md) | Build issues, safety limits, known gaps, "why does X behave like that" |

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

Crate dependency order (each only depends on crates to its left):

```
core  →  lang  →  layout  →  export  →  app (CLI + GUI)
  ↑                                        ↑
  └──────────── imageimport ───────────────┘
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
