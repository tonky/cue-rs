# Repeated-field unification

The user's explicit fix request authorizes this bounded evaluator repair.
`eval_single_decl` currently overwrites map entries and scope bindings. Reuse
the existing recursive unifier for repeated regular, dynamic, hidden and
definition labels; combine optionality with logical AND and bind the result.
Defer unresolved declarations so retries cannot permanently unify transient
unresolved-reference errors into an otherwise valid field. Preserve unresolved
errors on the final pass so export cannot silently omit a declaration.

Regression coverage: both declaration orders for nested service policies and
distinct services, conflicting policies (including a third declaration after a
conflict), scalar constraints, dynamic labels, definitions, hidden fields,
optionality and forward references. Run workspace tests, fmt and Clippy and
an independent review per the parent AGENTS.md.

Downstream: enve currently pins bfb40bc78b19878e004d843f6d8c1378facbc00d.
Validate against the local repair where possible. A durable git pin requires
committing and publishing cue-rs; parent instructions require approval for that
final publication step. Do not invent a revision or overwrite enve's dirty work.

## Implementation

Collect each static field's declarations into a unification expression before
evaluating references. Keep each expression's lexical scope: merging struct AST
bodies would incorrectly allow a field in one declaration to shadow an outer
policy referenced in another. Dynamic labels are resolved using the retry pass,
then normalized and evaluated again from the original scope so references see
all constraints. Runtime insertion also unifies overlapping entries.

Optional/optional conflicts remain optional bottom constraints; making the field
concrete later still fails. Unresolved fields are deferred until the final pass.
Nested dynamic-label failures propagate as unresolved values to enclosing retries.

Comprehension and embedding evaluation remains eager. Updates to already
referenced fields are explicitly rejected instead of exporting stale values;
this conservative guard may also reject redundant generated updates. Successful
generated/embedded merges update bindings for subsequent references. Embedding
conflicts propagate instead of being discarded. Deferred generated-field
dependency evaluation is tracked in follow_up.md.

## Validation and release status

- All 62 cue-rs workspace tests pass, including 12 new regression tests.
- Clippy with --workspace --all-targets -- -D warnings passes.
- Rustfmt checks pass for changed Rust files; unrelated formatting is preserved.
- Upstream CUE checks informed lexical-scope and optional-conflict regressions.
- Final independent review passed with no outstanding blocker among the
  reported findings; deferred generated/embedded evaluation remains documented.
- Two new enve integration tests and three existing environment-policy tests
  pass in an isolated copy with local cue-rs path dependencies.
- Clippy also passes for enve's new integration test and its dependencies.
- The new test is installed at enve/crates/enve-cue/tests/repeated_service_fields.rs.
- The user authorized committing and publishing cue-rs and will update enve's
  Cargo.toml/Cargo.lock pin themselves.
