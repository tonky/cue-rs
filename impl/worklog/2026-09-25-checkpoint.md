# Documentation and commit checkpoint

The user requested documenting everything for future improvements and committing
the accumulated work. This checkpoint includes the conformance runner, semantic
families, evaluator boundaries and memory improvements since phase 11/d16afa0.
It does not mark phases 13–15 complete.

Added `impl/CONTINUATION.md` as the current entry point: verified outcomes,
prioritized compatibility work, a preliminary reclamation-root inventory,
ownership/cache invariants, API migration notes, downstream boundaries and safe
reproduction commands. Linked it from PLAN and follow_up. Preserved before/after
Odoo benchmark samples and the final all-slot profile under `impl/measurements/`
so future work does not depend on ignored temporary logs. Historical worklogs
retain their original context; the continuation guide gives current totals.

Updated stale current-count prose in follow_up and the oracle README, and recorded
the latest sharing boundaries in phase 15. No production implementation changed
during this documentation/commit step. Reused the just-completed capped test,
Clippy, formatting, corpus and benchmark evidence rather than rerunning heavy jobs
for prose changes. Checked whitespace, local documentation links, copied evidence
and the staged file inventory before committing. Removed trailing blank lines
from three newly added reduced fixtures; the 547 original archives remain
byte-identical.

Current limitations remain explicit: original Odoo is about 693 MiB against a
512 MiB target; 749 mismatches and 3383 unsupported corpus checks remain; string
sharing incidentally masks part of the identity-based `list.Contains` defect;
general root ownership, scheduling, decimals, diagnostics and downstream adoption
are still open. This commit does not publish or change downstream revision pins.
