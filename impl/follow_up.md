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

Closed, as the formatting-only commit this asked for. `crates/cue-eval/src/lib.rs`,
`crates/cue-wasm/src/lib.rs` and `crates/cue-eval/src/stdlib/encoding.rs` carried
drift from the `serde_yaml_ng` swap in 2aa698d, so `just ci` failed at
`fmt-check` on a clean checkout from then until 69aa4c3. Phases 03-06 left it
alone deliberately, to keep their diffs reviewable; it was `cargo fmt --all` and
nothing else.

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

## A bytes literal cannot interpolate

Open, and a stated limit rather than a surprise. Upstream accepts `'a\(1)b'` and
gives the bytes `a1b`; `Expr::Interpolation` carries no delimiter, so cue-rs
cannot represent an interpolation that yields bytes and phase 06 refuses it by
name instead of producing the characters `a\(1)b` as HEAD did. This is the one
corpus case phase 06 moved, `upstream_cue_testdata_interpolation_scalars.txtar`.
The fix is a delimiter on `Expr::Interpolation` plus bytes-valued concatenation
in `eval_interpolation`, which also wants the string-from-bytes rule that file
pins: invalid UTF-8 interpolated into a *string* becomes replacement characters
by the Unicode standard, one per invalid sequence, not one per byte.

## Optional struct and list fields are materialised

Open, and wrong output on a public surface - found reviewing the 2026-09-22
recursive-schema report, which it is not the cause of. An optional field whose
constraint is a struct or a list is exported as `{}` or `[]` although nothing
ever specified it:

```cue
#S: {a?: string, b?: {x?: int}, c?: [...string], d?: [...#S], e?: int}
v: #S & {a: "hi"}
```

Upstream exports `{"a": "hi"}`. cue-rs exports `a`, plus `b: {}`, `c: []` and
`d: []`; only the scalar `e?` is correctly absent. At `enve`/`enact` scale this
is most of a pipeline's schema appearing in its own export - `ci: {}`,
`jobs: {}`, `workers: {}` on an example that declares none of them. Worth
suspecting the point where an optional field's constraint is turned into a
value: a struct or list constraint apparently settles to an empty concrete
value, where a scalar one stays incomplete and is dropped at export.

## A sibling reference inside the struct being defined does not resolve

Open, upstream-valid, and it blocks the ordinary way a DAG is written:

```cue
stages: {
	build: {name: "build"}
	test: {name: "test", prev: stages.build}
}
```

Upstream exports both stages. cue-rs answers `_|_ (unresolved reference
'stages')` - while `stages` is being evaluated, its own name does not resolve.
No definition, list or recursion is involved; it fails the same way with the
fields written as separate `stages: build:` paths, with the reference inside a
list, through an import, and under a `#Pipeline &`. Referring to a sibling by a
path rooted at the enclosing field is how a `depends_on`/`needs` graph is
written, so this is what a user hits as soon as components reference each
other. Likely the same `resolving_symbols` guard that turns a definition's
self-reference into a `RecursiveRef`, but with no equivalent lazy node for a
plain field.

## A structural cycle is exported as a placeholder string

Open, and it puts a bogus value in a user's JSON:

```cue
#Stage: {name: string, parent: #Stage}
s: #Stage & {name: "a"}
```

Upstream refuses with `#Stage.parent: structural cycle`. cue-rs exports
`{"s": {"name": "a", "parent": "<ref:#Stage>"}}` - an internal placeholder that
has escaped into the output as a JSON string. Either verdict beats this one: a
structural-cycle error, or an incomplete field dropped at export. The recursive
schema shapes around it terminate correctly, so this is about the verdict, not
about termination.

## A disjunction is never normalised

Phase 08 drops a branch equal to one already kept, which is what the
`monorepo-go` abort needed. Upstream also drops a branch *subsumed* by another,
and cue-rs has no ordering relation to decide that with - `compare_values`
answers equal, different or unknown, not narrower. Until it does, a disjunction
of `{t: "x"} | {t: string}` keeps both branches where upstream keeps one. No
reported case needs it; it is the other half of normalisation and belongs with
whatever gives cue-rs a subsumption check.

Two smaller pieces sit behind the same work: `compare_values` answering
`Unknown` past its 4096-node budget keeps a duplicate branch, which the content
hash already wanted under "Re-derivation cost" would settle; and `ValueArena`'s
`SlotMap` never returns capacity after a rollback, so a peak is paid for the
life of the process. Neither is reachable now that the doubling is gone.
