# Abyssal Thread

A free, open-source pattern design studio for crochet and other crafts.

Abyssal Thread is a craft design and pattern-making tool written in Rust, created to make designing custom patterns easier, more accessible, and less tedious.

The project began as a crochet CAD tool for my wife, who struggled to find a free, easy-to-use application for creating custom crochet patterns without running into feature restrictions or expensive paywalls. I wanted to build something that would give her the freedom to design, visualize, and export her own patterns without unnecessary limitations.

What started as a tool for crochet has grown into a broader creative workspace for fiber arts, needlecraft, and other grid-based crafts - cross stitch, knitting, quilting, diamond painting, fuse beads, latch hook, Pixelhobby, pixel macrame and pixel art.

The goal is simple: make custom pattern creation accessible, intuitive, and free from feature paywalls.

[![CI](https://github.com/AbyssalOath/abyssal-thread/actions/workflows/ci.yml/badge.svg)](https://github.com/AbyssalOath/abyssal-thread/actions/workflows/ci.yml)

## What it does

Pick a craft when you start a new pattern (the **New Pattern** picker opens
on launch, and from **New...** in the toolbar), then start from a blank
chart, a picture, typed text, or an existing file. The craft is saved with
the pattern, so opening a file always puts you back in the right craft.

### Crochet

- **Two ways to build a pattern:** a compact text DSL for shaped/tapered
  pieces (amigurumi, hats, motifs - `8sc`, `(3sc, inc) * 4`, custom
  stitches), or a point-and-click paint grid for flat colorwork
  (graphgan/tapestry-crochet charts, and filet crochet) - hand-painted,
  imported from a photo, or generated from typed text.
- **3D preview** of the actual stitch structure, laid out automatically
  from the pattern (with an optional physics-relaxation pass for
  asymmetric shapes), so you can see what you're making before you make it.
- **Tension checking** - flags stitches that are unusually tight or loose
  relative to your gauge, right in the 3D view and the grid editor.
- **Real crochet charts** - standard US stitch symbols, not placeholders,
  exported as SVG.

### Chart crafts

Every other craft uses one chart editor (draw, fill, erase, zoom, symbols,
a heavy line every 10 cells, confetti cleanup), the same picture/text
import - colors matched to real products where a catalog exists - a
Materials tab with a shopping list, SVG/PNG export, and a printable PDF
with a color key. On top of that, each craft gets what it actually needs:

| Craft | Highlights |
|---|---|
| **Cross stitch** | DMC and Anchor floss; full, half, 3/4 and 1/4 stitches, backstitch, French knots and blended threads; Aida count and hoop sizing ("size design to fit hoop"); skein estimates; opens and saves `.oxs` (MacStitch / WinStitch / KXStitch) as well as `.cgp` |
| **Knitting** | Colorwork charts at your stitch *and* row gauge (stitches drawn wider than tall); row 1 at the bottom, stitch 1 at the right; flat or in the round; written row-by-row instructions; long-float and 3+-color row warnings |
| **Quilting** | Pixel and half-square-triangle quilts in finished-square sizes; cutting list, yardage per fabric, backing, binding and batting; optional blocks; row-by-row assembly order |
| **Diamond painting** | DMC or Diamond Dotz drill colors; bags of drills with a spare allowance; `.oxs` save |
| **Fuse beads** | Perler, Hama and Artkal colors (midi and mini); charts split into pegboards; bead bags |
| **Pixelhobby** | Standard and XL pixels; baseplates; pixelsquare counts |
| **Latch hook** | Rug-canvas sizes; knot counts per color |
| **Pixel macrame** | Knot sizes; knot counts per color |
| **Pixel art / other** | Plain pixel grids, exported as PNG with a transparent background |

### Everything

- **Print-ready PDFs** tiled across pages with reference numbers,
  ready to tape together, plus a legend/key, shopping list and (for
  knitting and quilting) written instructions.
- **Crash-safe editing** - your work is autosaved continuously and offered
  back to you if the app ever closes unexpectedly, with undo/redo for
  every craft.

Current gaps are listed honestly in ARCHITECTURE.md's "What's real vs.
stubbed" section - for example, knitting charts are colorwork only (no
knit/purl or cable symbols yet), and Pixelhobby, latch hook, macrame and
pixel art use free colors (named, with hex codes) rather than a
manufacturer's color numbers.

## Project Philosophy

Abyssal Thread is built around the idea that creating custom patterns should not require expensive software, paid feature unlocks, or unnecessarily complicated workflows.

The project is open source under the GNU Affero General Public License v3.0 (AGPL-3.0), with the goal of keeping pattern creation accessible and giving users the freedom to use, study, modify, and share the software.

## Demonstrations
![Grid demo](screenshots/grid_demo.gif)

![Image import demo](screenshots/image_import_demo.gif)

![Text demo](screenshots/text_demo.gif)


## Install & run

### Download a prebuilt release (recommended)

Grab the installer for your OS from the
[latest release](../../releases/latest) page (built automatically by
`.github/workflows/release.yml` for every tagged version). Each release
uploads these files - version numbers below match the current release
(`0.2.9`) as an example, but the same naming pattern applies to every
release:

- **Windows** - `Abyssal-Thread_0.2.9_x64-setup.exe` (or the `.msi` if you
  prefer an MSI install). Run it and click through the install wizard.
- **macOS** - `Abyssal-Thread_0.2.9_aarch64.dmg` for Apple Silicon
  (M-series), or `Abyssal-Thread_0.2.9_x64.dmg` for an Intel Mac. Open the
  `.dmg` and drag Abyssal Thread into Applications.
- **Linux** - either `Abyssal-Thread_0.2.9_amd64.AppImage` (no
  installation: `chmod +x` it and run it directly; needs `libfuse2` on
  distros that don't ship FUSE 2 by default) or
  `Abyssal-Thread_0.2.9_amd64.deb` (Debian/Ubuntu: `sudo apt install
  ./Abyssal-Thread_0.2.9_amd64.deb`, or open it with your distro's package
  installer).

**These installers aren't code-signed**, so your OS will warn you the
first time you run one - this is expected, not a sign anything's wrong:

- **Windows**: SmartScreen will say "Windows protected your PC." Click
  "More info," then "Run anyway."
- **macOS**: Gatekeeper will refuse to open it from a normal double-click.
  Right-click (or Control-click) the app and choose "Open" instead, then
  confirm in the dialog that appears - only needed the first time.
- **Linux**: no OS-level warning; just make sure the AppImage/deb is
  executable/installed as described above.

Once installed, the app checks for new releases on startup and offers to
open the release page or download-and-run the new installer directly -
see "Check for Updates" in the toolbar to check manually at any time.

### Build from source

Requires a recent [Rust toolchain](https://rustup.rs/).

On Linux, you'll also need a few system libraries before `cargo build`
will succeed: 
`sudo apt install libfontconfig1-dev libfreetype6-dev` (Debian/Ubuntu; adjust for your distro).
Windows and macOS need nothing extra - their font handling uses OS-native APIs.

```bash
git clone https://github.com/AbyssalOath/abyssal-thread.git
cd abyssal-thread
cargo run -p abyssal-thread
```

That opens the GUI with the New Pattern picker. Or, from the command line
(crochet patterns only):

```bash
cargo run -p abyssal-thread -- build examples/sphere.cgp --svg sphere.svg --obj sphere.obj --tension
```

which compiles a pattern, writes a chart (SVG) and a 3D armature (OBJ,
importable into Blender), and prints a tension report.

## Learn more

- [ARCHITECTURE.md](ARCHITECTURE.md) - workspace layout, full DSL grammar
  reference, the chart crafts' model and file formats, what's implemented
  vs. still stubbed, and testing notes.
- [CHANGELOG.md](CHANGELOG.md) - what changed between releases.
- [WHAT_TO_TEST.md](WHAT_TO_TEST.md) - what's most worth poking at if
  you're trying it out.
- [docs/wiki/](docs/wiki/00-START-HERE.md) - crate-by-crate contributor
  notes for learning the codebase.

## Status

Pre-1.0, actively developed. The crochet pipeline (DSL/paint grid -> 3D
model -> chart/print export) works end-to-end for both shaped and colorwork
patterns, and every chart craft goes from picture/text/blank to a printable
pattern with a shopping list. The chart crafts are new, so expect rough
edges there - see ARCHITECTURE.md's "What's real vs. stubbed" section for
the honest current gaps.

## License

See the [LICENSE](LICENSE) file for license information.

## Contributing

Issues and PRs welcome. `cargo test --workspace` should pass and
`cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets --
-D warnings` should be clean before opening a PR - CI enforces all three.

## Support the Project

If you find Abyssal Thread helpful, please consider supporting its development:

[Donate via Ko-fi](https://ko-fi.com/abyssaloath)
