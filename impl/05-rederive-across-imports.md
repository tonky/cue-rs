# Re-derive across the import boundary

Phase 04 made a field keep the recipes it was built from, so a merge that
overrides one field re-derives the siblings that read it. That holds inside one
package. It stops at `import`.

```cue
// recipes/recipe.cue
#Recipe: {
	size:  (>0 & <65536) | *5432
	root:  string | *"/var/lib"
	label: string | *"run -r \(root) -n \(size)"
	copy:  size
}

// overridden.cue
import "example.com/defaults/recipes"
overridden: recipes.#Recipe & {size: 15432, root: "/dev/shm/r"}
```

HEAD exports `size: 15432` beside `label: "run -r /var/lib -n 5432"` and
`copy: 5432`. Upstream v0.16.1 exports the overridden values. Written in one
package the same file is already correct - `defaulted_field_references.rs` has
twelve cases that pass - so this is the phase-03/04 defect surviving in exactly
the place `enve` meets it: `pkgs.#PostgresService` is imported, not written
beside the override.

## Why it stops there

`PackageLoader::resolve_imports_for_files` evaluates the imported package in a
*separate* `Evaluator` with a *separate* `ValueArena`, then deep-copies the
result into the importer's arena with `clone_value_into`. A `Conjunct::Thunk`
holds an `Rc<ThunkEnv>` whose `scopes` are `ValueId`s of the arena it was
captured in, so the copy cannot carry it and drops it:

```rust
// package.rs, in clone_value_into_memo
// Conjuncts hold value ids of the source arena, so a cross-arena
// clone keeps only the value. An imported package is already
// evaluated; nothing re-derives it here.
FieldEntry::value(clone_value_into_memo(...), entry.optional)
```

Every imported field arrives as a single `Conjunct::Value`. `rederive_struct`
derives a field only if one of its recipes reads a name the merge moved, and a
`Conjunct::Value` reads nothing, so `label` and `copy` are never derived again.
The comment is precise about the constraint and wrong about the consequence:
the importer *does* re-derive, at its own `&`.

## Design

Stop copying. Load the imported package **into the importing evaluator's
arena**, so its value ids and the recipes its fields carry are valid where the
merge happens. The import boundary then stops being a boundary at all: an
imported struct is an ordinary struct with ordinary conjuncts, and
`BinaryOp::Unify` re-derives it by the rule phase 04 already has.

Only the arena is shared. The imported package keeps its own `Evaluator` - its
own scope stack, placeholders and imports - because those *are* per-package.

That leaves one thing the sub-evaluator owned that a re-derivation needs: which
packages the imported file itself imported. Import resolution today reads two
evaluator-wide maps (`import_aliases`, `imported_packages`) that belong to
whichever file is being evaluated. A recipe carried across the boundary is
derived under the *importer's* maps, where its own imports are absent. So the
import set moves onto `ThunkEnv`, beside the scope stack it already captures -
which is where it belongs: in CUE an import declaration binds an identifier in
file scope, and `ThunkEnv` is exactly "the lexical environment this literal was
written in".

## Stage 1 - share the arena

- `Evaluator::with_arena(ValueArena) -> Self`; `new()` becomes
  `with_arena(ValueArena::new())`.
- Extract `PackageLoader::load_dir_into(dir, target_pkg, &mut Evaluator) ->
  Result<ValueId, EvalError>` from `load_dir_with_package`, which keeps its
  signature and becomes a two-line wrapper. `load_file` is unchanged.
- `resolve_imports_for_files` moves the host arena into a fresh sub-evaluator,
  loads the package, and moves the arena back - on the error path too, so a
  package that fails to load cannot take the arena with it. The sub-package's
  root id is stored in `imported_packages` directly; nothing is copied.
- Delete `clone_value_into` and `clone_value_into_memo` (~170 lines). They have
  no other caller, here or in `enve`.

Cost: the arena now retains everything an imported package allocated, including
what its own unification attempts threw away, where the copy retained only the
reachable part. Measured below: no change on the corpus, which does import.

