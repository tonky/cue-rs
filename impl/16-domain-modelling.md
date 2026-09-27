# Phase 16: domain modelling — newtypes, parse-don't-validate, functional core

Status: approved plan. Implements the review findings on upstream correspondence,
architecture and Rust idioms, in slices that each land with tests green.

## Problem

`String`/`bool`/`f64` carry invariants the compiler cannot see:

- `ImportDecl.path/alias`, `Label`/`Expr` idents, `Imports.aliases/packages`,
  `ModuleInfo.module` are all raw `String`. Emptiness and the `path:qualifier`
  split are re-checked (or not) at every consumer (`package.rs:244`,
  `eval.rs:251` duplicate the default-alias split).
- `Expr::Number(String)` defers literal validity to `number::literal`.
- `BoundOp` exists twice (`ast::{UnaryOp,BinaryOp}` vs `value::BoundOp`).
- `TypeKind` is 22 flat variants + `is_integer/is_unsigned/is_float` predicates.
- `Evaluator` (~15 fields, `eval.rs` 2398 lines) mixes binding, scheduling,
  builtins, I/O-adjacent package state and export support.
- `PackageLoader` mixes `fs::canonicalize`, parsing and evaluation; unit tests
  in `cue-eval/src/lib.rs` do real filesystem writes.

Target order (highest leverage first):

1. Import identity newtypes (`PackagePath`, `ImportAlias`).
2. Validated number literal (`NumberLit`).
3. Canonical `Bound` operator type.
4. `Evaluator` split (resolver / relaxation loop / builtin registry).
5. Filesystem trait for `PackageLoader` (keep `cue-eval` I/O-free).
6. `TypeKind` hierarchy — done as `TypeKind::Number(NumberKind)` with the
  lattice on the type (`subsumes`, `contains_int`, family predicates;
  `TypeKind` predicates delegate). Type-vs-Type meet is one subsumption
  check, narrower side wins, verified pair-by-pair equivalent to the old
  chain. The 12-arm concrete check collapsed to two arms; Bounds base
  merging keeps its distinct narrower rule via `narrows_number_base`
  (`number`→`int`/`float` only). Display strings unchanged; serde shape
  change (`"Int"` → `{"Number":"Int"}`) is the explicitly-migrated
  exception — no in-repo serialization consumers (`Value` is not serde).

## Invariants (each phase upholds)

1. Parse-don't-validate: a constructed AST value is usable without re-checking.
   Branching on validity lives in constructors, not consumers.
2. One owner per invariant: qualifier split, default alias, traversal rejection
   each live in exactly one function.
3. Serde compatibility: wire format of AST types is unchanged
   (`#[serde(transparent)]` on newtypes) unless a phase explicitly migrates it.
4. No behavior change without a conformance reason: `just conformance-baseline`
   must not move unexpectedly; snapshot updates are reviewed, fixtures untouched.

## Phase 1 — import identity (done)

- New module `cue-syntax/src/import_path.rs`:
  `PackagePath` (guaranteed non-empty, no control chars),
  `ImportAlias` (guaranteed non-empty), each with `as_str`, `Display`,
  `into_inner`, serde-transparent.
- `PackagePath::split_qualifier() -> (&str, Option<&str>)` owns the `:` split;
  `PackagePath::default_alias() -> ImportAlias` owns the trailing-segment rule.
  `package.rs` and `eval.rs` call these instead of inline `split_once/split`.
- `ImportDecl { path: PackagePath, alias: Option<ImportAlias> }`.
  The parser maps decode failures to `ParseError::UnexpectedToken`
  (no new error variant — KISS).
- Strict shape checks (absolute paths, `..` traversal) stay in `package.rs`:
  resolution guarantees resolvability, parsing guarantees non-emptiness.

Acceptance: `cargo test -p cue-syntax`, `cargo test -p cue-eval`,
`cargo clippy --workspace --all-targets` clean; no fixture changes.

