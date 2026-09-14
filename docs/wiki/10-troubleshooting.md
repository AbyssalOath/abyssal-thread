# Troubleshooting / knowledge base

Practical "why does X behave this way" and "how do I debug Y" notes,
consolidated from module docs, `ARCHITECTURE.md`, and `CHANGELOG.md`.

## Build issues

**Linux: `cargo build` fails looking for fontconfig/freetype.** Install
`libfontconfig1-dev libfreetype6-dev` (Debian/Ubuntu; adjust package names
for your distro) - needed by `font-kit` for the Text tab's system-font
enumeration. Windows/macOS need nothing extra (they use OS-native font
APIs). CI's Linux job also needs `libgtk-3-dev` (see `CHANGELOG.md`'s 0.2.1
entry - this silently dropped out of `ci.yml` once already).

**`image` crate version mismatches.** `crates/app/Cargo.toml`'s `image`
dependency version+features **must match** `crates/imageimport/Cargo.toml`'s
exactly. `gui/image_import.rs` uses `image::DynamicImage` directly, and two
different major versions of `image` in the same binary produce two
*different, incompatible* `DynamicImage` types (not just an inconvenience -
a real compile error) even though they share a name. If you ever bump
`image` in one crate, check the other.

**`imageproc`/`ab_glyph` pinned versions.** `imageproc = "0.27"` is
deliberately not the newest - check the comment in
`crates/app/Cargo.toml:59-66` before bumping either; there's a real
transitive-dependency conflict with the `image` pin above if you go past
the compatible range.

## Stitch-count safety limits (two separate caps, two separate reasons)

- **`parser::MAX_LITERAL_COUNT = 10_000`** - caught at *parse* time, on any
  single `count`/`*N` literal (`999999999shell` or `(...) * 999999999`
  both error immediately with `ParseError::CountTooLarge`).
- **`eval::MAX_TOTAL_STITCHES = 500_000`** - caught at *eval*/flatten time,
  on the running total across the whole pattern (`ParseError::PatternTooLarge`).

Why both are needed: the literal cap alone doesn't catch a *compounding*
multiplication where every individual number stays under 10,000 but the
product doesn't - `DEF: shell = (500dc) * 20` invoked as `2000shell` has
500, 20, and 2000 all individually fine, but unrolls to 20,000,000 stitches.
`eval::flatten_into` checks `out.len() > MAX_TOTAL_STITCHES` on every
recursive call (not just once at the top), so this fires as soon as the
unrolling actually crosses the line, and `eval()`'s main loop has its own
matching check per `FlatOp` for the same reason, applied to fully-flattened
patterns. **If you ever see a pattern rejected as "too large" that you
believe should be legitimate**, check whether it's actually near either cap
- both have been exercised against realistic large patterns (90,000
stitches across 300 rounds, 499,900 right at the boundary, 300 custom-stitch
invocations at scale - see `eval.rs`'s tests) specifically to confirm
neither cap false-positives on real large patterns, so a rejection is more
likely a genuine typo (an extra digit) than a limit that's set too low.

## Why colorwork and shaped charts flip row/round order oppositely

This one has bitten the project before (a real regression, not
hypothetical) - see [04-export-crate.md](04-export-crate.md)'s
`colorgrid.rs` section. **Shaped** patterns (`export::svg`,
`print_shaped.rs`) draw round 0 at the **bottom**, because that's the real
order rounds are crocheted in. **Colorwork** patterns (`export::colorgrid`,
`print.rs`) draw row 0 at the **top**, because that matches the source
photo's natural reading order. These are *not* inconsistent with each
other - they're each matching a different real-world reference (working
order vs. photo order) - so don't "fix" one to match the other if you
notice the discrepancy; that's what actually caused the original bug (an
earlier version borrowed colorwork's flip from the shaped convention by
analogy, silently flipping every imported image upside down). If you add a
third exporter, decide explicitly which convention it should follow rather
than copying whichever one is nearest in the file.

## `printpdf` pinned at 0.7

Not upgraded despite `RUSTSEC-2026-0187` (a `lopdf` stack-overflow-via-deep-
nesting advisory, pulled in transitively through `printpdf`). Accepted (not
fixed) via a justified entry in `.cargo/audit.toml`, because:

