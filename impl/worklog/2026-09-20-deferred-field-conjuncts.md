# Deferred field conjuncts

A reference to a field whose value unification later replaced kept the value it
first saw. `{p: int | *5, c: "v\(p)"} & {p: 9}` exported `p: 9` beside `c: "v5"`,
so an enve service preset listened on the overridden port while handing the
application the default one. Eleven cases in
`crates/cue-eval/tests/defaulted_field_references.rs`, each expectation taken
from upstream `cue` v0.16.1; a twelfth covers the nested-binding shape found
later.

## What was built

A field now keeps the conjuncts it was built from - an evaluated value, or a
thunk holding the expression and the environment its literal captured - and
struct unification concatenates the two lists, as upstream does with vertex
arcs. `val` stays a cache, so export and the unifier are untouched until a
refresh runs. Refresh re-forces the thunks whose reads went stale against the
merged struct and replaces the cached value; replacement rather than unification,
because the stale cache came from the same thunk and `"v5" & "v9"` is bottom.
Copy on write, so a struct nothing touched keeps its ids.

Two refinements were not in the plan and both came from the same place, a reader
that resolves a name somewhere other than where it was written.

Levelled reads. A read records the scope level that answered for it, counted
outward from the literal. Without that, an inherited `let` resolves its
dependency at whichever literal is being forced: `let servicePort = port` beside
`healthCheck: {port: ... | *servicePort}` re-derived forever, which is what first
exhausted memory on enve's suite.

Content, not identity. A field is stale when a name it read resolves elsewhere,
and comparing ids alone answers that wrongly - deriving one expression twice
gives two ids for one value, so every re-evaluation marked every reader stale and
each re-force rebuilt a whole definition. `unify::values_equivalent` compares the
values, counting a pair already under comparison as equal so recursive schemas
terminate, under a node budget that reports a difference rather than paying an
unbounded walk.

## Trade-offs

Identity comparison is one instruction and content comparison is a walk. It is
paid only when the ids differ, and it is what stops the cascade that made the
walk expensive in the first place: enve's inlined 3.2k-line schema corpus went
from 4.1 GB / 4.1 s to 19 MB / 22 ms, against 153 MB / 37 ms before the phase.
The budget makes an oversized value compare as different, which costs one
needless derivation, never a wrong one; follow_up.md carries the cached-hash
alternative if that ever shows up.

A thunk holds an `Rc<Expr>` and a per-literal `Rc<ThunkEnv>`, so the model costs
one env clone per literal rather than per field. `FieldEntry` keeps a manual
`PartialEq` that ignores conjuncts, so value equality keeps its meaning.
`package.rs` drops conjuncts when cloning across arenas - their ids belong to the
source arena - which is sound because a cloned package is already merged.

## Diagnosis worth keeping

The first suspect for the memory blowup was refresh running at every `&`; the
standalone reduction did not reproduce it even with the old code, so that was
wrong. The second was a stale test binary, which a rebuild cleared: the enve
failures at that point were not real. What found the cause was counting - 2.6 M
refresh visits, 548 k conjuncts and 1.09 M arena allocations from a file with
about 2 k declarations, against 2 k conjuncts with the per-field refresh disabled
- and then tracing which reads were considered stale. They were all `port` and
`dataDir`, disjunction to disjunction, same content, different id.

## Validation

74 workspace tests green. 527/547 txtar, five fixed against the baseline
(`comprehensions_issue2171`, `comprehensions_issue4423`, `definitions_root7`,
`definitions_root8`, `eval_merge`) and none new; the failure set is identical
before and after both refinements. Clippy with warnings denied; rustfmt on the
changed lines, leaving the pre-existing drift in `lib.rs` and
`stdlib/encoding.rs` alone.

Downstream, in an isolated enve copy on local path dependencies: all 74
`enve-cue` tests green, the same set the pre-phase engine passes.
`repeated_service_fields` 0.45 s (55 s mid-phase, 0.12 s before),
`transpiler_integration` 0.17 s (15.7 s, 0.06 s). The posthog example exports
byte-identical JSON to the pre-phase engine.

The corpus costs 1.09 s against 0.75-0.87 s at HEAD, at 1.61 GB against 1.58 GB.
An earlier entry here said 1.14 s against 1.06 s at identical memory; that
comparison used a stale baseline binary, and the real refresh cost is 1.4x.

A `justfile` lands with the phase, per AGENTS.md: test, lint, fmt, txtar,
sync-upstream, eval, ci. Its `ci` recipe fails at `fmt-check` on a clean
checkout: `lib.rs`, `stdlib/encoding.rs` and `cue-wasm/src/lib.rs` carry rustfmt
drift this phase deliberately leaves alone.

## Review outcome

The independent review confirmed the conformance and downstream numbers and all
three stages, and found four defects the corpus does not reach: a lexical level
indexed into a value-depth chain, reads keyed by name alone and all attributed to
level 0, a conjunct-less merge side dropped on re-force, and a pass cap that
reports a cycle for a dependency chain longer than 16. Two are silent wrong
answers and two are hard errors on files HEAD accepts. Each was reproduced
against upstream v0.16.1 and a freshly built HEAD; 03-deferred-field-conjuncts.md
carries the detail.

The lesson for the numbers above: a comparison is only as good as its baseline
binary. The stale one also hid the f1a regression, which HEAD gets right.

Not committed. The fix is a rework of how a read is recorded and resolved,
planned as phase 04; the enve pin stays user-owned.
