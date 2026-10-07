# Comprehension fields in closed definitions, and error paths

A field an `if` or `for` in a definition's own body adds is part of the
definition (spec: a closed struct allows the fields its declarations define,
comprehensions included). cue-rs refused it when the guard read a field the
definition is unified with: `#P: {a: int | *0, n: {if a > 0 {c: 2}}}` and
`#P & {a: 1, n: c: 2}` failed `field not allowed`, because the closedness check
in `unify_structs_inner` runs before re-derivation re-runs the comprehension
with `a: 1`. enve's service presets hit it (E51). Separately, every conflict was
reported at the struct it collapsed (`cannot export bottom at 'task': _|_
(...)`), where upstream names the field (`task.timeout: conflicting values`).

Pinned against `cue export` v0.17.1 in `tests/comprehension_closedness.rs`
(`fixtures/comprehension_closedness`, `fixtures/error_paths`, `regen.py` in
each): `if`/`for`/`let` guards, nested, embedded, listed, pattern-matched and
disjoined definitions, definitions unified with definitions, merges in several
steps, and the negatives (guard false, a field nothing declares).

## Credits

- A field the closed side does not declare is admitted **on credit** when a
  comprehension could generate it: one of the closed struct's recipes, or one
  of an enclosing closed struct's at the right depth (`UnifyContext.vouchers`
  and `path`). `Voucher::may_generate` is a static over-approximation over the
  comprehension body: static labels must match; pattern, dynamic and
  interpolated labels match anything.
- The credit is a `Provisional { name, vouchers }` on the struct. It is settled
  (`closedness::vouch_struct`) when re-derivation re-runs a voucher and what it
  generates declares the name, or a generated pattern matches it.
- A credit left at export fails as `x.n.d: field not allowed`
  (`export.rs`; `validate_json` via `unvouched_field`). A disjunction branch
  holding one is dropped (`allowed_branches`).

## A definition's fields stay closed when derived again

`close_deep` marks a definition's field conjuncts (`Conjunct::Closed`,
`Thunk.closes`) and recipes (`DeclRecipe.closes`). `derive_field_value` unifies
the definition's conjuncts of a field, closes them once, and only then meets
what the merge added: closing each conjunct alone would intersect them.
`definition_generation` does the same for what a definition's comprehension
generates; a generated field the definition declares nowhere else is closed
at once, so a user's subfield under it is checked (`#R & {a: true, s: y: 2}`).
A field it declares elsewhere too is derived again from all its conjuncts
(`derive_recipes`), not met as a closed value: `#G: {s: admin: {spec: 1}, if
class == "admin" {s: admin: lab: 1}}` allows `lab`, and a nested `#X` in its
place still refuses it.
`Conjunct::same` treats `Value(id)` and `Closed(id)` as one contribution, so a
retraction still finds a field the guard no longer generates.

Re-derivation now reaches what a merge put in list elements
(`rederive_value`; an element derived to an equal value keeps its id), in a
disjunction branch holding a credit, and in a field a pattern merged into when
that field holds a credit and the pattern brings a comprehension. Descending
into every pattern-matched field re-derived nested patterns on every pass
(`issue3857` went from 4 ms to 140 ms).

## Error paths

`BottomReason.path` is filled where a field collapses its struct
(`ValueArena::bottom_at` in `unify_structs_inner`, list elements, metadata
collapse, embed violations), innermost label first. Export prefixes the export
path: `task.timeout: bound target type mismatch for int`, `l.0.o.timeout: ...`,
list indexes dotted as upstream. Incomplete values keep `cannot export ... at
'x.name'`.

## Results

- 65 + 17 fixtures. On master 68 of the 79 compared ones differ (verdict,
  value or path); 3 more are stated divergences (below).
- Legacy txtar 513/547 before and after, same failure set.
- Corpus (release, `test-txtar`): same RSS (443 MB), user time +3%
  (0.86 s to 0.89 s). `issue3801` is the costliest file: `cue-rs eval` 0.58 s
  before, 0.68 s after on a quiet machine (list elements re-derived).
- Fixed on the way: `svcs: [...#Svc]` with `svcs: [{port: 9090}]` exported the
  default URL beside the overridden port (follow-up "Re-derivation through
  lists").

## Stated divergences

- **A sibling read in a comprehension body is stale** when the guard decides
  the same way: `P: {a: int | *0, if true {x: a}}`, `P & {a: 1}` gives `x: 0`.
  Not closedness; also on master and without definitions (follow_up.md).
- **A definition's own error** is reported where it is used (`x.b`), not at the
  definition (`#P.b`): cue-rs does not check unused definitions (phase 10).
- **Conflict after incomplete**: cue reports `t.a.b: conflicting values` before
  the incomplete `s.a.b`; cue-rs reports the first field it exports.
