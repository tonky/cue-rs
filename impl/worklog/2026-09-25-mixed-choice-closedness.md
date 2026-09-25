# Fixed mixed-choice admission and selector views

Continuation of approved phases 13/15. No commit, downstream edits, publication
or dependency-pin changes. All evaluator runs, builds and tests used `just safe`
with a 2 GiB process-tree cap, zero swap and a 180-second timeout.

## Diagnosis and design

`#Mixed: {*1 | {f:int}, #x:int}; b:#Mixed & {f:3}; bf:b.f` incorrectly
rejected `f`. The payload branch and its common metadata closed separately;
metadata then imposed a second closed body admitting no regular fields. Simply
opening metadata would make closed top admit undeclared fields, while pooling
alternative field names would admit `{f:3,g:4}` across separate branches.

Normalize fixed choices before closing, attaching common fields to each branch.
A `MetadataSource::ChoiceFields` record retains a common selector view without
being another constraint. `Embedded` retains actual conjuncts. Unification,
opening/reclosing, deep closing, re-derivation, equivalence and rollback preserve
that distinction. Existing branch-private fields survive distribution; common
optional/pattern fields participate in each branch's closing boundary.

Only constant conjuncts or unshadowed predeclared type bindings normalize.
Copying a whole-expression thunk into every branch or replacing it with a cached
default would freeze or otherwise alter later specialization. Dependency-bearing
choices therefore retain the existing recipe path and remain an explicit follow-up.
The mutable arena can also contain cyclic/over-deep choice graphs; normalization
tracks active value/field pairs and has a depth limit. A regression covers both
branching cycles and excessive acyclic depth. `UnifyContext` has a new tracking
field; direct struct-literal consumers should prefer its constructor/Default.

## Reference regressions

The saved repro moved from `open/metadata-mixed-choice.txtar` to
`regressions/metadata/mixed-choice.txtar`. Together with mixed-neighbors,
mixed-views and mixed-recipes, 29 observations pass the pinned Go v3 reference.
They cover both operand orders, distinct branch admission, optional/pattern
fields, nested closing, open branches, embedding, branch-private metadata,
selectors after narrowing and default/lexical-shadow specialization controls.
Two API tests cover evaluation/typed conflicts and field-view rollback.

The new `open/metadata-dynamic-mixed-choice.txtar` preserves the remaining case:
`#Dynamic: {*(#x+1) | {f:#x}, #x:*1|int}`. Scalar specialization is verified;
struct specialization is still over-closed. The report has one passing scalar
check, one unsupported struct-field observation and one export mismatch. This
is not a passing regression or part of the unchanged 547-fixture corpus.

## Validation

- 223 workspace tests and six additional explicit pinned-reference controls pass
  (229 unique tests). The reference invocation also reruns three ordinary tests.
- Formatting and Clippy across all targets are clean.
- Every original corpus observation matches the preceding baseline: 745 passed,
  754 mismatches, 3382 unsupported, one oracle disagreement, 603 not applicable.
  27/547 complete archives remain verified. No check was reclassified to claim
  success, and the baseline was not rewritten. All 547 fixture hashes match.
- Logs/reports: `tmp/mixed-choice-{workspace,oracle,clippy,corpus,open,odoo}.*`.

General closedness provenance, dynamic branch recipes, arena reclamation and the
512 MiB original-Odoo target remain open. See the phase docs and follow_up.md.

Final release measurements (three runs each, saved upstream JSON checked):

| Variant | Median RSS KiB | Max RSS KiB | Median seconds |
| --- | ---: | ---: | ---: |
| literal | 171900 | 172068 | 0.38 |
| refs | 301868 | 302096 | 0.68 |
| original | 1003648 | 1003916 | 2.49 |

Outputs/verdict preserved, RAM effectively unchanged from the previous family.
All runs stayed under 2 GiB; original still rejects normally at about 980 MiB.
The separate legacy harness retains the same 520 passes and 27 failure names;
its nonzero exit remains expected and is not the semantic acceptance gate.
