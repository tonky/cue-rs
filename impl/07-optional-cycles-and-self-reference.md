# What is in a value: optional fields, self-reference, and a cycle's verdict

Three divergences from upstream `cue` v0.16.1, all found reviewing the
2026-09-22 recursive-schema report and none of them the OOM it described. They
are one phase because they are one question asked three times - *what belongs in
a value, and what is merely a constraint on it* - and because two of the three
are answered in the same function.

```cue
// 1. an optional field is not part of the value
#S: {a?: string, b?: {x?: int}, c?: [...string]}
v: #S & {a: "hi"}

// 2. a sibling is reachable through the field that encloses it
stages: {
	build: {name: "build"}
	test: {name: "test", needs: [stages.build]}
}

// 3. a structural cycle is an error, not a value
#Stage: {name: string, parent: #Stage}
s: #Stage & {name: "a"}
```

Upstream gives `{"v": {"a": "hi"}}`, both stages, and
`#Stage.parent: structural cycle`. cue-rs gives `v` with `b: {}` and `c: []`
beside it, `_|_ (unresolved reference 'stages')`, and a JSON string
`"parent": "<ref:#Stage>"`.

The second is the one a user meets first: referring to a sibling through the
field that encloses it is how every `depends_on` / `needs` graph is written. The
first is the one that shows up in every export of a schema with optional
structs - `enact`'s own example exports `ci: {}`, `jobs: {}` and `workers: {}`
that upstream omits. The third puts an internal placeholder into someone's JSON.

## 1. An optional field is exported when its value happens to be exportable

`to_json_at_path` treats `optional` as a hint rather than a verdict:

```rust
if entry.optional {
    if matches!(self.arena.get(entry.val), Some(
        Value::RecursiveRef { .. } | Value::Top | Value::Type(_) | Value::Bounds { .. }
            | Value::BuiltinValidator { .. } | Value::Validators(_))) {
        continue;
    }
    if let Ok(v) = self.to_json_at_path(entry.val, &field_path) { … }
}
```

The list is the set of values that are obviously not concrete. `Value::Struct`
and `Value::List` are not on it, so `b?: {x?: int}` - a struct whose own fields
are all optional, hence `{}` - and `c?: [...string]` - an open list with no
elements, hence `[]` - export. The scalar `e?: int` is a `Value::Type` and is
correctly dropped, which is why the bug reads as arbitrary.

CUE's rule has nothing to do with the shape of the value. An optional field is a
*constraint on a field that may appear*, not a field, and `cue export` never
emits one whatever it holds - `a?: 1` exports `{}`. The shape test is the wrong
question; the answer is already in `entry.optional`.

**Design.** Skip every optional field. The eleven-line branch becomes
`if entry.optional { continue; }`.

### Validated by spike

105 workspace tests green, corpus unchanged at 526/547, `v` exports `{"a":
"hi"}`, and `enact`'s `01-shell` example exports the four fields upstream emits
and none of the empty ones. Nothing depended on the old behaviour, which is the
answer to the obvious worry - that the shape test was compensating for an
`optional` flag set too widely somewhere upstream of export.

## 2. A field cannot see the field that encloses it

A static field's name is bound to its value *after* the value is evaluated:

```rust
let (val_id, conjunct) = self.eval_field_value(&f.value, env)?;
if is_unresolved && !final_pass {
    return Ok(false);          // pending; nothing is bound
}
…
self.insert_binding(name, val_id);
```

That is what makes a *sibling* forward reference work - `a: b` is pending on
pass 1, `b: 1` binds `b`, `a` resolves on pass 2 - and it is exactly what makes
a *self* reference impossible. `stages.build` is evaluated while `stages` is
being evaluated, so `stages` is not bound, and no later pass changes that: the
relaxation loop retries the declaration and learns nothing new, saturates, and
writes the bottom on its final pass.

Note what is *not* wrong. `is_unresolved` already walks into composites, so the
loop does see a struct with a bottom nested inside it and does retry. The
missing piece is only that a retry has nothing new to read.

