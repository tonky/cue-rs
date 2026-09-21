# Follow-up

## Deferred generated-field evaluation

Closed by stage 3 of 03-deferred-field-conjuncts.md. A comprehension or an
embedding that constrains an already-read field is now ordinary re-derivation,
so the "deferred generated-field unification is not supported" guards are gone
and `repeated_fields.rs` asserts the upstream values instead of the refusals.

## Re-derivation cost

Mostly closed, and what is left is cheap. Phase 04 stage 3 derives only the
recipes that read a name the merge moved, in dependency order, which took the
corpus to 1.07x HEAD and `rederive_cost.rs` pins it with the evaluator's own
`derivations` and `unsettled` counters rather than with wall time.

What remains: `deps::recipe_deps` walks a field's expression at capture, so a
literal nested n deep has its subtree walked n times, and the resulting
`HashSet<String>` is allocated per field whether or not anything ever merges.
Neither shows up on the corpus or on enve. If one ever does, the walk is
memoizable per expression and a shared empty set covers the common constant
field.

Phase 05 folded the descent into merged children into the same loop as the
sweep, so a struct now descends once per sweep rather than once in total. The
corpus does not notice - a settled child returns its own id, so the extra passes
cost one confirming sweep each - but this is the loop to look at first if
re-derivation ever shows up in a profile.

`compare_values` still answers `Unknown` past a 4096-node budget, where a large
value settles rather than sweeping again. The cost is a missed propagation to a
reader of that field, not a wrong value in it; a content hash cached per
`ValueId` is the cheap way to answer properly.

## Re-derivation through lists and disjunctions

Open, and the same bug phases 03-04 exist to fix. `rederive_value` only descends
into structs, so a merge inside a list element or a disjunction branch never
re-derives its readers: `svcs: [...#Svc]` with `svcs: [{port: 9090}]` exports the
default URL beside the overridden port. HEAD is wrong here too. The fix is to
descend into `Value::List` and `Value::Disjunction` with the same copy-on-write.

## vet does not re-derive, and its verdict is inverted

Open, and a correctness hole on a public surface. `lib.rs::validate_json` - which
is `cue-rs vet` and `cue-wasm`'s `validate_json` - calls the free `unify` and
never goes through the evaluator, so data that is correct against a defaulted
schema is rejected and data that carries the stale default is accepted. Wants an
`Evaluator::unify_and_rederive` that every `unify` caller outside `unify.rs`
routes through.

## A forward let is dropped rather than relaxed

Open. `derive_thunk` evaluates a literal's `let` bindings in one declaration-order
pass, so `let b = a2` written above `let a2 = p` never resolves, its reader
derives to an unresolved reference, and `derive_field` keeps the pre-merge value
with no diagnostic. Upstream derives it. HEAD is wrong here too. The binding loop
wants the same relaxation the declaration loop uses, and "unresolved because the
relaxation loop has not got there" wants telling apart from "unresolved because
of the merge", which upstream reports as an error.

## Value is no longer Send

Open, needs a decision. `FieldEntry.conjuncts` reaches `Rc<Expr>` and
`Rc<ThunkEnv>`, so the public `value::Value` lost `Send`/`Sync` in phase 03.
That forecloses parallelising evaluation or the txtar runner. `Arc` plus a lock,
or an arena-side expression table, would keep the property; neither phase doc
decided this deliberately.

## rustfmt drift

Open, needs a decision. `crates/cue-eval/src/lib.rs`, `crates/cue-eval/src/stdlib/encoding.rs`
and `crates/cue-wasm/src/lib.rs` carry rustfmt drift that predates these phases,
so `just ci` fails at `fmt-check` on a clean checkout. Either a formatting-only
commit of its own, or drop `fmt-check` from `ci`. Phases 03 and 04 deliberately
leave the drift alone so the diff stays reviewable.

## A package imported twice is loaded twice

Open, cheap, and only worth doing if it shows up. `imported_packages` caches per
evaluator, and each package gets its own evaluator, so when A and B both import
C, C is loaded once per importer. HEAD does the same - it copies C into A's arena
and into B's - so phase 05 neither helps nor hurts here. Threading one cache
through `load_dir_into` would make it once per build.

## Downstream release

User-owned: update both enve workspace dependency revisions and Cargo.lock after
cue-rs publication, then rerun its policy regressions against the actual git pin.
The user explicitly requested handling this downstream step themselves. Current
local-path validation does not complete this release step.
