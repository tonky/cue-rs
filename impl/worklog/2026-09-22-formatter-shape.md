# The shape a file was written in

Closing the two rules that kept `enve cue fmt` 39 files from its own module.
They turned out to be four, and the measurement turned up two defects that were
not about layout at all.

## Method

`cue fmt` v0.16.1 as the oracle, 105 probes, each one run through both
formatters and then through upstream again. The second pass is the one that
matters: a divergence where upstream accepts our output back unchanged costs a
one-time diff, and a divergence where it does not costs a diff on every run.
That property — **upstream leaves our output alone** — is the contract, and it
holds for all 103 probes upstream parses.

## What upstream actually does

Not "expand structs" and "align fields". Four rules:

1. **A composite keeps its line structure.** The deciding bit is the gap
   between the opening delimiter and the first element. A newline deeper in
   belongs to the composite that holds it, so `a: {b: {` newline `c: 1` newline
   `}}` keeps its outer braces on one line.
2. **A path stays a path.** `a: b: c: 1` is one field holding a struct holding
   a field, and cue-rs wrote all three braces back. This is the dominant shape
   in enve's files — `profiles: dev:`, `services: postgres: files:` — and on
   its own it accounts for most of the 39.
3. **Columns are `text/tabwriter`'s**, not a rule about fields. A cell is
   padded only when another cell follows it on the line; column *k* is sized
   over the maximal run of adjacent lines whose cell *k* is followed by one.
   Everything else contributes no cells and so ends every run — a comment, a
   blank line, an embedding, a `let`, an ellipsis, and a field that does not
   fit on one line.
4. **A wrap inside an expression is kept**, indented one level.
5. **A comment is separated from what is above it** by a blank line, unless that is
   a field. This one was found late, by formatting the module and then asking
   `cue fmt` whether it agreed: it had a line to add to two files, above a
   comment explaining a measured timeout in a run of `let` bindings. It is the
   reason the fixed-point check is run over the module and not only over probes.

Rule 3 has two sharp edges, both measured rather than guessed. A field whose
value holds a composite **anywhere** contributes no cells — `short: f({d: 1})`
ends a run exactly as `short: {d: 1}` does, and so does `short: []`. And a path
aligns as a single cell, against paths of the same depth only: `a: b:` aligns
with `cc: dd:` and not with `dd: ee: ff:`.

## The two defects

Found by formatting `schema/v1/schema.cue` and reading the diff. Neither is a
layout question, and either one makes `just fmt` unsafe to point at.

**Parentheses were dropped.** The parser keeps none, so `(a | b) & c` and
`a | b & c` arrive as the same shape of question with different trees, and the
formatter printed the operands bare for both. `&` binds tighter than `|`, so
`c` moves inside the disjunction: `(1 | 2) & 2` is `2`, and the rewrite is a
disjunction with no default that `cue export` refuses.

The fix is not to remember the source's parentheses but to derive them from the
tree. The output then reparses to the tree it was printed from whatever the
author wrote, which is a stronger guarantee than preservation gives — and a
pair the tree does not need is not written back, which is the last place enve's
module differs from upstream.

**`[...]` was written back as `[]`.** The bare ellipsis names no element type
and the parser stored that as no ellipsis, which is what it stores for a closed
list. `tools?: [...] | *[]` would have become `tools?: [] | *[]`: a list open
to anything, closed at nothing. The same hole was in the evaluator — `[...]`
unified with `[1, 2]` was "conflicting list lengths: 0 and 2" where upstream
gives `[1, 2]` — and it is four lines, so it is fixed here rather than left for
a formatter to write a form the evaluator refuses.

## Trade-offs

**One bit, not a position per token.** Upstream tracks a relative position on
every node, which is how it reproduces `{b: 1,` newline `c: 2}` exactly. One
bit per composite cannot, and normalises such a file to whichever side the
first element fell on. The bit buys the whole of the common case at a fraction
of the model, and the fixed-point property bounds what it costs.

**Depth as a grouping key, not as cells.** A path aligns as one cell, which the
widths prove: `aaaaaaa: b:` and `c: dddddd:` align to 12, not per segment. But
a block breaks when the depths differ. Cells cannot express that, so depth
groups the run.

**The evaluator change is in scope.** It is a four-line fix to a defect the
formatter would otherwise have to write around, and the corpus is unchanged by
it. The alternative was a formatter that emits `[...]` correctly into a file
its own evaluator rejects.

## Verification

- 105 probes: 92 byte-identical to `cue fmt` v0.16.1; all 103 it parses are
  fixed points. 42 pinned in `crates/cue-syntax/tests/formatter_shape.rs`,
  generated from upstream's bytes rather than transcribed.
- enve's module, formatted by this tree: `cue fmt` rewrites none of the 40, and
  39 are byte-identical to its own output. `enve cue fmt --check` names the same
  12 files upstream rewrites, down from 39.
- 133 workspace tests, 0 failed. Corpus 526/547, unchanged by name. Clippy and
  `cargo fmt --check` clean.
- enve built against this tree: 693 tests, 0 failed.

## Open

Five divergences in `follow_up.md`, every one a shape `cue fmt` accepts back.
The one that would close the most is a position per element, which is upstream's
own model and a phase of its own.
