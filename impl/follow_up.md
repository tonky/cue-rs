# Follow-up

## Deferred generated-field evaluation

Open: replace eager comprehension/embedding evaluation with dependency-aware
reevaluation. A generated constraint can otherwise leave an earlier reference
holding a partial service value. The repeated-field fix rejects updates to
already referenced fields with an explicit unsupported-evaluation error.
The guard is conservative, including redundant updates; full support should
preserve CUE lexical scopes and handle cycles without weakening constraints.

## Downstream release

User-owned: update both enve workspace dependency revisions and Cargo.lock after
cue-rs publication, then rerun its policy regressions against the actual git pin.
The user explicitly requested handling this downstream step themselves. Current
local-path validation does not complete this release step.
