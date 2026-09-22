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

Closed by phase 07 stage 2. Export skips every optional field, whatever its
constraint holds, which is `cue export`'s rule - `a?: 1` exports `{}`. The
eleven-line shape test it replaced is why `b?: {x?: int}` used to appear as `{}`
and `c?: [...string]` as `[]` while the scalar `e?: int` was correctly dropped.

## A sibling reference inside the struct being defined does not resolve

Closed by phase 07 stages 3, 4 and 6. A pending declaration binds the partial
value its pass produced, an unresolved reference no longer collapses the struct
that holds it, and a pass that refines a partial counts as progress - which is
what lets a chain of them resolve a link at a time.

Two limits remain, both stated rather than hidden. **A chain longer than eight
links does not resolve**, where upstream resolves any length: the allowance is
`MAX_UNRESOLVED_DEPTH / 8`, and it cannot simply be raised, because a partial
deeper than `is_unresolved_within` walks is judged *resolved* and written with a
bottom buried inside it. Giving that walk a visited set instead of a depth budget
is what would lift both. **A non-converging value is reported as an unresolved
reference** at the link it is written on, where upstream says `structural cycle`;
naming it would mean claiming a cycle whenever the allowance runs out, which is
also what a nine-link chain looks like.

## A structural cycle is exported as a placeholder string

Closed by phase 07 stage 2. A `RecursiveRef` reaching export is refused as a
structural cycle naming its path. An optional recursive field never reaches it,
because optional fields are dropped first - which is what upstream does with
`needs?: [...#Stage]` too.

## A bottom's path is not subsumed the way upstream's is

Open, and presentation rather than verdict. `BottomReason` carries a `path` that
nothing populates consistently, so cue-rs reports
`cannot export bottom at 'a.x': _|_ (reference "nope" not found)` where upstream
reports `a.x: reference "nope" not found`. Phase 07 made the *paths* agree, which
is the part that was wrong; what is left is the envelope around them. Worth doing
once, now that the kind is on the type.

Beside it, one wording row still differs: `a: {b: c}, c: a.b` says `reference "c"
not found` where upstream says `incomplete value _`, because `declares_field`
sees only the innermost literal - an enclosing literal's environment is saved and
restored around this one. A stack of environments, or capturing the enclosing
`own_fields` into each `ThunkEnv`, would settle it.

## Exported fields are sorted, not in declaration order

Open, cosmetic, and visible in every diff a user reads. `StructValue` holds its
fields in a `BTreeMap`, so `cue-rs eval` emits `{"jobs": …, "name": …}` where
`cue export` emits `{"name": …, "jobs": …}` - upstream keeps the order the fields
were written in. The values agree; only the order does not, which is why no test
has caught it (`serde_json::Value` compares objects by key). Fixing it means an
insertion-ordered map on `StructValue`, which touches every merge.

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

## The shape rules phase 09 left open

All four are shapes upstream preserves and cue-rs normalises. None changes a
value, and `cue fmt` leaves every one of cue-rs's outputs alone, so each costs
a one-time diff and nothing after it.

**A mixed composite settles on one side.** `{b: 1,` newline `c: 2}` has
elements on both sides of a newline; upstream reproduces the mixture, and one
bit recorded at the opening delimiter cannot. It collapses to whichever side
the first element fell on. Closing it means a position per element, which is
the whole of upstream's `RelPos` model and a phase of its own.

**Trailing comments are still dropped.** `comment_positions.rs` names the
position and has since phase 06. They are also a tabwriter cell upstream
aligns, so closing the hole adds a cell to `Rendered`, not a rule.

**An interpolated label gains parentheses.** `"\(k)": v` is written back as
`("\(k)"): v`. The parser reads CUE's two spellings — an interpolated string
label and a parenthesised dynamic one — as the same node, so the formatter has
nothing to tell them apart with. It is the same defect as the parentheses
phase 09 fixed for expressions, one level down, and wants the same answer: a
form on the label.

**Multiple attributes are one cell.** Upstream gives each its own column. No
CUE file in either repository has two attributes on a field.

**A wrapped `&` chain is not preserved.** `DisjunctionBranch::on_new_line`
covers `|` because that is where the wrapping occurs in practice — a union of
named constants. The same field on `Expr::Binary` would cover the rest.
