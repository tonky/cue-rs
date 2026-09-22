# A disjunction that doubles every time it is unified

`enve` aborted evaluating `usecases/monorepo-go/.enact`: 3.1 GB, 19 seconds,
`memory allocation of 3758096384 bytes failed`. The report that came with it
called it a combinatorial disjunction explosion across four files, which was the
right neighbourhood and the wrong mechanism.

## Finding it

`PackageLoader::load_dir` reproduced it first try, so the work was all in
measuring. Two things the bisection corrected in the report:

- `workflows.cue` is *necessary*, not one of four interchangeable files. Omit it
  and evaluation is instant; omit `ci.cue` and it still aborts. No three files
  trigger it.
- Merging the same declarations into one file does not trigger it, because
  repeated declarations of one field fold into a single `&` chain evaluated
  once. It takes separate files.

Instrumenting allocations, disjunction calls and branch widths settled the
mechanism in one run:

```
[2M allocs]  disj_calls=213 trials=3990066  max_in=1048576 max_out=1048576
[10M allocs] disj_calls=216 trials=19989532 max_in=8388608 max_out=8388608
```

Powers of two, three calls apart. Not a Cartesian product over many
disjunctions - one disjunction meeting a structurally equal copy of itself and
doubling, about twenty times.

## What was wrong

`unify_disjunction_inner` keeps every trial that is not bottom and never asks
whether it already has that branch. `*"sql" | "tcp"` unified with an equal copy
examines four pairs: two conflict, and the two that succeed are `"sql"` and
`"tcp"` again. Same meaning, twice the width. Each branch is a whole `#Service`
struct, so twenty doublings is tens of millions of arena nodes.

Upstream normalises as it builds, so `D & D` is `D`. cue-rs had no
normalisation at all - no `dedup`, `retain` or subsumption anywhere in
`unify.rs`.

## What was built

One helper, `push_branch`, used by both arms. It compares content rather than
`ValueId` - each trial allocates a fresh node, so identity never matches - using
the `compare_values` phase 04 already built, including the active-pair rule that
makes a recursive schema terminate. It runs as a branch is kept rather than over
the finished list, because it is quadratic in the branches kept and the entire
point is that the count stays small. And a survivor inherits the default mark of
every copy it absorbs.

## Trade-offs

**`Equivalence::Unknown` keeps the branch.** The comparison budget running out
on a large branch means "not established", and here the cautious answer is to
keep: a duplicate costs width, a distinct branch dropped costs meaning.

**Equality only, not subsumption.** Upstream also drops a branch another branch
subsumes. That needs an ordering relation cue-rs does not have, and the abort
did not need it. In `follow_up.md`.

**Quadratic, deliberately.** Deduplicating a list that has already reached 2^23
is the cost being avoided; keeping it from reaching three is free. The corpus
did not move, which is the evidence that the comparison is not itself expensive
at the widths real schemas produce.

## The tests, and a false guard caught while writing them

The first fixture asserted the exported value and a branch count of 1, and
passed with the deduplication removed - it had resolved to a single branch
before any doubling could accumulate. Doubling needs a branch that unifies with
*more than one* branch of the other side, which is what a schema's permissive
alternative (`kind: *"custom" | string`) provides and what keeps the disjunction
alive across every file's unification. Rewritten that way the fixture reaches
1024 branches without the fix and 2 with it - and 2 is what `cue export`
v0.16.1 gives, which reports the same value as `incomplete value {…} | {…}`.

Three of the five tests fail if the deduplication is removed, at 7, 7 and 1024
branches. The other two guard the opposite mistake - dropping a branch that
should have been kept, or losing which branch was the default - and pass either
way. The module doc says which is which, because a test that cannot fail for the
reason it was written is worse than no test.

## Validation

The five-file `monorepo-go` package exports in **0.01 s at 13 MB** where it
aborted at 3.1 GB. Branch width peaks at 2 instead of 2^23, disjunction calls
drop from 216 to 98, branch trials from 24 million to 392.

110 workspace tests green, up from 105. Clippy clean. Corpus unchanged at
526/547 with the same 21 failures.

Spiked alongside phase 07 before either was implemented: they do not interact.
Worth stating because phase 07 stops `unify_structs_inner` collapsing a struct
on a transient bottom, and collapsing early is a form of pruning - removing it
might have been expected to widen this search. It does not. The pruning that
mattered here is on branches that *succeed*.
