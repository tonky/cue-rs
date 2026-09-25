# Plain scalar roots and embeddings

## Context and design

Continuation of the approved phases 13/15. Every source file and nested block
was forced into StructValue, so a root string and `{2}` failed before export.
Investigation of pinned upstream `scalars_embed` also showed why erasing fields
is not a solution: scalars can carry selectable definitions and recipes. Split
out the plain-value construction boundary now; retain the metadata cases as
explicit future work in phase 13 and follow_up.md.

`DeclarationValue` holds a structural body and embedded constraints separately.
It preserves explicit empty structs, top, kinds, defaults and multiple conjuncts.
Root evaluation returns ValueId; both package loaders preserve that kind. Origin
annotations still apply to struct roots. Nested embedding contradictions remain
Bottom values, allowing valid sibling observations instead of a package-load
failure. Non-null bounds can be discharged against non-null kinds while keeping
any other constraints and the bound's base type.

Full-corpus review caught regressions in forward definition embeddings and
comprehension bodies with disjunctions. Fixed before accepting the family:
placeholders wait, and a final inner pass leaves an unresolved bottom for the
outer retry loop. Comprehensions merge construction state, including scalar and
disjunction constraints; pending constraints retry rather than freeze. This also
avoids allocating/cloning a completed arena struct for each comprehension body.
Closed embedding behavior remains covered by prior tests and reference controls.

## Validation

- 214 ordinary workspace tests pass; all five ignored reference controls were
  explicitly run (219 unique tests total). The reference test binary reports
  eight passes, including three ordinary controls already counted above.
- Clippy with warnings denied passes; formatting and diff whitespace checked.
- All 547 original fixture hashes preserved. Final corpus report:
  `tmp/embedding-corpus3.json`.
- 616 -> 725 passed; 1091 -> 756 mismatches across 178 archives;
  3174 -> 3400 unsupported; one reference-annotation disagreement and
  603 not applicable. 27/547 complete archives, up from 25.
- No previously passing check regressed and no new mismatch appeared.
  105 mismatches and four unsupported checks now pass. Another 230 mismatches
  become unsupported because evaluation now reaches their abstract observation
  boundary. These are verification debt, not successful compatibility checks.
- Reduced API/reference cases cover root scalar/list/default values, multifile
  constraints, empty struct/top, forward lets/definitions, nested conflicts,
  non-null kinds/base types, re-derivation and comprehension constraints.

Logs: `tmp/embedding-workspace.log`, `tmp/embedding-oracle3.log`,
`tmp/embedding-clippy.log`. Earlier numbered corpus reports record the rejected
intermediate regressions; only corpus3 is accepted.

## Remaining limits

Scalars with definition/hidden/optional/pattern metadata need a persistent
vertex model and embedded recipes, including recipe re-derivation after metadata
changes. This slice does not implement those semantics. Existing field admission,
per-file scope, exact decimals, pending disjunction and diagnostic gaps remain.
No downstream revision pins changed and no commit/publication was performed.

## Memory and legacy signals

Three release runs per Odoo variant, all under the 2 GiB process-tree cap with
swap disabled. Median/max RSS (KiB): literal 171960/171980, refs 301952/301992,
original 1003600/1003652. Median times: 0.38/0.68/2.47 seconds respectively.
Both valid variants match saved upstream JSON; original rejects normally.
Report `tmp/embedding-odoo.json`, log `tmp/embedding-odoo.log`. Memory is effectively
unchanged; original still exceeds the planned 512 MiB target.

The legacy heuristic runner is now 521/547 (26 failure signals): 18 of its old
failures disappear, one additional name appears (`definitions_explicitopen`).
That archive was already incompatible in the corrected corpus (100 mismatches,
70 unsupported checks, seven implementation observations). The legacy runner
checks each file separately and matches error wording; reaching export now
surfaces a missing cross-file name instead of its expected closedness wording.
No conformance observation regressed. Keep the original 43-name inventory intact;
do not adjust heuristics to make the new failure disappear. Log:
`tmp/embedding-legacy.log`.
