# `crates/lang` - the DSL: lexer → parser → AST → eval

This is the trickiest crate and the one most worth understanding deeply -
it's a real hand-rolled compiler pipeline (lexer, recursive-descent parser,
AST, evaluator), small enough to read end to end in one sitting.

```
&str --tokenize()--> Vec<Spanned<Token>> --parse()--> Pattern (AST) --eval()--> StitchGraph
        lexer.rs                            parser.rs                  eval.rs
```

`lib.rs`'s `compile(src: &str) -> Result<StitchGraph, ParseError>` is the
one-call entry point that chains all three stages - start there if you just
want to see the pipeline invoked.

## `lexer.rs` - hand-rolled tokenizer

Works **line by line** (`src.lines()`), not as one continuous character
stream - a deliberate simplification, since every piece of this grammar
(`DEF:` lines, `COLORGRID:`/`ROW` lines, comments, ordinary stitch rounds)
is a complete line. Comments (`# ...`) are stripped per-line before
tokenizing that line's content.

Three "whole-line" constructs are recognized *before* falling through to
character-by-character tokenizing, because their content doesn't fit the
ordinary token grammar: `COLORGRID: WxH`, `ROW n: <hex> <hex> ...`, and
`DEF: name = <body>` (whose body is captured **verbatim as a string**,
`Token::DefLine { name, body }` - it's re-parsed lazily later, either as
ordinary DSL ops or as raw geometry, depending on what it turns out to
contain - see `eval.rs` below).

Ordinary tokenizing is a `chars().peekable()` loop with one match arm per
character class - this is the textbook shape of a hand-rolled lexer, worth
studying if lexers are new to you: peek, decide what kind of token starts
here, consume however many chars that token needs, push, repeat. `~RRGGBB`
(inline color) and bare hex digits (`~ff0000`, no `#`) are parsed digit-by-
digit right here rather than delegated to a helper - small enough not to be
worth extracting.

## `ast.rs` - the shape of a parsed pattern

```rust
pub enum Op {
    Stitch { modifier: Option<Modifier>, abbrev: String, count: u32, color: Option<[u8;3]> },
    Repeat { body: Vec<Op>, times: u32 },
    Label(String),
    AttachTo(String),
}
pub struct Round { pub ops: Vec<Op> }
pub struct Pattern {
    pub name: Option<String>,
    pub rounds: Vec<Round>,
    pub definitions: Vec<(String, String)>,   // DEF: name = <raw unexpanded body>
    pub color_grid: Option<ColorGrid>,        // present <=> this is a colorwork pattern
}
```

Notice `definitions` stores **unexpanded body text**, not parsed ops - the
parser doesn't know yet whether a given `DEF` body is a plain-DSL alias or
CrochetPARADE-style raw geometry (that's `eval`'s job, per-invocation, since
it needs to check for a `%` character - see `raw_def::looks_like_raw_body`).
This is a genuinely useful pattern to notice: *defer parsing a sub-grammar
until you know which sub-grammar applies*, rather than trying to make one
grammar cover both up front.

`color_grid.is_some()` is the single flag that decides which of the two
wholly different "pattern modes" you're in - checked once, right at the top
of `eval::eval`.

## `parser.rs` - recursive-descent parser

Classic hand-rolled recursive descent: `Parser { tokens: Vec<Spanned>, pos: usize }`,
with `peek`/`advance`/`expect` primitives and one method per grammar rule
(`parse_pattern` → `parse_round` → `parse_op` → `parse_stitch_term`).
`parse_op` is the dispatch point: `@label` → `AttachTo`, `(` → repeat group,
a leading number → counted stitch, a bare identifier → either a label
(`name!`) or an uncounted stitch, decided by whether a `!` follows.

