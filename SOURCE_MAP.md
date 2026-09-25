# Architecture and source map

This map describes the current implementation. The remaining architectural work
is tracked in [phase 15](impl/15-evaluator-architecture.md); the current evaluator
is not yet a compiled program with a separate execution session.

## Workspace

| Crate | Responsibility |
| --- | --- |
| `cue-syntax` | Lexer, parser, owned syntax tree, formatting and visitors |
| `cue-eval` | Evaluation, unification, values, package loading, builtins and export |
| `cue-cli` | `cue-rs` commands and isolated conformance worker |
| `cue-derive` | `CueValidate` derive macro |
| `cue-wasm` | Evaluation, validation and formatting bindings |
| `cue-test-harness` | Txtar parsing, legacy checks and pinned-reference comparisons |

The Go adapter in `tools/conformance-oracle` is a test dependency only. It is
pinned to the revision matching all 429 imported archives. The 118 other archives
have local provenance. See [conformance](tests/conformance/README.md) for the
assertion baseline and unsupported verification; legacy passes do not establish
language conformance.

## Source ownership and evaluation

- [ast.rs](crates/cue-syntax/src/ast.rs) owns parsed expressions, declarations
  and formatting metadata. `StringLit` holds Unicode text; `BytesLit` holds
  arbitrary octets. Derived AST equality/hashing compare syntax, not CUE values.
- [parser.rs](crates/cue-syntax/src/parser.rs) handles expression precedence,
  declarations and literal decoding; [formatter.rs](crates/cue-syntax/src/formatter.rs)
  preserves supported source forms and byte values through reparsing.
- [eval.rs](crates/cue-eval/src/eval.rs) owns execution state, forward-reference
  relaxation, comprehensions, builtin dispatch and re-derivation after merging.
- [declaration.rs](crates/cue-eval/src/declaration.rs) keeps declaration construction
  separate from completed values, including scalar/disjunction embeddings.
- [scope.rs](crates/cue-eval/src/scope.rs) encapsulates lexical binding maps.
  Captures share immutable frames; evaluator writes copy a shared map first.
- [expression.rs](crates/cue-eval/src/expression.rs) owns a session-local store
  of exact syntax trees. Recipes share identical syntax, with separate lexical
  environments, imports and results. It does not memoize evaluated values.
- [deps.rs](crates/cue-eval/src/deps.rs) conservatively collects identifiers and
  follows let dependencies. Binding-free recipes retain a value constraint;
  dependent recipes retain their expression and lexical environment. Direct
  dependency sets are cached per expression; let closures remain scope-specific.
- [operators.rs](crates/cue-eval/src/operators.rs) selects scalar operands and
  executes arithmetic/comparisons; [number.rs](crates/cue-eval/src/number.rs)
  decodes integer/radix/SI literals without machine-integer overflow.
- [package.rs](crates/cue-eval/src/package.rs) performs filesystem/module
  resolution, package assembly and opt-in origin annotations. Imported values
  are evaluated into the importing arena, preserving their recipes.

## Values and observation

- [value.rs](crates/cue-eval/src/value.rs) defines values, field conjuncts,
  captured environments and the generational `ValueArena`. Generations detect
  removed slots; IDs do not encode arena identity. The public API still permits
  constructing a handle/store mismatch.
  Boolean/string constructors share unannotated values; `alloc` always creates
  a fresh node. Mutable access changes every alias and evicts string cache keys;
  rollback removes keys with their owners. Field copies share immutable recipe
  slices; adding constraints builds a separate slice. Optional allocation
  attribution lives in [value/profile.rs](crates/cue-eval/src/value/profile.rs).
- [metadata.rs](crates/cue-eval/src/metadata.rs) completes values with retained
  fields and embedded recipes. Arena metadata is sparse and rolls back with its
  owner; `get` reads payloads and `fields` exposes scalar/list/struct fields.
  `MetadataSource` distinguishes embedded constraints from selector-only common
  fields on normalized choices. Closed recipe groups retain their original
  declarations and closing mode; input fields are separate from branch-local
  selector views.
- [unify.rs](crates/cue-eval/src/unify.rs) implements constraint intersection,
  branch trials, field merging, bounds, validators and budgeted value comparison.
  Field recipes remain distinct from semantic value equivalence.
- [closedness.rs](crates/cue-eval/src/closedness.rs) produces and caches closed
  copies when definitions are referenced.
- [export.rs](crates/cue-eval/src/export.rs) implements concrete JSON export and
  the exact-number YAML bridge. Evaluator exports and builtin encoders share
  defaults/optionality policy. Bytes become base64 and large integers stay numbers.
- [stdlib/](crates/cue-eval/src/stdlib) contains package dispatch and builtin
  implementations. [importer/](crates/cue-eval/src/importer) converts JSON Schema
  and OpenAPI to CUE; [manifest.rs](crates/cue-eval/src/manifest.rs) handles module
  manifests.

The arena's allocation trail supports speculative rollback. It does not collect
all unreachable intermediate values. Captures and expression storage are scoped
to evaluator/recipe ownership, with no global cache. Evaluation remains
single-threaded and uses `Rc`; no `Send`/`Sync` contract is implied.

Pending references, incompleteness, conflicts and resource limits still have
partially shared error representation. Depth and sweep limits do not establish
semantic convergence. Decimal arithmetic still uses f64. These and the public
mutable arena are open design work, not safety or conformance guarantees.

## Verification

Run builds, tests and evaluator experiments under the process-tree cap:

```sh
just safe just test
just safe just lint
just fmt-check
just safe just oracle-build
just safe just oracle-test
just safe just conformance-test
just safe just conformance-baseline
just safe just benchmark-odoo ../cue-rs-odoo-repro
```

`conformance-baseline` detects changed outcomes; it is not strict acceptance.
`just safe just conformance` reports the remaining mismatches and unsupported
checks and exits nonzero. The baseline is only updated after reviewing changes;
original fixture contents remain unchanged. Benchmarks record repeated runtime,
RSS, binary/source hashes and output consistency in `tmp/odoo-benchmark.json`.
Pass `--reference-dir` to the benchmark script to check saved upstream JSON too.


Allocation attribution is opt-in and allocates scratch sets; it is separate from
production RSS measurements:

```sh
just safe cargo run --release -p cue-eval --example memory_profile --features memory-profile -- ../cue-rs-odoo-repro/original
```

The report includes every allocated node (including unreachable intermediates),
unique field recipe sequences, captured frame storage and exact duplicate counts.
Its selected buffer sizes are not a total heap estimate or a GC root inventory.
