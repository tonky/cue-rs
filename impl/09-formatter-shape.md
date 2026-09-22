# The shape a file was written in

Phase 06 gave the formatter back the *spelling* of a literal. This gives it back
the *shape* of a file: which composites were written on one line, and the column
a block of fields aligns its values in.

`enve cue fmt` reformats 39 of enve's 40 CUE files today, every one of them
already formatted by upstream `cue fmt`. Nothing in those 39 diffs is a fix.
Two rules account for all of it:

```cue
# written, and what upstream keeps
a: {b: 1, c: 2}          # cue-rs expands it onto three lines
x: {
	short:      1        # cue-rs writes `short: 1`
	longerName: 2
}
a: [
	1,
	2,
]                        # cue-rs collapses it to `a: [1, 2]`
```

Until they are closed `just fmt` in enve has to point at upstream `cue fmt`,
which is the tool `enve cue fmt` exists to replace.

## What upstream does

Measured against `cue fmt` v0.16.1 over ~50 probes; every case below is a test.

**Shape.** A composite keeps the line structure it was written in. The bit that
decides is the gap between the opening delimiter and the first element: a
newline there means expanded, its absence means inline. A newline *inside* a
nested composite belongs to that composite, so `a: {b: {⏎c: 1⏎}}` keeps its
outer braces inline.

**Alignment.** Upstream runs its output through Go's `text/tabwriter`, and the
behaviour to reproduce is that package's, not a rule about fields:

- A line is a sequence of cells. A field contributes `label:`, the value, and
  the attributes as a third cell when it has them.
- A cell is padded only when another cell follows it on the same line.
- Column *k* is aligned over the maximal run of consecutive lines whose cell
  *k* is followed by another cell. Width is the widest cell in that run, plus
  one space.
- Everything that is not an alignable field contributes no cells and therefore
  ends every run: a comment, a blank line, an embedding, a `let`, an alias, an
  ellipsis, a comprehension, and a field that does not fit on one line.

The last of those has a sharp edge worth stating: **a field whose value holds a
struct or list literal anywhere contributes no cells**, even inline, even
empty, even nested inside a call, an index, a parenthesis or a disjunction
branch. `short: f({d: 1})` breaks the run exactly as `short: {d: 1}` does. This
is why a block of service declarations is never aligned and a block of
environment variables always is.

**Wrapping.** A break inside an expression is the author's too. A disjunction of seven
named constants is the CUE idiom for an enum and is written over lines because one line
of it runs to 150 characters; upstream keeps the break where it was written, indented one
level from the declaration.

## What this implements

`StructLit` gains a three-valued `form` — `Inline`, `Block`, or the `Path` of
`a: b: 1`, which is the braces the author never wrote — and `ListLit` a two-valued
one, both set by the parser from that single gap, on the phase-06 recipe: the parser is
the only place that still knows, so it records rather than leaving the formatter to
guess. `DisjunctionBranch` gains `on_new_line` the same way. The formatter grows a
`Rendered` split — cells or an opaque line — and a tabwriter pass over each run of
siblings.

## Two defects the measurement turned up

Neither is a layout rule. Both were found by formatting enve's own schema and reading
the diff, and both change what a file means, so a formatter that has them is not one
`just fmt` can be pointed at.

**Parentheses were dropped.** The parser keeps none, so `(a | b) & c` and `a | b & c`
arrive as different trees and the formatter printed the second for both — `&` binds
tighter than `|`, so `c` moves inside the disjunction. `(1 | 2) & 2` is `2`; the rewrite
is a disjunction with no default, which `cue export` refuses. Fixed by deriving the
parentheses from the tree rather than remembering them from the source: the output
reparses to the tree it was printed from, whatever was written. A pair the tree does not
need is not written back, which is the one place the module still differs from upstream.

**`[...]` was written back as `[]`.** The bare ellipsis names no element type, and the
parser stored that as no ellipsis at all — the same thing it stores for a closed list. A
list open to anything became one closed at nothing, in `schema/v1/schema.cue`'s
`tools?: [...] | *[]`. The AST now carries `open` beside the element type. The same hole
was in the evaluator, where `[...] & [1, 2]` was a length conflict rather than `[1, 2]`;
that is four lines and is fixed here too, because leaving it would have meant a
formatter writing a form the evaluator refuses.

## The one divergence, and why it is safe

A *mixed* composite — `{b: 1,⏎ c: 2}`, elements on both sides of a newline —
is normalized, to inline if the first element shared the opening line and to
expanded otherwise. Upstream reproduces the mixture. One bit cannot.

The property that makes this safe is checkable, so it is the headline test:
**upstream leaves our output alone.** `cue fmt(format(x)) == format(x)` for
every probe, mixed shapes included. We may normalize where upstream preserves;
we may never emit something upstream would rewrite. A file formatted by either
tool is then a fixed point of both, which is the only thing `just fmt` needs.

## Out of scope, and registered

- **Trailing comments are still dropped** (`comment_positions.rs` names the
  position). They are also a tabwriter cell upstream aligns, so closing that
  hole later adds a cell, not a rule.
- **Multiple attributes are one cell here**, where upstream gives each its own
  column. No CUE file in either repository has two attributes on a field.
- **An interpolated label keeps parentheses it was not written with** — `"\(k)": v`
  comes back as `("\(k)"): v`. The parser reads the two spellings as one node. Stable,
  and unrelated to shape.
- **A wrapped `&` chain is not preserved**, only a wrapped disjunction. Neither
  repository has one; the same field would close it.

## Verification

- `cue fmt` v0.16.1 as the oracle over 95 probes: 82 byte-identical, and **all 93 that
  upstream parses satisfy the fixed-point property**. The 11 that differ are the three
  mixed shapes above and eight cases of two pre-existing gaps — dropped trailing
  comments, and parentheses around an interpolated label — both registered.
- 33 of those cases are pinned in `crates/cue-syntax/tests/formatter_shape.rs`, with the
  expectation generated from upstream's bytes rather than transcribed.
- enve's 40-file module: 39 of 40 byte-identical to `cue fmt`'s own output, the 40th
  being the redundant parentheses. Every one of the 40 is a fixed point. `enve cue fmt
  --check` now names **the same 12 files `cue fmt` itself rewrites**, down from 39, so
  `just fmt` can point at `enve cue fmt`.
- 132 workspace tests (up from 126), corpus unchanged at 526/547 by name, Clippy and
  `fmt --check` clean.
- enve, built against this tree: 693 tests, 0 failed.
