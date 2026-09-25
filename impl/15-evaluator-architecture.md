# Phase 15: evaluator architecture and Rust API

Status: roadmap approved. Beneficial API breaks explicitly permitted by the user.

## Architectural direction

Introduce the domain boundaries needed by phases 12-14 as those changes are
made. This phase completes the API migration, removes superseded paths and
verifies the boundaries across the crate. A file split alone is not success.

Observed coupling at d16afa0:

- eval.rs is 2,480 lines and combines lexical binding, scheduling,
  comprehensions, builtin calls, unification orchestration and JSON export.
- unify.rs is 1,503 lines and combines value comparison, field admission,
  scalar constraints, disjunction trials and recursion guards.
- Public Value/FieldEntry reach Rc<Expr>, Rc<ThunkEnv> and RefCell state.
  Evaluator exposes a mutable arena, placeholders and import state.
- Pending references and final errors share Bottom, while readiness is
  inferred through recursive walks with hard depth limits.
- ValueId keys identify slots but do not carry the identity of their owning
  arena. A raw public handle is not a self-contained evaluated value.
- Package loading performs filesystem work and evaluation; the CLI also
  owns conformance policy. SOURCE_MAP.md contains stale ownership claims.

## Target responsibilities

| Boundary | Responsibility |
| --- | --- |
| Source/program | Source IDs, spans, symbols, compiled expressions, lexical binding |
| Package resolver | I/O, module/package identity, per-file imports, dependency loading |
| Evaluation session | Work scheduling, dependency tracking, scoped state, request limits |
| Value store | Values, stable internal IDs, sharing and explicit ownership/lifetimes |
| Unification | Constraints, defaults, closedness provenance, branch outcomes |
| Diagnostics | Typed causes, source locations and structured value paths |
| Observation/export | Lookup, abstract-value inspection, validation and concrete serialization |
| Test adapter | Operation-specific oracle comparisons and coverage accounting |

These are module boundaries first. Split crates only where dependencies and
consumers justify it; avoid a framework of traits around internal operations.

## Domain invariants

1. Evaluation progress and semantic value are distinct. A ready value may be
   abstract; a pending dependency is not a contradiction; an execution limit
   is not a proof of a cycle.
2. A recipe has a stable source expression, lexical environment and resolved
   dependency identity. String spelling alone is not binding identity.
3. A value handle cannot be used against the wrong store through the safe
   public API. Prefer a borrowed view or an owned evaluation/result object.
4. Closedness/field admission records its origin instead of combining
   contradictory booleans. Required, optional, definition and hidden fields
   have explicit semantics at binding, merging and observation.
5. A speculative branch's allocations and cache effects have one owner.
   Rollback cannot invalidate a retained result or leave a stale cache entry.
6. Comparison budgets and unsettled work remain observable. Unknown
   equivalence cannot silently mean equal, and a scheduling cap cannot
   silently mean successful completion.

## Public API and migration

- Provide a small entry point for source/package evaluation, explicit
  evaluation/validation options, typed diagnostics and JSON export.
- Support partial evaluation and lookup for the harness and embedding users
  without exposing mutable maps or undocumented arena internals.
- Allow breaking low-level exports where that establishes invariants. Keep
  simple convenience functions if they remain honest adapters.
- Enve directly uses PackageLoader, LoadOptions, OriginAnnotation,
  EvalError, eval_to_json and validate_json. Port and test that boundary.
  Enact currently consumes enve's CLI JSON/exit behavior; test that route.
  Include cue-cli, cue-derive and cue-wasm in the migration.
- Preserve origin annotations as an explicit loading option.
- Decide thread semantics deliberately: immutable compiled inputs may be
  shared; one evaluation owns its mutable state. Do not add locks merely to
  promise Sync, or replace every Rc with Arc without measuring.
- Downstream revision pins and publication remain user-owned. Test local
  integration without silently updating release dependencies.

## Stages and completion

