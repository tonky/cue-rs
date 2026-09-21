# Re-derive across the import boundary

Phase 04 shipped with twelve passing cases in `defaulted_field_references.rs`
and the bug still live in the one place `enve` actually meets it. The user added
the thirteenth: the same definition, imported rather than written beside the
override.

## What was wrong

An imported package was evaluated in its own `Evaluator` and its own
`ValueArena`, then deep-copied into the importer's arena. A `Conjunct::Thunk`
holds an `Rc<ThunkEnv>` whose `scopes` are `ValueId`s of the arena it was
captured in, so the copy could not carry it. `clone_value_into_memo` said so, in
a comment that was right about the constraint and wrong about the consequence:

> An imported package is already evaluated; nothing re-derives it here.

The importer does re-derive, at its own `&`. Every imported field arrived as a
single `Conjunct::Value`, which reads nothing, so the gate that decides what a
merge moved never picked one up.

## What was built

Four stages. The first two were planned; the third was found while spiking the
first; the fourth was found by a fixture written for the second.

1. **Share the arena.** `Evaluator::with_arena`, and
   `PackageLoader::load_dir_into` lends the importer's arena to the package it
   is loading and takes it back - on the error path too. `clone_value_into` and
   its helper are deleted, ~170 lines. The import boundary stops being a
   boundary: an imported struct is an ordinary struct with ordinary conjuncts.
2. **The import set is part of the environment.** `Evaluator`'s two
   evaluator-wide maps become one `Rc<Imports>`, `ThunkEnv` captures it beside
   the scope stack, and `derive_thunk` swaps it in. In CUE an import binds an
   identifier in *file* scope, so this is the model rather than a workaround.
3. **A composite can hide an unresolved reference.** `is_unresolved` gained an
   arm for `Disjunction`, `Bounds`, `Validators` and `BuiltinValidator`, plus a
   depth budget, because it walks a graph that can be cyclic on every
   declaration of every relaxation pass.
4. **The descent and the sweep are one loop.** `rederive_children` extracted,
   and the two run together until nothing moves.

## What the spike bought

The whole design was implemented, measured and reverted before the phase doc was
written, which turned three guesses into three facts.

Stage 1 alone is a *regression* on an imported recipe that reads a third package
- `_|_ (unresolved reference 'naming')` where upstream gives `"svc-15432"` -
because the recipe derives under the importer's imports. That made stage 2 a
requirement of stage 1 rather than a nicety.

The obvious form of stage 2 - merge the sub-evaluator's maps into the host's -
is wrong, and cheaply shown to be: with the importer binding `a/naming` and the
recipe binding `b/naming`, both to `naming`, the merged map answers the
importer's. `"HOST-15432"` instead of `"svc-15432"`. That is what moved the
import set onto `ThunkEnv` instead of into a wider map.

And stage 1's cost - the arena retains everything an imported package allocated,
where the copy retained only the reachable part - turned out to be unmeasurable
on the corpus, which was the open question in the design.

## Stage 4, which was not planned

The depth-3 fixture written for stage 2 came out half-right:
`preset.label: "svc-15432"` beside `summary: "svc-5432/15432"` - one field read
two ways in one export. Reproduced in a single package, so it is a phase-04
defect and HEAD is wrong too; the import fixture only made it visible.

`rederive_struct` swept a struct's own recipes to convergence and *then*
descended into the fields the unifier merged, so a field reading a field of a
nested merge was derived against the value the merge left behind. Both movements
feed one worklist, so both belong in one loop. The conjunct-count gate and the
copy-on-write that make the descent cheap are unchanged, and convergence is
unchanged: a settled child returns its own id, so the extra pass adds nothing
and the loop exits on the condition it already had.

Taking this on widened the phase past what the user confirmed. It is the same
defect family, it was already on the diff's critical path, and it cost about
thirty lines; the alternative was checking in a fixture that asserts a value the
engine gets wrong.

## Trade-offs

A struct now descends once per sweep instead of once in total. The corpus does
not notice - each extra descent costs one confirming sweep of an already-settled
child - but it is the first place to look if re-derivation ever profiles hot.
`follow_up.md` carries it beside the phase-04 cost notes.

`is_unresolved` answering yes more often makes two callers more conservative:
the relaxation loop retries more declarations, and accepts them on the final
pass regardless; `derive_thunk` keeps more values it might have overwritten.
Both directions are the safe one, and the corpus is unchanged.

The depth budget rather than a visited set is a deliberate trade: the walk runs
on every declaration of every relaxation pass, and past 64 levels it answers as
it did before descending into composites at all.

## Validation

- Workspace 91 green, up from 86: six new import cases in
  `defaulted_field_references.rs` and one in `rederive_cost.rs`.
- 527/547 txtar, failure list diffed against a saved baseline - byte-identical
  to HEAD's twenty names.
- Cost: 0.82-0.83 s / 1.60 GB against a freshly built HEAD at 0.82-0.85 s /
  1.61 GB, three runs each, under `ulimit -v` with a timeout.
- `rederive_cost.rs` pins the gate across the boundary: a 600-field imported
  definition beside one override derives fewer than 32 recipes.
- Downstream, in the isolated `enve` copy on path dependencies: 74 `enve-cue`
  tests green. Six of its eight examples byte-identical to HEAD; the two that
  change are the bug. `multi_service_posthog` stops handing the application
  `localhost:5432` while the server listens on `15432`, and
  `distributed_monorepo` stops collapsing two separately configured postgres
  services onto one default connection string. Leaf by leaf against
  `cue export` v0.16.1: **24 leaves newly match upstream, and none that matched
  on HEAD stopped matching.**
- Clippy clean with warnings denied; rustfmt reports only the three pre-existing
  drift files.

## Process note

`tempfile` added as a dev-dependency of `cue-eval`, for the generated package
the import cost test needs.

Every experiment ran under `ulimit -v` with a timeout, as phases 03 and 04
established. Not committed; the `enve` pin stays user-owned.
