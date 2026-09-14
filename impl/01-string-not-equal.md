# String inequality bounds

The requested fix is a bounded change in unify_bounds: compare concrete strings
exactly for NotEqual and return a constraint violation for equal strings.
Keep the existing loop so all constraints must hold, and leave unresolved bounds
and regex handling unchanged.

Coverage: both operand orders, empty and nonempty exclusions, multiple exclusions,
literal regex metacharacters, regex match/nonmatch combinations, unresolved bounds
later unified with a concrete candidate, local definitions and imported definitions.
Use committed package fixtures for positive, negative and incomplete exports;
compare those against installed upstream CUE. Run workspace tests, fmt and Clippy,
then an independent review as required by the parent AGENTS.md.

The user's explicit fix request authorizes this small implementation.

## Results

Completed the seven-line NotEqual match arm and four integration tests covering
candidate evaluation, violation diagnostics, deferred unification and package exports.
All 50 workspace tests passed; the four regressions also passed after refining the
imported negative fixture. Upstream CUE v0.16.1 agreed on positive JSON and negative/
incomplete exports. Clippy with --workspace --all-targets -- -D warnings passed.
Independent review found no blocking issues.
