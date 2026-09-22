# A disjunction that doubles every time it is unified

`enve` aborts evaluating `usecases/monorepo-go/.enact`: 3.1 GB resident, 19
seconds, `memory allocation of 3758096384 bytes failed`. The 2026-09-22 report
called it a combinatorial disjunction explosion during cross-file struct
unification, which is the right neighbourhood and not the mechanism.

## Reproducing it

`PackageLoader::load_dir` over the four files the report names -
`components.cue`, `workflows.cue`, `services.cue`, `local.cue`, each contributing
its own `pipeline: schema.#Pipeline & {…}` to one package. Bisecting the five
files in that directory sharpens the report's claim: `workflows.cue` is
*necessary* - omit it and evaluation is instant - while `ci.cue` is not, and no
three files trigger it. Merging the same declarations into a single file does
not trigger it either, because repeated declarations of one field are folded
into a single `&` chain and evaluated once. It takes four separate files.

## What is actually happening

Counting allocations, disjunction calls and branch widths during the abort:

```
[2M allocs]  disj_calls=213 trials=3990066  max_in=1048576 max_out=1048576
[4M allocs]  disj_calls=214 trials=7990032  max_in=2097152 max_out=2097152
[6M allocs]  disj_calls=215 trials=11989566 max_in=4194304 max_out=4194304
[10M allocs] disj_calls=216 trials=19989532 max_in=8388608 max_out=8388608
```

The branch widths are exact powers of two - 2²⁰, 2²¹, 2²², 2²³ - and they are
reached in **three more disjunction calls**. This is not a Cartesian product
over many disjunctions. It is one disjunction being unified with a structurally
equal copy of itself, over and over, **doubling** each time.

`unify_disjunction_inner` unifies every branch of one side against every branch
of the other and keeps each result that is not bottom:

```rust
for b1 in branches {
    for b2 in &other_branches {
        let u = unify_internal(arena, b1.val, b2.val, ctx);
        if bottom { rollback; continue }
        valid_branches.push(DisjunctionBranch { default: …, val: u });
    }
}
```

Nothing asks whether a branch it keeps is one it already has. Unify
`*"sql" | "tcp"` with an equal copy and all four trials are examined: two
conflict, two succeed - and the two that succeed are `"sql"` and `"tcp"` again.
The disjunction is unchanged in meaning and twice as wide. Do it again and it is
four wide, then eight. Each branch is a whole `#Service` struct, so by 2²³
branches the arena is holding tens of millions of nodes and the `SlotMap`'s
backing `Vec` asks for another 3.7 GB.

Four files is the threshold because each file's `pipeline: schema.#Pipeline & {…}`
is one more unification of the same service disjunction against itself, and
because phase 04's re-derivation re-unifies a field whose inputs a later file
moved. The doubling needs roughly twenty rounds to exhaust memory, and four
files supply them.

`arena.rollback` is not the problem and is not a leak: it correctly unwinds the
branches that *conflict*. The branches that survive are the ones that should
never have been kept twice.

## Why upstream does not have it

Upstream normalises a disjunction as it builds one: a branch equal to a branch
already present is dropped, and a branch subsumed by another is dropped. `D & D`
is `D`. cue-rs has no normalisation step at all - there is no `dedup`, `retain`
or subsumption anywhere in `unify.rs`.

Upstream leaves this very value as a seven-branch disjunction that never
collapses (`pipeline.services.postgres: incomplete value {…} | {…} | …`), so the
schema genuinely is ambiguous here. That is the user's business; what matters is
that seven branches stay seven.

## Design

Deduplicate as branches are kept, not afterwards:

```rust
fn push_branch(arena: &ValueArena, kept: &mut Vec<DisjunctionBranch>, b: DisjunctionBranch) {
    for existing in kept.iter_mut() {
        if compare_values(arena, existing.val, b.val) == Equivalence::Equal {
            existing.default |= b.default;
            return;
        }
    }
    kept.push(b);
}
```

Three things this gets right:

- **`compare_values`, not `ValueId`.** Each trial allocates a fresh node, so
  identity never matches; phase 04 already built the content comparison this
  needs, including the active-pair rule that makes a recursive schema terminate.
- **Eagerly, not at the end.** The comparison is quadratic in the kept branches,
  which is why it has to run where the count is small. Deduplicating a list that
  has already reached 2²³ is the cost we are trying to avoid; keeping it from
  reaching three is free.
- **The default mark is merged, not dropped.** If either copy of a branch was
  the default, the survivor is. Losing that would silently change which branch
  an ambiguous disjunction exports.

`Equivalence::Unknown` - the 4096-node budget running out - keeps the branch.
That is the cautious answer here: a duplicate kept costs width, a distinct
branch dropped costs meaning.

Both arms of `unify_disjunction_inner` get it: the disjunction-against-
disjunction arm, which is where the doubling happens, and the
disjunction-against-value arm, which can produce duplicates the same way when
several branches unify to the same thing.

### Validated by spike

The five-file `monorepo-go` package that aborted at 3.1 GB now exports in
**0.01 s at 13 MB**. Branch width peaks at 2 instead of 2²³; disjunction calls
drop from 216 to 98 and branch trials from 24 million to 392. 105 workspace
tests green, corpus unchanged at 526/547 with the same 21 failures.

Spiked together with phase 07, all of which lands first: the seven recorded
shapes still agree with `cue export` v0.16.1, tests and corpus unchanged, and
the abort stays fixed. The two phases do not interact, which is worth stating
because phase 07 stage 4 stops `unify_structs_inner` collapsing a struct on a
transient bottom, and collapsing early is a form of pruning - one might have
expected removing it to widen the search. It does not: the pruning that matters
here is on branches that *succeed*.

## Tests

`crates/cue-eval/tests/disjunction_width.rs`:

- Unifying a disjunction with a structurally equal copy of itself, n times,
  keeps the branch count constant. Asserted on the exported value and on the
  branch count, not on wall time.
- A branch that is genuinely distinct survives; a disjunction of three branches
  unified with one of two gives the branches that actually unify, and no
  repeats.
- The default mark survives deduplication from either side.
- A multi-file package fixture in the shape of the reproducer, evaluated through
  `PackageLoader::load_dir`, exporting the value upstream exports.

## Not in this phase

- **Subsumption.** Upstream also drops a branch that another branch subsumes;
  this phase only drops equal ones, which is what the abort needed. Subsumption
  needs an ordering relation cue-rs does not have yet.
- **The 4096-node budget.** `compare_values` answering `Unknown` on a large
  branch keeps a duplicate. The content hash per `ValueId` already noted under
  "Re-derivation cost" in `follow_up.md` would answer properly and cheaply.
- **Arena growth.** The `SlotMap` never returns capacity after a rollback, so
  the high-water mark is what the process pays. With the doubling gone nothing
  reaches a mark worth reclaiming, so this stays an observation.