1. This app only **writes** PDFs from scratch - nothing calls
   `lopdf::Document::load`/`load_mem` on file input, so the
   parsing-only vulnerable code path has no way to actually trigger through
   anything this codebase does.
2. `printpdf` 0.7 → 0.12 (the version with a fixed `lopdf`) is a real API
   migration (0.12 added an `html`/`svg`/`azul-layout` rendering path
   alongside the original low-level drawing calls `print.rs`/`print_shaped.rs`
   use), unverified whether the specific calls this project relies on
   (`PdfDocument::empty`, per-layer `add_line`/`use_text`, etc.) survived
   four major versions in compatible form - noted as a real "attempt the
   migration, don't just bump the version number" task, not yet done.

If you're the one attempting that migration eventually: budget real time
for compile-and-verify, and re-run the visual/byte-level PDF checks (see
`print.rs`'s tests, which pin exact tiling numbers against visually
verified output) - this is explicitly not a drop-in bump.

## `.cargo/audit.toml` - where accepted security advisories live

`cargo-audit` (both locally and in CI) reads `.cargo/audit.toml`
*automatically* - it does **not** read a repo-root `audit.toml`, which is
an easy mistake (this project's own history includes it - see
`CHANGELOG.md`'s 0.2.1 entry). If `cargo audit` doesn't seem to be
respecting an ignore you added, check you edited the file in `.cargo/`, not
the repo root.

## No AccessKit/accessibility support - deliberate, not an oversight

Dropped when `eframe`/`egui` bumped to 0.29.1, because `accesskit` pulled
in a vulnerable/unmaintained dependency chain on Linux (`quick-xml` et al.)
with no actual accessibility work built on top of it yet - carrying a live
CVE surface for a feature nothing used yet wasn't worth it. Re-adding it is
just an `eframe` feature flag if/when real screen-reader support becomes a
goal - the trade-off was purely "security exposure now" vs. "unused now,"
not a technical blocker.

## Why the GUI always recompiles the whole pattern on any edit

Covered in depth in [07-app-gui.md](07-app-gui.md) and
[09-rust-patterns-glossary.md](09-rust-patterns-glossary.md) - the short
version: `dsl_source` is the single source of truth, every edit path ends
in re-parse + re-eval + re-layout, and this is deliberate (simplicity over
incremental-update complexity), not something to "optimize away" lightly.
If a specific edit path feels slow, look at whether `layout::relax`'s
iteration count is unnecessarily high for that pattern size before
assuming the recompile itself is the bottleneck - `relax_iterations` is a
GUI-adjustable field specifically because it trades quality for speed.

## Custom-stitch (`DEF:`) round-trip: expected behavior, not a bug

If a pattern using `DEF: shell = ...` gets edited anywhere in the Grid tab
and the saved DSL text no longer contains `Nshell` (just raw flattened
stitches instead), **this is expected**, not data loss - the `DEF:`
definition itself is untouched, only the invocation is gone, and the GUI
shows a specific status-bar note plus an italic-cell/banner warning the
moment it happens. See `StitchNode::def_origin`'s doc comment
(`core/src/graph.rs:24-42`) for the full reasoning on why this isn't
auto-reconstructed. If you need to keep using a custom stitch after editing
nearby cells, edit the DSL tab text directly instead of the Grid tab.

## Where to look when the grid/3D/DSL views seem out of sync

They shouldn't be able to get out of sync at all, by construction (see
[07-app-gui.md](07-app-gui.md)) - if they do, the most likely cause is a
new code path that mutates `self.grid`/`self.colorwork`/`self.graph`
directly without going through `recompile_from_dsl` (or one of the
`sync_dsl_from_*` wrappers around it). Grep for direct assignments to
those fields outside `mod.rs`'s existing ones as the first debugging step.

## Undo/redo is DSL-text snapshots only

It does not survive a crash (in-memory `Vec<String>` history, dies with the
process) - that's what the separate autosave/crash-recovery mechanism is
for (see [07-app-gui.md](07-app-gui.md)'s "Crash recovery" section). If
you're debugging "undo doesn't reach far enough back," remember
`HISTORY_LIMIT = 50` snapshots, and check whether the edit you expect to
find actually pushed a history entry - some paths intentionally *don't*
(e.g. anything that doesn't change `dsl_source` at all skips
`push_history`).
