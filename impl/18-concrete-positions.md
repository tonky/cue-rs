# Concrete positions

Positions that need a concrete value read whatever the evaluator held. An `if`
guard matched only `Value::Bool(true)` and skipped everything else, so a
`*true | bool` guard (grafana), an error and a non-bool all read as `false`;
`for` skipped non-iterable sources, dynamic labels dropped non-strings, and
index/slice/select ignored defaults. `==`/`!=` had arms for a few scalars only,
so `["a"] != []` was bottom, and a bottom guard was a silent `false` (n8n lost
the test job of 85 components). The fix makes every such position resolve the
default and fail loudly on anything else, pinned against `cue export`.

## Reading a concrete value

`operators::operand` selects the unique default. `operators::concrete` then
classifies the result: a value, pending (a reference not resolved yet - the
relaxation loop retries), incomplete (abstract, e.g. `bool` or a disjunction
without a default) or an error. Guards, `for` sources and dynamic labels go
through it (`Evaluator::comprehension_guard`, `comprehension_source`,
`dynamic_label`), returning `Clause::Ready` or `Clause::Incomplete`. A wrong kind
is an error with upstream's wording; an incomplete decision is recorded on the
struct and `export` fails with "incomplete value", as `cue export` does.

Builtin arguments resolve defaults through list elements (`operators::argument`),
so `list.Concat([l])` reads `l: *[1] | [...int]` as `[1]`.

## Equality and ordering

`operators::equal` compares null/bool/string/bytes by value, lists element by
element and structs by their regular fields (optional, hidden and definition
fields do not take part), selecting defaults inside them. Both sides must be
concrete all the way down first: `[int] == null` is incomplete, not `false`.
Different kinds are unequal. Strings and bytes order lexicographically; `=~`
accepts a bytes pattern. `list.Contains` and `list.UniqueItems` use content
equality instead of arena identity.

## Decisions that change after a merge

A struct-level comprehension and an undecided dynamic field keep a
`DeclRecipe` (source, captured environment, dependencies, outcome) on their
struct. When a merge moves one of the dependencies, `rederive_struct` re-runs
the recipe in the merged scope; if the decision differs, the old contribution is
retracted by conjunct identity and the new one merged. This is what makes
`#S: {d: bool, if d {x: 1}}` & `{d: true}` produce `x`, and a default override
flip a guard. A label decided by a default is rewritten into a single
`if true { (k): v }` comprehension so that it gets a recipe too.

## List arithmetic

`[1] + [2]` and `[1] * 2` are errors pointing at `list.Concat`/`list.Repeat`
(removed in v0.11). `list.Repeat(x, n)` takes a list, as upstream does.

## Interpolation nesting

The lexer and the parser counted parentheses to find the `)` closing `\(`, so a
`)` inside a nested literal ended it early (`"\(f(["'\(t)'"]))"` kept a stray
`)`). `token::interpolation_end` steps over nested literals of every form -
quoted, block, `#`-guarded, bytes - recursively, and over `//` comments; the
lexer and `Decoder::split` share it. A quoted literal may span lines inside an
interpolation (`token::breaks_line`).

## Which fields a reference can read

Upstream binds only fields declared with an identifier label in the struct
literal itself (`a:`, `#A:`, `_a:`). Quoted (`"a":`), dynamic (`("a"):`),
embedded and comprehension-generated fields are reachable by selection
(`s.a`, `s["a"]`) but no reference resolves to them. cue-rs bound all of them,
so `b: a` found a field upstream would leave unresolved, or found the wrong one
when an outer `a` existed. `Label::ident_name` is now the one predicate:
`collect_field_declarations`, the pending reservations and every
`insert_binding` site in `relaxation.rs`/`eval.rs` bind only through
`declares_field`. A quoted field that merges with an identifier field of the
same name (`"a": 1, a: int`) declares it, as upstream does.

Field aliases (`X=a: v`) are kept on `FieldDecl::alias` and expanded by
`schedule::expand_field_aliases` into a `let X = a` after the field. An alias
on a quoted, dynamic or pattern label is refused with an error rather than
bound wrongly.

## Unreferenced bindings and imports

`references::check_file` runs before evaluation, on every file `eval_file` and
the package loader read: an import whose name no identifier in the file reads
is `imported and not used: "<path>"`; a `let`, alias or field alias that
nothing in its struct literal reads is `unreferenced alias or let clause
<name>`. A `let` read only from its own expression counts as unreferenced, as
in upstream.

## Tests

`crates/cue-eval/tests/concrete_positions.rs` evaluates every fixture in
`tests/fixtures/concrete_positions/` and compares it with the golden `cue
export` output (`.json`, or `.err` when cue rejects the file), and runs a matrix
of every comparison operator over every value kind (scalars, null, bytes, empty
and non-empty lists, structs, defaults, ambiguous disjunctions, incomplete and
bottom values) against `comparisons.json`. `regen.py` regenerates all goldens
with the `cue` on PATH (v0.17.1).

Fixtures prefixed `scope_`, `alias_` and `unused_` cover the binding rules
above, each with and without an enclosing field of the same name.
`KNOWN_DIVERGENCES` in the test lists what still differs and asserts it still
does, so a fix has to remove its entry.

Remaining divergences, each an error and not a wrong value: bytes literals do
not interpolate; aliases on quoted labels and value aliases (`a: V={...}`) are
refused; an alias named like a field of an enclosing scope is accepted where
upstream rejects it.