### Validated by spike

Measured before writing this doc, then reverted and implemented for real:

- `an_imported_definition_propagates_the_importers_overrides` passes.
- Workspace: 86 tests green, none newly failing.
- Corpus: 527/547, byte-identical failure list, 0.82 s / 1.61 GB against a
  baseline 0.92 s / 1.61 GB. No measurable cost.

## Stage 2 - the import set is part of the environment

Stage 1 alone is a **regression** on an imported recipe that reads a third
package, and the spike proves it:

```cue
// recipes/recipe.cue    imports example.com/defaults/naming
#Recipe: {size: int | *5432, label: string | *"\(naming.prefix)-\(size)"}
// use.cue
overridden: recipes.#Recipe & {size: 15432}
```

HEAD gets this right by accident (it never derives `label` again). With stage 1
it derives `label`, fails to resolve `naming` under the importer's maps, and
exports `_|_ (unresolved reference 'naming')` where upstream gives
`"svc-15432"`.

The obvious patch - merge the sub-evaluator's two maps into the host's - is
wrong, and the spike proves that too. With the host importing `a/naming` and
the recipe importing `b/naming`, both under the alias `naming`, the merged map
resolves the recipe's `naming` to the host's package: `"HOST-15432"` where
upstream gives `"svc-15432"`. Aliases are per file; a flat map cannot hold two.

So:

```rust
/// The packages one file imported: what its identifiers name, and the value
/// loaded for each path.
#[derive(Debug, Default, Clone)]
pub struct Imports {
    pub aliases: HashMap<String, String>,   // identifier -> import path
    pub packages: HashMap<String, ValueId>, // import path -> package value
}
```

- `Evaluator::{import_aliases, imported_packages}` (both `pub`, both used only
  inside this crate and not by `enve`) become one `pub imports: Rc<Imports>`.
  The three insert sites go through `Rc::make_mut`, which copies at most once
  per file because every import of a file is resolved before any of its
  declarations are evaluated.
- `ThunkEnv` gains `imports: Rc<Imports>`, captured where it already captures
  the scope stack - a refcount bump per literal, not a map clone.
- `derive_thunk` swaps `self.imports` for the thunk's alongside `scopes` and
  `current_env`, and restores it. A nested literal inside the recipe captures
  the swapped-in set, so it is right at any depth.

This removes the alias-collision class entirely rather than narrowing it, and
it deletes the "which file am I in" ambiguity that the evaluator-wide maps
carry today.

## Stage 3 - a composite can hide an unresolved reference

The stage-1 regression surfaced as a *written bottom* rather than as a kept
value, and that is a second defect. `derive_field` keeps the previous value when
a recipe derives to an unresolved reference, which is what makes re-derivation
safe against a forward reference. `is_unresolved` walks `Bottom`, `Struct` and
`List` - not `Disjunction`. So `string | *"\(naming.prefix)"` deriving to a
disjunction whose default branch is bottom is judged resolved, and overwrites a
good value with one that exports as `_|_`.

This is pre-existing and reachable without imports; it is simply hard to hit
when every recipe resolves. On HEAD,

```cue
x: {let b = a2, let a2 = p, p: int | *5, c: string | *"v\(b)"} & {p: 9}
```

fails to export at all - `_|_ (unresolved reference 'b')` at `x.c` - where with
the fix it keeps `"v5"`. (`"v9"` is the open forward-`let` gap, untouched here.)

The walk gains an arm for every composite, not only `Disjunction`: `Bounds`,
`Validators` and `BuiltinValidator` can each hold a reference the same way, and
the question the walk answers - "has anything in this value failed to resolve?"
- does not depend on which composite holds it. Two callers share that answer:
the relaxation loop, which retries such a declaration and accepts it on the
final pass regardless, and `derive_thunk`, which keeps the value it had. Both
get strictly more conservative.

