# Odoo unification investigation

## Final result

The user explicitly approved phase 11. Finished the existing scope/default
changes and removed the repeated interpolation decoration. A bottom now
retains its typed cause and path across derivation. A shared scope helper
gives comprehension bodies the same lexical boundary and error restoration
as other literals. Pending reservations prevent nested readers from seeing
an outer binding while a nearer declaration is still unevaluated.

An initial reservation for every declared field increased memory use. Only
names shadowing an existing binding need a reservation; those share one
pending value per literal. That brought the valid Odoo cases back within
about 1% of the starting checkout's memory measurements.

Bare self-references can meet a concrete constraint without accepting cycles
inside other expressions. Disjunction export no longer picks the first
unmarked alternative; equal literal branches are deduplicated so that a
repeated value remains concrete. Two existing width tests depended on the
old permissive export and now check the incomplete verdict while retaining
their width and concrete-field checks.

Final validation:

- 179 workspace tests pass, including six new regressions.
- Clippy with warnings denied, formatting and diff whitespace checks pass.
- cue-rs passes 504 of 547 local fixtures under strict-errors,
  with exactly the same 43 failures by name as the starting checkout.
- literal: 0.70 s, 348,692 KiB; refs: 1.16 s, 576,956 KiB. Both JSON outputs
  are identical to the installed upstream implementation.
- original: rejects normally in 4.12 s at 2,184,656 KiB (about 2.08 GiB).
  The final run used a 3 GiB cgroup and 2.75 GiB data limit, with swap and
  core dumps disabled. It no longer grows its diagnostics to the sweep cap.
  The earlier full evaluator diagnostic recorded zero unsettled structs.

Added just safe for reusable process-tree limits, defaulting to 2 GiB and a
three-minute timeout. CUE_SAFE_MEMORY=3G accommodates the original snapshot's
remaining allocation cost. Verified effective memory/swap/core limits from
inside the runner, including preservation of quoted command arguments.
Recorded the remaining allocation cost in follow_up.md. The user subsequently
authorized committing phase 11 locally. The supplied sibling repro is unchanged.

The user questioned the 43 corpus failures. Auditing the results found 18
missing expected errors, 6 wrong error categories and 19 other failures.
The corpus has 429 upstream-prefixed imports and 118 other fixtures; all 43
failures are in the imported subset. These are cue-rs results. The harness does
not compare all expected values, and its directory runner exits successfully despite
failed fixtures. Recorded that gap in follow_up.md and corrected the stale
547/547 README badge and tracker counts without claiming complete conformance.

## Initial investigation

Read the supplied standalone repro and preserved the three pre-existing
modified evaluator files. Verified a systemd cgroup memory boundary before
running the evaluator: 2 GiB aggregate memory, no swap, timeouts and serialized
build/test work. The original package reached that cap; a subsequent run with
a 1.5 GiB data limit aborted its allocator without exhausting host memory.

Both supplied minimal bugs already reject with the partial fix. The valid
`literal` and `refs` variants match upstream JSON at 347 MB and 574 MB peak RSS.
The original still fails to complete under the allocation limit.

A one-component reduction confirms the runaway: cycle interpolation repeatedly
prefixes its error, changing the value at each sweep. It consumes 1,024 field
derivations, leaves two structs unsettled, and emits 6,006 bytes of result text.
Direct comprehension bodies still fail to shadow loop bindings, and a
disjunction with incompatible defaults still exports its first branch if the
remaining branches are concrete. Upstream rejects both reductions.

Phase 11 records the proposed correction and validation. No production code
was changed during investigation. Initial evaluator tests all passed; strict
corpus baseline for the starting checkout is 504 passed / 43 failed, recorded
in `tmp/oom-investigation-corpus.log`.
