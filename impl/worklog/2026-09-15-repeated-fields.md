# Repeated-field unification

Reproduced silent replacement with six failing integration tests before the fix.
Static declarations now collect into conjuncts before evaluation, dynamic labels
resolve then regroup, and runtime collisions recursively unify. Scope bindings
receive the result. Optional conflicts stay constraints until made concrete.

Independent review identified stale references, premature nested dynamic-label
errors, and lexical-scope/empty-struct pitfalls. Regressions cover these cases.
Struct expressions remain separate rather than flattening their declaration
bodies, matching upstream CUE lexical lookup. Review also found eager generated
updates could leave stale aliases; these and embedding updates now fail explicitly
when previously read values would be invalidated. Full deferred support is open.

Validation: 62 workspace tests; Clippy with warnings denied; changed-file rustfmt.
Two new enve integration tests plus its three existing policy tests passed in an
isolated checkout copy using local cue-rs dependencies. Added the new test file
to the sibling enve repository without replacing its existing changes.

The user subsequently authorized committing and pushing cue-rs. They will handle
the enve dependency pin and lockfile update themselves.