1. Extract observation/diagnostics and scoped execution boundaries alongside
   phase 12; keep behavior covered by the corrected oracle.
2. Introduce scheduling/value states and unification invariants alongside
   phase 13, with minimized compatibility regressions.
3. Consolidate compiled-expression/environment/store ownership alongside
   phase 14, preserving its allocation and RSS improvements.
4. Migrate public callers, remove obsolete implementation paths and update
   source-map/API documentation. Audit remaining broad visibility, unchecked
   growth, string-based error control flow and unnecessary cloning.

Completion requires the conformance and memory gates, workspace tests,
formatting, Clippy and both downstream integrations. Architectural changes
must have a demonstrated invariant, allocation benefit or reduction in
coupling; no unrelated dependency upgrades or module churn are needed.

## Boundaries introduced so far

Concrete export is now a shared observation module used by the evaluator,
builtin encoders, CLI and wasm. Bytes have an octet AST type. Lexical scope
maps are encapsulated by `ScopeFrame`, with read-only captures and copy-on-write
updates. A session-owned `ExpressionStore` shares exact immutable syntax across
recipes while each thunk retains separate bindings/imports. Binding-free
recipes store values directly. These establish ownership boundaries but do not
complete compiled-program lowering, typed execution state, diagnostics, handle
ownership or downstream migration. The phase remains open.

Scalar operators and numeric literal decoding now have separate modules from
lexical evaluation. The expression store caches immutable direct dependencies;
lexical let closures remain recipe-local. These are functional/domain boundaries,
not a complete Program/Session/value-store API migration.

Declaration construction now has a private `DeclarationValue` accumulator,
separate from the completed semantic `Value`. Root evaluation returns a ValueId;
package loaders no longer force it into a struct. Comprehensions merge their
construction state directly, retaining embedded constraints without extra arena
allocation for their structural bodies. Persistent scalar metadata and embedded
recipes still need a broader value-store model; this boundary does not complete
that migration.

Scalar metadata now has sparse arena ownership: ValueId identifies the semantic
payload plus retained fields and embedded conjuncts. `ValueArena::get` reads the
payload; `fields` reads the value's field view for scalar/list/struct selectors.
Rollback removes metadata with its owner. Unification keeps cached embedded
results separate from their original recipes; the evaluator refreshes them in
the merged lexical context before settling affected choices. Package selectors,
closing and observation use the same ownership boundary.

Mixed-choice normalization adds an explicit `MetadataSource` distinction:
embedded constraints versus common-field selector views. A view never adds a
closed conjunct; actual admission is checked in each alternative. Both records
remain sparse arena-owned data with rollback tied to the owning ValueId. General
branch recipe provenance remains necessary for dependency-bearing choices.

Dynamic choices now retain whole-expression closing groups instead of creating
recipes from cached alternatives. `MetadataSource::Closed` references original
conjuncts and a recursive/outer/contents-only closing mode. A subsequent meet
keeps that group as a value conjunct; the metadata evaluator projects new inputs
onto its original declarations and applies closing before outside constraints.
The group remains immutable, so specializations do not change their source.
Recipe inputs and materialized selector views are separate IDs. Struct selectors
read materialized struct fields; scalar selectors use their retained view.
Re-derivation checks both payload and selector view before keeping a cached result.
Group descent is bounded and tracks active IDs; normal branch normalization keeps
its existing cycle/depth guards. Ordinary arena values gain no metadata slots.


Field entries now share immutable `Rc<[Conjunct]>` recipe sequences. Appending a
constraint builds a new sequence, keeping earlier snapshots and lexical captures
intact. Plain boolean/string constructors share arena-local values; fresh `alloc`
and metadata owners remain distinct. Cache invalidation is tied to mutable access
and rollback, and borrowed string lookup avoids allocating on cache hits. Public
mutable graph access still affects every alias; it is not an owned-result API.
Opt-in all-slot profiling is separate from the default evaluation path. Remaining
ownership and root-inventory work is summarized in [CONTINUATION.md](CONTINUATION.md).