`MAX_LITERAL_COUNT = 10_000` (a hard cap on any single `count`/`*N` literal)
is enforced right here, at parse time - see
[10-troubleshooting.md](10-troubleshooting.md#stitch-count-safety-limits)
for the full reasoning and how it composes with `eval`'s separate
compounding-blowup check.

`parse_ops` (distinct from `parse`) parses a **bare op list with no
round/pattern wrapper** - this is what lets a `DEF` alias body get parsed by
literally the same grammar an ordinary round uses, just entered at a
different rule.

## `raw_def.rs` - CrochetPARADE-style raw stitch geometry

This is the DSL's most novel piece, and the one most worth reading slowly.
The module doc (`crates/lang/src/raw_def.rs:1-45`) is excellent - read it
directly, this section is a compressed pointer to it, not a replacement.

The problem it solves: an *alias* `DEF` (`DEF: shell = 3dc`) is just
"replay these ops N times" - no new attachment topology. But some real
stitches (a picot: chain a few, then slip-stitch back into an earlier point
to close a loop) need to attach into a point *other than the normal
positional parent* - genuinely new graph shape, not a replay of existing
ops. Raw geometry is the escape hatch for that:

- `%` - "whatever was most recently placed" (the same node an omitted
  attachment would default to).
- `%-N` / `%+N` - N stitches before/after that same cursor point, counting
  individual placed stitches (a `3ch` = 3 positions, not 1 - this trips
  people up, see the worked example below).
- `ss@1[%,%-4]` - `@N` overrides this stitch's *primary* parent to be N
  positions back; the bracketed list adds **extra** `Parent` edges on top
  of that primary one (a picot's closing stitch really does attach to more
  than one earlier point at once - a single parent edge can't express that,
  which is exactly why `StitchEdge::Parent` can appear more than once
  incoming to the same node).

Worked example - `examples/motif_with_attachment.cgp`'s `DEF: p = 3ch, ss@1[%,%-4]`:
1. Place chain 1 (cursor: [chain1])
2. Place chain 2 (cursor: [chain1, chain2])
3. Place chain 3 (cursor: [chain1, chain2, chain3])
4. Place the slip stitch. Its primary parent is `@1` = one position back
   from the cursor *as it stood before this stitch* = chain2. Then resolve
   the bracket: `%` = the cursor itself = chain3 (extra `Parent` edge),
   `%-4` = 4 back from the cursor = out of range this early (only 3
   stitches exist yet) → `g.warn(...)`, no edge added, compilation still
   succeeds.

`parse_raw_def` is a **second, separate hand-rolled parser** just for this
grammar (not routed through `lexer::tokenize`) - worth noticing *why*: `%`,
numeric `@N` (as opposed to the ordinary DSL's `@label` identifier), and
`[...]` brackets are unique to this one sub-grammar, and bolting them onto
the shared `Token` enum would mean every *other* caller of the lexer now has
to account for tokens that only ever mean something inside a raw `DEF` body.
Splitting into a second small parser is simpler than growing the shared one.

`RawAttachRef::Forward(n)` (`%+N`) is the one genuinely tricky bit in
`eval::expand_raw_def`: it can't be resolved until N *more* stitches have
actually been placed later in the same body, so it's queued
(`pending_forward: Vec<(NodeIndex, usize, u32)>`) and resolved in a second
pass after the whole body is placed. If you're adding a new relative
reference kind, this two-pass structure (resolve backward-refs inline,
queue forward-refs, resolve them after) is the shape to follow.

## `eval.rs` - AST → `StitchGraph`

The biggest file, doing three real jobs:

**1. Round/repeat/inc/dec walk.** `eval()` threads a `parent_cursor:
VecDeque<NodeIndex>` through each round - a queue of "next parent slot to
consume," seeded from the previous round's stitches at the top of each
round. Ordinary stitches `pop_front()` one slot each. `inc` pops **one**
slot and gives it to **two** new stitches (shares one parent between two
children - this is why an `Increase` node's counterpart isn't a special
graph shape, it's just two ordinary nodes with the same parent). `dec` pops
**two** slots for **one** new stitch (merges two parents into one child via
two `Parent` edges on the same node - same trick as the raw-geometry
bracket above, a node with more than one incoming `Parent` edge). This
parent-slot bookkeeping is the entire "correct crochet shaping math" the
project is built around - if you ever need to add a new shaping stitch
(triple increase? decrease-3-into-1?), this is the code to extend, and the
pattern (pop N slots, place M nodes, wire Parent edges) generalizes
directly.

**2. Custom-stitch expansion (`flatten`/`flatten_into`).** Alias-form `DEF`
bodies are expanded as pure **text-level macro splicing** before any graph
node exists: `flatten_into` recursively unrolls `Op::Repeat` and any
`Op::Stitch` whose abbreviation matches a known `DEF` name, pushing the
result into a flat `Vec<FlatOp>`. `expanding: Vec<String>` is a simple
stack-based cycle detector (`DEF: a = 1b` / `DEF: b = 1a` → error) - worth
studying as the minimal correct way to detect indirect recursion in a
tree-expansion pass. Raw-geometry bodies are detected here too
(`raw_def::looks_like_raw_body`) but *not* flattened to ops - they're
carried whole as `FlatOp::RawInvoke` and only resolved against the live
graph in `expand_raw_def`, because `%`-family references need real
`NodeIndex`es that don't exist at pure-text-expansion time.

**3. Safety limits.** `MAX_TOTAL_STITCHES = 500_000`, checked in *two*
places: once per `FlatOp` inside `eval()`'s main loop, and once per
recursive call inside `flatten_into` itself. Both are needed - see
[10-troubleshooting.md](10-troubleshooting.md#stitch-count-safety-limits)
for why a single check in one place doesn't catch a *compounding*
multiplication (`DEF: shell = (500dc) * 20` invoked as `2000shell`: neither
500, 20, nor 2000 individually exceeds the parser's per-literal cap, but
`flatten_into` would unroll 10,000,000 ops before `eval()`'s loop ever got a
chance to check).

`def_origin` tagging happens here too: every stitch produced while
expanding a `DEF` gets tagged with that `DEF`'s name (the *innermost* one,
for nesting - see the `origin: Option<&str>` parameter threading through
`flatten_into`). This is pure provenance metadata for the GUI's round-trip
warning banner (see `core::graph::StitchNode::def_origin`'s doc comment,
and [07-app-gui.md](07-app-gui.md)) - it doesn't feed back into `eval`'s own
logic at all.

## `colorwork_dsl.rs` - `ColorGrid` ↔ DSL text

`color_grid_to_dsl` is the serializer half of `COLORGRID:`/`ROW`
(`lexer`/`parser` are the reader half) - small, symmetric, and the
`round_trips_through_parse_and_eval` test is a good template if you ever add
a new serializable DSL block: write the serializer, then assert
`parse(serialize(x)) == x` (or close enough) rather than testing either
direction in isolation.

## `golden.rs` (in `crates/lang/tests/golden.rs`)

Not part of the library - an integration test that compiles the three
bundled `examples/*.cgp` files end-to-end and pins their resulting round
shapes (`sphere.cgp` → `[6, 12, 18, 24, 24, 24, 18, 12, 6]`, etc.). This is
the cheapest possible regression net for "did I just break shaping math" -
if you touch `eval.rs`'s inc/dec logic, this is the first test to watch.
