# Rust patterns glossary - learning Rust through this codebase

This file is idiom-first, not file-first: each entry names a Rust pattern,
explains *why* Rust nudges you toward it, and points at a real example
already in this repo. Read this once independent of any specific crate.

## Recursive enums need `Box`

`core::stitch::StitchKind` has variants like `Increase(Box<StitchKind>)`.
Why the `Box`? Rust needs to know a type's size at compile time to lay it
out (on the stack, inside a `Vec`, wherever). `enum StitchKind {
Increase(StitchKind), ... }` (no `Box`) is infinitely recursive in size -
"how big is a `StitchKind`?" would require knowing how big a `StitchKind`
is, forever. `Box<T>` is always a fixed-size pointer (one `usize` wide)
regardless of what `T` is, so wrapping the recursive occurrence in `Box`
breaks the infinite regress: `StitchKind` is now "a tag, plus either
nothing or one pointer," a fixed, known size. This is *the* standard fix
for "the compiler says my type has infinite size" any time you see a type
that contains itself - box the recursive occurrence.

## Traits instead of ad-hoc methods for operators

`core::geometry::Vec3` implements `std::ops::Add`/`std::ops::Sub` rather
than plain methods `fn add(self, o: Vec3) -> Vec3`. This lets every call
site write `a + b` instead of `a.add(b)` - operator overloading in Rust is
just implementing the matching trait (`Add`, `Sub`, `Mul`, `Index`, ...)
from `std::ops`. The comment at `geometry.rs:68-75` is worth reading for a
real gotcha: clippy's `should_implement_trait` lint fires if you have an
*inherent* method whose name and signature match a trait method (like a
hand-written `fn add(...)`) regardless of whether you also implement the
trait - so once you're implementing `Add`, delete any same-named inherent
method rather than keeping both.

## Threads + `mpsc` channel instead of `async`

`app::update.rs` (network calls) and its polling code in `gui/mod.rs` use
`thread::spawn` + `std::sync::mpsc::channel()` rather than `async`/`await`.
Why: `egui`/`eframe` is an **immediate-mode GUI with no async runtime**
built in - there's no executor to `.await` inside. The shape:

```rust
pub fn check_async(version: String) -> Receiver<Result<...>> {
    let (tx, rx) = channel();
    thread::spawn(move || { let _ = tx.send(check(&version)); });
    rx   // returned immediately, before the thread finishes
}
```

The caller stores the `Receiver` and calls `rx.try_recv()` once per frame
(non-blocking - returns `Err` immediately if nothing's arrived yet) instead
of `rx.recv()` (which would block the calling thread, freezing the GUI,
until the background thread sends). **`try_recv` vs. `recv` is the whole
trick**: never block the UI thread, poll instead. This is the general
pattern for "do blocking I/O from a GUI with no async support" in any
language with real OS threads, not just Rust.

## Immediate-mode GUIs

`egui` has no persistent widget tree, no "component" objects that live
across frames holding their own state. `GoblinApp::update(&mut self, ctx,
_frame)` runs **every frame** (≈60 times/sec) and *is* the UI - every
`ui.button("...")`, `ui.label(...)` call both draws that frame's widget and
returns whether it was clicked/changed *this frame*. All persistent state
(what's in a text field, which tab is selected, what a slider's value is)
has to live somewhere that survives between calls - which is exactly what
all of `GoblinApp`'s fields (and the various `*State` structs) are for. If
you're used to retained-mode UI (React, GTK, a widget tree you build once
and mutate), the mental adjustment is: **there is no widget tree to hold
state in - the state has to live in your own data, and the UI code is pure
function-of-that-data, called every frame.**

## `thiserror` for a closed error enum

`lang::error::ParseError` derives `thiserror::Error`, with `#[error("...")]`
attributes providing `Display` impls per variant, e.g.:

```rust
#[error("count {0} at line {1} exceeds the maximum of {2} for a single stitch/repeat count - \
         split it into multiple smaller repeats or invocations")]
CountTooLarge(u32, usize, u32),
```

This is the standard way to define a library-internal error type in Rust:
one enum, one variant per distinct failure kind, `thiserror` generates
`impl std::error::Error` and `impl std::fmt::Display` from the attribute
strings so you don't hand-write a `match` in a `Display` impl yourself.
Contrast with `anyhow::Result<T>` (used in `app::main.rs`/`update.rs`) -
`anyhow` is for the *application* boundary, where you just want "an error,
with context, that I'll print and move on from," and don't need callers to
match on specific variants. **Rule of thumb this codebase follows**:
library crates (`lang`, `core`, `layout`, `export`, `imageimport`) define
their own precise error enum with `thiserror`; the `app` binary crate
mostly uses `anyhow` at its own top level, converting a library's typed
error into an `anyhow::Error` with `.map_err(|e| anyhow::anyhow!("..."))`
or `?` + `.context(...)`.

## `petgraph` for graph structures

`core::graph::StitchGraph` wraps `petgraph::graph::DiGraph<StitchNode,
StitchEdge>`. `petgraph` gives you nodes-with-payloads, edges-with-payloads,
directed traversal (`edges_directed(node, Direction::Incoming/Outgoing)`),
and stable `NodeIndex` handles - but it does **not** give you domain
semantics ("what's this stitch's parent," "is this node an increase") or
guaranteed iteration order matching insertion order. That's why
`StitchGraph` layers `rounds: Vec<Vec<NodeIndex>>` (explicit stitch
ordering) and helper methods (`parent_of`, `sequence_neighbors`) on top -
a generalization worth keeping: **a graph library gives you connectivity
primitives; your own domain-specific index/ordering structures on top are
still your job**, don't expect the library to know what "parent" or
"round order" means for your domain.

## `VecDeque` as a FIFO queue

