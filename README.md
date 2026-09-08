# Abyssal Thread

A crochet CAD tool, written in Rust to help out my wife: describe a pattern in a small text
language (or paint/import one visually), and get a 3D model, a tension
check, a real crochet chart, and a print-ready PDF out the other end -
all from the same pattern.

[![CI](https://github.com/AbyssalOath/abyssal-thread/actions/workflows/ci.yml/badge.svg)](https://github.com/AbyssalOath/abyssal-thread/actions/workflows/ci.yml)

## What it does

- **Two ways to build a pattern:** a compact text DSL for shaped/tapered
  pieces (amigurumi, hats, motifs - `8sc`, `(3sc, inc) * 4`, custom
  stitches), or a point-and-click paint grid for flat colorwork
  (graphgan/tapestry-crochet charts) - hand-painted, imported from a
  photo, or generated from typed text.
- **3D preview** of the actual stitch structure, laid out automatically
  from the pattern (with an optional physics-relaxation pass for
  asymmetric shapes), so you can see what you're making before you make it.
- **Tension checking** - flags stitches that are unusually tight or loose
  relative to your gauge, right in the 3D view and the grid editor.
- **Real crochet charts** - standard US stitch symbols, not placeholders,
  exported as SVG.
- **Print-ready PDFs** for colorwork charts, tiled across pages with
  reference numbers and a color legend, ready to tape together.
- **Crash-safe editing** - your work is autosaved continuously and offered
  back to you if the app ever closes unexpectedly.

## Demo
![Grid demo](screenshots/grid_demo.gif)

## Install & run

Requires a recent [Rust toolchain](https://rustup.rs/).

On Linux, the GUI's file dialogs need GTK development headers installed
first: `sudo apt install libgtk-3-dev` (Debian/Ubuntu - use your distro's
equivalent package otherwise). Not needed on Windows or macOS.

```bash
git clone https://github.com/AbyssalOath/abyssal-thread.git
cd abyssal-thread
cargo run -p abyssal-thread
```

That opens the GUI directly into a blank paintable canvas. Or, from the
command line:

```bash
cargo run -p abyssal-thread -- build examples/sphere.cgp --svg sphere.svg --obj sphere.obj --tension
```

which compiles a pattern, writes a chart (SVG) and a 3D armature (OBJ,
importable into Blender), and prints a tension report.

Prebuilt binaries for Windows/macOS/Linux are attached to
[releases](../../releases) (built by `.github/workflows/release.yml`).
These aren't code-signed yet, so your OS will likely warn you on first
launch - on macOS, right-click the app and choose "Open" instead of
double-clicking; on Windows, click "More info" then "Run anyway" on the
SmartScreen prompt. This is expected, not a sign anything's wrong.

## Learn more

- [ARCHITECTURE.md](ARCHITECTURE.md) - workspace layout, full DSL grammar
  reference, what's implemented vs. still stubbed, and testing notes.
- [CHANGELOG.md](CHANGELOG.md) - what changed between releases.

## Status

Pre-1.0, actively developed. The core pipeline (DSL/paint grid -> 3D model
-> chart/print export) works end-to-end for both shaped and colorwork
patterns; see ARCHITECTURE.md's "What's real vs. stubbed" section for the
honest current gaps.

## License

See the [LICENSE](LICENSE) file for license information.

## Contributing

Issues and PRs welcome. `cargo test --workspace` should pass and
`cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets --
-D warnings` should be clean before opening a PR - CI enforces all three.
