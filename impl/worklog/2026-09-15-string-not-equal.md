# String inequality bounds

Implemented exact string comparison for BoundOp::NotEqual. Equal strings produce
a constraint-violation diagnostic; unequal strings continue through every remaining
constraint and preserve the candidate ID. Existing regex and unresolved-bound paths
remain unchanged.

Added evaluator integration coverage for all requested expressions, both operand
orders, conjunction failures, literal metacharacters, case sensitivity, regex failures
and invalid patterns, and deferred constraints subsequently unified with candidates.
Committed package fixtures cover local and imported definitions, successful exports,
inequality violations and incomplete exports.

Validation:
- cargo test --workspace: 50 tests passed, plus doc tests.
- cargo test -p cue-eval --test string_not_equal: 4 passed after fixture refinement.
- cargo clippy --workspace --all-targets -- -D warnings: passed.
- Upstream cue v0.16.1 export: positive fixture JSON matched; local and imported
  negative fixtures rejected; unresolved imported definition remained incomplete.
- cargo fmt --all and its check passed; unrelated pre-existing formatting changes
  were then restored to keep the patch focused. New test file rustfmt check and
  git diff --check passed.
- Required independent review: no blocking findings; unrelated formatting cleaned up.

No dependencies added. No commits or pushes performed.
