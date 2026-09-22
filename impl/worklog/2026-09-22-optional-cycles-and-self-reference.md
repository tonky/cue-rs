# What belongs in a value: optional fields, self-reference, a cycle's verdict

Three divergences from `cue` v0.16.1, found reviewing the recursive-schema OOM
report and none of them its cause. One phase because they are one question asked
three times, and because two are answered in the same function.

Designed as five stages. Six landed: the design fixed self-reference one link
deep, and the shape a user actually writes - a `needs` DAG - is a chain.

## Stage 1: a bottom says what kind of bottom it is

`BottomKind` on `BottomReason`, decided where the bottom is built. Meant to be a
pure no-op, and the corpus said otherwise: **519/547 instead of 526**.

The string match being replaced was load-bearing in a place nothing pointed at.
`eval_single_decl`'s embedding arm raised `EvalError::Evaluation(reason.to_string())`,
and a bottom's `Display` is `_|_ (unresolved reference 'x')` - so an embedding
whose reference had not resolved produced an *error string containing the phrase*,
which `eval_expr`'s nested-struct arm matched and carried back to the retry loop.
Two sites coupled through the text of a third's `Display`. Seven cases broke.

The fix is the thing the stage exists for: raise `EvalError::Unresolved` when the
bottom's kind says so. A new variant with the same `#[error]` wording, so nothing
a user reads changed.

`Conflict` is constructed by a `conflict()` helper in `unify.rs` rather than at
fifty-odd call sites, with the cycle guards and the internal "invalid node id"
left as `Other` - they are not two values that cannot both hold.

## Stage 2: the export verdicts

An optional field is a constraint on a field that may appear, not a field. Eleven
lines asking whether the constraint *looked* concrete became `continue`. A
`RecursiveRef` reaching export became a structural cycle rather than the JSON
string `"<ref:#Stage>"`, and cannot be reached by an optional field because the
struct arm drops those first - which is what upstream does with
`needs?: [...#Stage]` too.

## Stages 3 and 4: binding a partial, and not collapsing on it

A pending declaration now binds the partial value its pass produced. Nothing is
written into the struct, so a transient bottom cannot win the field; only the
scope gains a name, and only for the next pass.

`unify_structs_inner` stopped collapsing a struct for a bottom the relaxation
loop might still resolve. That is what made the pattern-constraint shape work:
`test`'s pending reference used to turn the whole of `stages` into bottom, taking
`build` - the sibling it was waiting for - with it.

## Stage 5, and the interaction it exposed

The messages: `reference "x" not found`, `undefined field: x`, `incomplete
value`, `structural cycle`. Three of the four rows in the design's table now
match upstream exactly.

Splitting `Unresolved` into three kinds broke the corpus again, because stage 4
asked `kind != Unresolved` and the two new kinds collapsed structs. The predicate
is `may_resolve_later()` on the kind, true for all three reference failures: cue-rs
decides the wording where the bottom is built, and at that moment a name the
enclosing literal has not reached yet is indistinguishable from a name that does
not exist. Only the pass that finds nothing new settles which it was.

## Stage 6, which was not in the design: a refined partial is progress

The five recorded shapes passed. This did not:

```cue
stages: {
	a: {name: "a"}
	b: {name: "b", needs: [stages.a]}
	c: {name: "c", needs: [stages.b]}
}
```

One declaration at the outer level, so no pass of it ever *resolves* anything
until the whole chain does - and `eval_decls_scoped` gave up the moment a pass
resolved nothing. One hop worked only because re-derivation caught it afterwards;
two did not.

A pass that resolved nothing but left a pending declaration bound to more than it
was has given the next pass something new to read. That is now progress, with its
own allowance because chain length is a property of the user's graph rather than
of the literal's declaration count.

### Three things this had to get right

**It must terminate.** A structural cycle refines forever: `a: {x: a}` grows a
level per pass.

**The bound is not arbitrary.** `is_unresolved_within` walks 64 levels and, past
that, answers *resolved*. A partial deeper than that is written with a bottom
buried inside it and the loop moves on. At 32 refinements the two-field cycle
`p: {q: r}, r: {s: p}` unrolled exactly to the budget and exported a 34-level
path. The allowance is `MAX_UNRESOLVED_DEPTH / 8`, which leaves room for a cycle
through four fields.

**A cycle should not be reported at the bottom of what was unrolled looking for
one.** When the allowance runs out while the value is still growing, the partials
are dropped before the final pass, so `a: {x: a}` is reported at `a.x` and not at
`a.x.x.x.…`.

## Trade-offs

**Chain length is capped at eight.** Upstream resolves any length. Eight links of
`needs` is already an unusual pipeline, and the cap is what stops a cycle.

**`declares_field` sees only the innermost literal.** An enclosing literal's
environment is saved and restored around this one, so a name *it* declares reads
as not found until the pass that binds it. That is the one row of the design's
table still off: `a: {b: c}, c: a.b` says `reference "c" not found` where upstream
says `incomplete value _`. Same verdict, same path, different wording.

**A non-converging value is an unresolved reference, not a structural cycle.**
Upstream names it. cue-rs reaches the same verdict at the same path. Naming it
would mean claiming a cycle whenever the allowance runs out, which is also what a
chain of nine links looks like.

**Cost: 1.08x time, 1.09x memory** on the corpus (3.54 s / 1.61 GB → 3.84 s /
1.76 GB). Measured with the refinement passes disabled, they are 3.63 s / 1.62 GB
of that - so the allowance costs about 6% time and 9% memory, and the rest is
noise. The `monorepo-go` package that phase 08 rescued still evaluates in 0.09 s
at 22 MB.

## Validation

126 workspace tests green, up from 110. Corpus unchanged at 526/547 with the same
21 failures, compared by name and not by count. Clippy clean, `fmt --check` clean.

Every shape recorded here agrees with `cue export` v0.16.1 on the value; the error
cases agree on the verdict, the path and - for three of four - the wording, and
differ in cue-rs's error envelope.

Each part of the fix was removed in turn to check the tests catch it: dropping
optional fields, 5 tests; the structural-cycle verdict, 1; the partial binding, 8;
not collapsing the struct, 2; the refinement passes, 7.