**Design, part one: bind what the pass did resolve.** When a declaration goes
back on the pending list, bind its name to the partial value that pass produced,
merged with whatever earlier declarations of the same field already wrote:

```rust
if is_unresolved && !final_pass {
    if let Some(name) = f.label.name() && !f.label.is_definition() && !f.label.is_hidden() {
        let partial = match target_struct.fields.get(name) {
            Some(entry) => unify(&mut self.arena, entry.val, val_id),
            None => val_id,
        };
        self.insert_binding(name, partial);
    }
    return Ok(false);
}
```

Nothing is written into the struct - the guard above still returns before
`unify_decl_field` - so a transient bottom cannot win the field. Only the scope
gains a name, and only for the next pass.

**Design, part two: a transient bottom must not collapse its struct.** Part one
alone fixes a plain nested struct and repeated `stages:` declarations, and does
not fix the shape the report actually carries:

```cue
stages: [string]: #Stage
stages: build: name: "build"
stages: test: {name: "test", needs: [stages.build]}
```

`collect_field_declarations` folds repeated declarations into one binary
`&`, and `unify_structs_inner` collapses the whole struct when any required
field unifies to bottom:

```rust
if matches!(arena.get(unified_val), Some(Value::Bottom(_))) && !(e1.optional && e2.optional) {
    return unified_val;
}
```

So `test`'s unresolved reference turns the whole of `stages` into bottom, and
part one binds a bottom, out of which no sibling can be selected.

An unresolved reference is not a conflict. It is the evaluator saying *not yet*,
and the relaxation loop exists because it may say something else next pass.
Collapsing the struct for it discards the very siblings that would let it
resolve. So: keep a transient bottom at the field it belongs to, and collapse
the struct only for a real conflict. A reference that never resolves is then
reported at its own path - `x.r`, not `x` - which is how upstream reports it
too.

### Validated by spike

All five recorded shapes - nested struct, repeated paths, plain field reference,
pattern constraint plus cross-reference, and the same through an imported
package - export byte-for-byte what `cue export` v0.16.1 exports. 105 workspace
tests green, corpus unchanged at 526/547 with the same 21 failures. Genuine
errors still fail: an unknown name, a self-cycle `a: a`, a real conflict
`a: {b: 1}` & `a: {b: 2}`, and a two-field cycle all stay bottom, and all four
are errors upstream as well.

## 3. A structural cycle is exported as a placeholder string

`Value::RecursiveRef` is the node that makes a recursive definition terminate:
`resolving_symbols` catches a definition referring to itself while it is being
resolved and allocates a lazy reference instead of expanding. That part is
right, and it is why every recursive-schema shape in the report evaluates
instantly.

Export then does this:

```rust
Some(Value::RecursiveRef { name, .. }) => Ok(serde_json::Value::String(format!("<ref:{name}>"))),
```

An internal placeholder becomes a JSON string in a user's output. A required
field that is still a `RecursiveRef` at export is precisely a structural cycle -
the value is infinite - and upstream says so: `#Stage.parent: structural cycle`.

**Design.** Refuse it, naming the path: `structural cycle at 's.parent':
'#Stage'`. An *optional* recursive field is unaffected, because change 1 drops
every optional field before this arm is reached, which is also what upstream
does with `needs?: [...#Stage]`.

### Validated by spike

`s: #Stage & {name: "a"}` with `parent: #Stage` is refused; `needs?: [...#Stage]`
still exports `{"s": {"name": "build"}}`, matching upstream. Corpus and tests
unchanged.

## 4. A bottom does not say what kind of bottom it is

Three places already ask `message.contains("unresolved reference")` - the
relaxation loop's `is_unresolved`, a re-derived recipe deciding whether to keep
its old value, and the nested-struct carry in `eval_expr` - and stage 3 above
would be the fourth. A `BottomReason` carries a `String` and a path, and the
kind of failure is recovered by reading English back out of it.

It also loses a distinction upstream makes. cue-rs says `unresolved reference`
for four situations that upstream separates:

| input | upstream v0.16.1 | cue-rs |
| --- | --- | --- |
| `a: {x: nope}` | `a.x: reference "nope" not found` | unresolved reference 'nope' |
| `x: {p: 1}, x: {r: x.zzz}` | `x.r: undefined field: zzz` | unresolved reference 'zzz' |
| `a: a` | `a: incomplete value _` | unresolved reference 'a' |
| `a: {b: c}, c: a.b` | `a.b: incomplete value _` | unresolved reference 'c' |

The verdict is right in every row and the wording is wrong in three.

**Design.** A `BottomKind` beside the message:

```rust
pub enum BottomKind {
    /// Nothing in scope defines this name.
    ReferenceNotFound,
    /// The base resolves; it has no such field.
    UndefinedField,
    /// Not resolved *yet*. The relaxation loop may resolve it next pass, and
    /// one that survives the final pass is a cycle.
    Unresolved,
    /// An infinite value: a `RecursiveRef` that reached export.
    StructuralCycle,
    /// Two values that cannot both hold.
    Conflict,
    Other,
}
```

The kind is decided where the bottom is built, which is the only place that
knows. `Expr::Ident` can already tell `ReferenceNotFound` from `Unresolved`: the
literal's `ThunkEnv` carries `own_fields`, so a name this literal declares is
*not yet*, and a name nothing declares is *not found*. The selector sites
likewise know whether the base resolved.

The message then follows the kind rather than the other way round, and
`Unresolved` reaching export - which can only happen after the relaxation loop
has finished, so it is exactly upstream's "there is no more information" - is
reported as `incomplete value`. The three existing string tests become
`kind == BottomKind::Unresolved`, and so does stage 3's.

This is why this stage is in this phase rather than filed behind it: stage 3
would otherwise add a fourth string match, and the cycle wording cannot be
fixed without the distinction anyway.

## Stages

1. `BottomKind` on `BottomReason`, with the four existing string matches moved
   onto it and no behaviour change. Mechanical and large; it lands alone.
2. Optional fields are not exported, plus the `RecursiveRef` verdict. Both are
   in `to_json_at_path`, both are small, and neither moves the corpus.
3. Bind the partial value of a pending declaration.
4. Keep a transient bottom at its field in `unify_structs_inner`, on
   `BottomKind::Unresolved` rather than on a substring.
5. The messages: `reference "x" not found`, `undefined field: x`,
   `incomplete value`, `structural cycle`.

Stages 3 and 4 land together: stage 3 alone leaves the pattern-constraint shape
broken, and stage 4 alone has nothing to bind. Stage 5 is wording only, and is
last so that a message change never hides a behaviour change.

## Tests

A new `crates/cue-eval/tests/self_reference.rs` for stage 2/3 and
`optional_fields.rs` for stage 1, each asserting the upstream v0.16.1 value:

- optional scalar, struct, list, and nested-optional-only struct, dropped; the
  same fields required, present; an optional field made regular by a conjunct,
  present.
- a sibling read through the enclosing field, as a nested struct literal, as
  repeated `a: b:` paths, under a pattern constraint, and through an import.
- a reference resolved on a later pass is not left stale - the partial bound for
  pass *n* must not survive into the value if pass *n+1* changes it.
- the negatives: unknown name, `a: a`, two-field cycle, real conflict - each
  still bottom, reported at the path that carries it, and each carrying the
  `BottomKind` and the wording the table in section 4 gives it.
- a structural cycle refused; the optional recursion beside it exported.

## Not in this phase

- **`cycle with field`.** `a: a + 1` is a cycle through an operation rather than
  a reference, and upstream names it separately. cue-rs reaches the same
  verdict; only the wording differs, and the machinery to tell it apart is not
  the machinery stage 4 builds.
- **Subsumption of a bottom's path.** `BottomReason` already carries a `path`,
  and nothing populates it consistently. Reporting `a.x:` as a prefix the way
  upstream does is a presentation change worth doing once the kind is there.