Descending into composites can reach a cyclic value graph, and the walk runs on
every declaration of every relaxation pass, so it carries a depth budget
(`MAX_UNRESOLVED_DEPTH`) rather than a visited set it would have to allocate.
Past the budget it answers as it did before it descended at all.

## Stage 4 - the descent and the sweep are one loop

Not planned; found by the depth-3 fixture written for stage 2, and reproduced in
a single package, so it is a phase-04 defect rather than anything the boundary
caused. HEAD is wrong here too.

`rederive_struct` swept a struct's own recipes to convergence and *then*
descended into the fields the unifier merged. A field that reads a field of a
nested merge is therefore derived against the value the merge left behind, not
against the value the descent settles:

```cue
#Stack: {preset: #Preset, summary: string | *"\(preset.label)/\(preset.size)"}
stack: #Stack & {preset: size: 15432}
```

`preset.label` comes out `"p-15432"` and `summary` comes out `"p-5432/15432"` -
the same field, read two ways, in one export.

Both movements feed one worklist, so both belong in one loop: descend, add what
the descent moved to what the merge moved, sweep, repeat until nothing moves.
The descent is extracted as `rederive_children`, returning the same `Sweep` the
sweep does. Convergence is unchanged - a settled child returns its own id, so
the second pass adds nothing and the loop exits on the existing condition.

## Tests

In `defaulted_field_references.rs`, against fixtures under
`tests/fixtures/defaulted_field_references/`:

1. `an_imported_definition_propagates_the_importers_overrides` - already written
   by the user; the phase's acceptance test. Also asserts the *un*-overridden
   copy still exports its defaults across the same boundary.
2. An imported recipe that reads a package **it** imported, overridden by the
   importer (stage 2's first case).
3. The same with the importer and the imported package using one alias for two
   different packages (stage 2's second case). Both assert the upstream values,
   which are recorded in this doc and were produced by `cue export` v0.16.1.
4. An imported recipe whose defaulted disjunction reads a name the importer
   overrides, asserting the default branch follows (stage 3).

Blind spots to cover while there, since the boundary is what changed:
- a package imported by two different files of the importing package,
- a package that imports a package that imports a package (depth 3),
- an import that fails to load, asserting the arena comes back and the
  importer still reports the same error it does today.

## Validation

All of the below ran under `ulimit -v` with a timeout, as phases 03/04
established.

- **Workspace**: 91 tests green, none failing. Was 86 before; the six new import
  cases and the import cost case are the difference.
- **Corpus**: 527/547, failure list byte-identical to HEAD's twenty names,
  diffed rather than eyeballed.
- **Cost**: 0.82-0.83 s / 1.60 GB against a freshly built HEAD at 0.82-0.85 s /
  1.61 GB, three runs each. Indistinguishable. Sharing the arena retains what an
  imported package allocated, and the corpus does not notice; the conjunct-count
  gate keeps the extra recipes out of the sweep, which `rederive_cost.rs` pins
  at fewer than 32 derivations for a 600-field imported definition.
- **Downstream**: `enve`'s 74 `enve-cue` tests green on a path dependency. Its
  eight examples evaluated against HEAD and against this phase: six byte-
  identical, and the two that change are this bug.
  `examples/multi_service_posthog.cue` goes from
  `DATABASE_URL=postgresql://postgres@localhost:5432/postgres` beside
  `PGPORT: "15432"` to `postgresql://posthog@localhost:15432/posthog`, and
  `examples/distributed_monorepo/enve.cue` stops collapsing two separately
  configured postgres services onto one default connection string. Against
  `cue export` v0.16.1, leaf by leaf: 24 leaves newly match upstream across the
  two, and **none** that matched upstream on HEAD stopped matching.
- `just lint` clean with warnings denied. `just fmt-check` reports only the
  three drift files that predate these phases.

## Not in this phase

`rederive_value` still descends only into structs, so an override through a list
element or a disjunction branch stays stale - the open `follow_up.md` item, and
orthogonal to the boundary. `validate_json` still never re-derives. Neither is
made better or worse here.