## Later phases (one slice each, same pattern)

- Phase 2 (done): `NumberLit` validated in parser via the decode moved from
  `cue-eval::number` to `cue-syntax::number`; `Expr::Number(NumberLit)`.
  `eval_number` is infallible; wire format stays the spelling string;
  equality stays syntactic (by spelling).
- Phase 3 (done): single `Bound` type in `cue-syntax::bound`; `UnaryOp` /
  `BinaryOp` convert via `TryFrom`/`From`. `value::BoundOp` removed (renamed
  to `Bound`); the seven hand-matched unary arms in `eval.rs` collapsed into
  one conversion, with the `*v` default marker passing through.
- Phase 4 (started): extract `Resolver`, `RelaxationLoop`, `BuiltinRegistry`
  from `Evaluator` without changing evaluation semantics first. First slice
  done: pure scheduling helpers (`Sweep`, `pending_binding_name`,
  `collect_field_names`, `collect_field_declarations`) moved from `eval.rs`
  to `cue-eval::schedule`; `eval.rs` 2398 → 2320 lines. Second slice done:
  `Section`/`SECTIONS` and Kahn's `derivation_order` moved to `schedule`;
  `eval.rs` → 2233 lines. Third slice done: `refined_any` is now a pure
  function over arena + scope lookup (call site passes a closure), with a
  direct unit test; `eval.rs` → 2204 lines. Also recovered two doc comments
  the first slice had orphaned. Fourth slice done: `register_builtins`' 19
  hand-written bindings are now the `TYPE_BUILTINS` table in the new
  `cue-eval::builtins` seed module, built by a pure function of the arena;
  `eval.rs` → 2167 lines. Still inside: the `rederive_*` family and
  `eval_call` dispatch.

## Phase 4 remainder — resume notes (paused, not done)

What is left in `eval.rs` (~2167 lines): the `rederive_*` family
(`rederive`, `unify_and_rederive`, `rederive_value`, `rederive_metadata*`,
`derive_*`, `rederive_struct`, `rederive_children`, `rederive_sweep`,
`derive_field*`, `derive_thunk`) and `eval_call` dispatch.

Why plain moves stop working here: both thread `&mut self` through arena,
scopes, imports, closed-copies, origin and deferring flags on nearly every
line. The next step is not another extraction but owner types:

- `RelaxationLoop`: owns sweep state (`moved`/`wrote`, pass budgets
  `MAX_REDERIVE_SWEEPS`/`MAX_REFINEMENT_PASSES`), borrows arena + scopes.
  The `schedule` module already holds its pure pieces (`Sweep`,
  `derivation_order`, `refined_any`); the loop driver
  (`eval_decls_into`/`eval_decls_scoped`/`eval_decl_pass`) moves last.
- `BuiltinRegistry`: owns `eval_call` dispatch plus the `TYPE_BUILTINS`
  table seed in `builtins.rs`. Needs a narrow interface to arg evaluation
  (`eval_expr` on sub-expressions) rather than full `&mut Evaluator`.

Resume rule: each move must keep the full workspace suite green; the
relaxation loop has no dedicated unit tests today, so write
characterization tests for `eval_decls_scoped` pass counts (`derivations`/
`unsettled` counters exist for this) before moving the driver.
- Phase 5 (done): `FileProvider` trait in `cue-eval::fs` with `StdFs` and
  `MemFs`; all six internal loader functions take `&dyn FileProvider`.
  Public API unchanged (delegates to `StdFs`); new `*_with_fs` overloads
  (`find_all_module_roots`, `find_module_root`, `load_file_with`,
  `load_dir_with`). The four temp-dir unit tests now run on `MemFs` with no
  real filesystem I/O. `MemFs` limitation, documented: directories are
  implied by files, so empty directories do not exist there.
- Phase 6: `TypeKind::Numeric(IntKind)` / `FloatKind`; `unify` matches
  exhaustively, predicates become constructors' business.