`lang::eval::eval` threads a `parent_cursor: VecDeque<NodeIndex>` through
each round, calling `.pop_front()` to consume the next parent slot in
order. `VecDeque` (a ring-buffer-backed double-ended queue) is the right
choice here specifically because you need **efficient removal from the
front** - a plain `Vec`'s `.remove(0)` is O(n) (it has to shift every
remaining element down), while `VecDeque::pop_front()` is O(1). Recognize
this shape generally: any time you're processing a list strictly
front-to-back and removing as you go, reach for `VecDeque`, not `Vec`.

## Small structs to sidestep borrow-checker pain, not for "design"

`lang::eval::RoundCursor<'a>` bundles four `&mut` references
(`parent_cursor`, `last_in_round`, `this_round`, `pending_label`) that
`expand_raw_def` needs simultaneously. The comment explains the real
reason it exists: **clippy's `too_many_arguments` lint**, plus "these four
already travel together everywhere they're used." This is worth
internalizing as a Rust-specific habit: when a function's parameter list
balloons because several pieces of caller state need to be mutated
together, bundling them into a small `struct` with a lifetime parameter
isn't over-engineering - it's often the natural response to Rust's
insistence on explicit, checked mutable borrows (you can't just close over
outer-scope mutable variables the way you might in a GC'd language without
being explicit about it).

## Struct update via `..Default::default()` and builder-ish patterns

`app::main.rs`'s `eframe::NativeOptions { viewport: ..., ..Default::default() }`
is the idiomatic way to construct a large config struct where you only
want to override one or two fields - implement/derive `Default`, then use
struct-update syntax (`..rest_expr`) to fill in everything else. Recognize
this as Rust's answer to "optional named parameters," which the language
doesn't otherwise have.

## `#[cfg(...)]` for platform-conditional code, not runtime `if`

`app::main.rs`'s `attach_parent_console` (Windows-only Win32 FFI) is gated
`#[cfg(windows)]` - this code **doesn't exist at all** in a non-Windows
build, not "exists but is skipped." Contrast with an ordinary runtime
`if cfg!(windows) { ... }` (used elsewhere, e.g.
`imageimport::ResizeFilter`'s platform-agnostic logic, or
`update::pick_asset`'s `cfg!(target_os = "windows")` checks) - `cfg!(...)`
as an expression is a compile-time-constant `bool`, so the branch not taken
still has to *compile* (and its code still exists in the binary, just
dead), whereas `#[cfg(...)]` on an item removes it from compilation
entirely on other platforms. Use `#[cfg(...)]` when the code literally
can't compile elsewhere (FFI to a platform-specific API, like
`windows_sys::Win32::...`); use `cfg!(...)` as a runtime bool when the code
compiles fine everywhere but should only *run* conditionally.

## Newtype-ish small wrapper structs for domain values

`layout::Gauge { sts_per_4in: f32, rows_per_4in: f32 }`,
`print::PageSize { width_mm: f32, height_mm: f32 }` - small plain structs
grouping two related `f32`s rather than passing them as separate
parameters everywhere. Cheap, `Copy`, and self-documenting at every call
site (`Gauge { sts_per_4in: 16.0, rows_per_4in: 16.0 }` reads clearly;
`layout_ring(&mut g, 16.0, 16.0)` would leave you guessing which float is
which without checking the signature). Worth adopting as a default habit:
**once you're passing the same two-or-more related scalars together to
more than one function, wrap them in a tiny struct.**

## `saturating_sub` / `.max(1)` clamps instead of trusting arithmetic

Seen repeatedly: `parser.rs`'s `row_idx = (index as usize).saturating_sub(1)`,
`imageimport`'s `img.width().max(1) as f32` before division,
`layout::Gauge::width_scale`'s `.clamp(0.1, 10.0)`. Rust integer
subtraction **panics on underflow in debug builds** (and wraps silently in
release builds unless you opt into overflow checks) - `usize` especially,
since it can never be negative, so `0usize - 1` is instant undefined
territory rather than a negative number. `saturating_sub` clamps to zero
instead of underflowing; `.max(1)` before a division prevents
divide-by-zero *and* the NaN/saturating-cast cascade described in
[05-imageimport-crate.md](05-imageimport-crate.md). **Any time you're about
to subtract from or divide by a value that ultimately comes from
user/file/network input, ask whether it could be zero or smaller than
what you're subtracting, and clamp explicitly rather than trusting the
arithmetic to fail loudly** - in Rust, unsigned underflow and float
divide-by-zero often *don't* fail loudly by default.

## `impl Default` + hand-written `fn default()` when zero-init isn't right

`GoblinApp` derives nothing for `Default` - it hand-writes `impl Default
for GoblinApp` because "default" here means "a blank paintable 20×20
colorwork canvas, freshly compiled," not "every field zeroed/empty." This
is the right call whenever `#[derive(Default)]`'s field-by-field zero/empty
semantics wouldn't actually be a sensible starting state - write the impl
by hand instead of trying to force the derive to do something it can't.

## Text as an intermediate representation, re-parsed rather than patched

The whole GUI's edit model (mutate in-memory grid → serialize to DSL text →
re-parse → re-eval) is worth naming as a *deliberate* choice, not laziness:
maintaining a live, incrementally-patched `StitchGraph` in sync with every
possible UI edit would be a much larger and more error-prone surface than
"always regenerate from a single text source of truth." The cost (losing
`DEF`/repeat-group compression on round-trip, a full recompile per edit) is
explicitly accepted in exchange for the simplicity of "there is exactly one
place edits become real, and it's the same parser the CLI and file-load
path already use." If you're ever designing your own tool with multiple
editable views over one model, "one text/canonical representation, always
regenerate all views from it" is a legitimate, simpler alternative to
"patch every view independently and hope they stay in sync."
