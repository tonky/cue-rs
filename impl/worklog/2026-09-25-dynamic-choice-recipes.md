# Dynamic choice recipes and closing boundaries

Continuation of approved phases 13/15, following the fixed-choice family.
All builds, evaluator runs and tests used `just safe`: 2 GiB process-tree cap,
zero swap, 180-second timeout, one build/test job. No commits, publishing,
downstream edits or dependency-pin changes.

## Diagnosis and implementation

The saved `#Dynamic: {*(#x+1) | {f:#x}, #x:*1|int}` repro could specialize its
scalar default but incorrectly rejected its struct branch. Closing cached
branches separately from the common metadata lost the literal's closing boundary.
Distributing a whole-expression thunk into cached alternatives would either
freeze defaults or give each branch the wrong recipe.

Retain the whole expression in a closed recipe group instead. Metadata records
recursive closing, outer-only closing, or recursive closing with the outer
boundary reopened for embedding. Later unification preserves the group as a
conjunct and merges its inputs. Re-derivation projects those inputs onto the
group's original declarations, re-evaluates the expression, adds those declarations,
and closes it before meeting outside conjuncts. New outside fields cannot expand
that group's admission. Struct and bottom caches retain their original recipes.

Recipe inputs and materialized selector views are separate arena IDs. This keeps
branch-private definitions selectable without freezing them as inputs to a future
branch evaluation. Struct selectors use the materialized struct body; scalar
selectors use the retained view. Copies, equality, rollback and closing preserve
the distinction. A stability check includes the selector view as well as payload.

Explicit `{}` and regular fields impose struct kind through a constant empty
struct conjunct; their values refresh separately. Storing the current complete
struct there would freeze dependent fields. Recipe-group descent is bounded and
tracks active IDs, alongside the existing choice-normalization guards.

## Corpus regression caught during review

The first full matrix regressed `embedding_disjunction_resolution.txtar`:
re-evaluating the embedded `#AppEnv` returned closed branches and then rejected
`name`, which the receiving literal declares. Embedded expression thunks now
reapply the outer opening used on their first evaluation; the receiving literal
then adds its fields and closes normally. A reduced reference regression covers
this case. The rejected report is `tmp/dynamic-choice-corpus.json`; the accepted
report is `tmp/dynamic-choice-corpus-final.json`.

## Validation and counts

- The saved open repro moved to `regressions/dynamic-choice/basic.txtar`.
  All 34 observations in six reduced fixtures pass the pinned Go v3 reference.
  Coverage: both operand orders, scalar/struct/default specialization, source
  isolation, dynamic labels, nested/open branches, patterns, independent closing
  boundaries, embedding, explicit struct constraints and private branch fields.
  Default-valued `base` in required.txtar is checked by concrete export; the
  oracle's abstract-value observation is unsupported and is not called a pass.
- 225 workspace tests plus six explicit reference controls pass: 231 unique tests.
  The reference invocation also reruns three ordinary CLI tests. Two new API
  tests exercise copy isolation, changing labels, private selectors and kind
  constraints. Clippy across all targets and formatting checks pass.
- Accepted corpus: 746 passed, 751 mismatches across 174 archives, 3384 unsupported,
  one oracle disagreement, 603 not applicable. Still 27/547 whole archives.
  `definitions_033_Issue_#153/in.cue:0001` changed from mismatch to passed.
  `builtins_matchn/in.cue:0025` and `0026` now reach abstract observations instead
  of missing paths; these remain unsupported, not successful compatibility fixes.
  No old pass regressed and no new mismatch/crash/resource failure appeared.
- All 547 original fixture hashes are unchanged. The reviewed migration baseline
  was updated explicitly with conformance-report.py; no expected CUE was changed.
- Legacy remains 520 passed/27 failed with the exact same failure names; this is
  separate from semantic conformance and exits nonzero as expected.

Final release measurements, three sequential runs per variant, saved upstream
JSON checked for literal/refs and normal rejection checked for original:

| Variant | Median RSS KiB | Max RSS KiB | Median seconds |
| --- | ---: | ---: | ---: |
| literal | 171824 | 171964 | 0.38 |
| refs | 301952 | 302188 | 0.69 |
| original | 1003900 | 1004056 | 2.51 |

Outputs/verdict and RSS are effectively unchanged from the previous family.
Original still exceeds the 512 MiB target. Logs/reports are
`tmp/dynamic-choice-{workspace,oracle,regressions,clippy,corpus-final,legacy,odoo}.*`.

## Remaining work

The reduced dynamic-choice issue is fixed; this is not a replacement of general
struct admission provenance or the evaluator scheduler. Exact decimals, pending
values, builtin contracts and structural/diagnostic observation coverage remain
open. Next memory work must attribute retained arena allocations and inventory
live execution roots before choosing safe reclamation points. The dirty enve
checkout and its dependency pins remain untouched; no current downstream build
would consume these uncommitted changes through its existing revision pin.
