# Conformance, memory and architecture investigation

## Scope and authorization

After committing phase 11 as d16afa0, the user requested upstream
compatibility fixes, lower memory use and an architectural/Rust refactor.
They explicitly permit beneficial API breaks; enve and enact are the users.
Prepared phases 12-15 for roadmap approval. No production code was changed.
Downstream revision pins and publication remain user-owned.

## Conformance evidence

The 547 archives comprise 429 upstream imports and 118 other fixtures.
The legacy strict harness has 43 failure signals, all in the imported subset.
It ignores inline field assertions, does not systematically compare values,
can accept an unrelated error and exits successfully for directory failures.
An abstract definition's expected incompleteness does not necessarily mean
the whole file's concrete export should fail.

Inspected cached upstream v0.16.1 runner sources and archive metadata.
Compared imported paths/content against that version: 14 identical archives,
384 different and 31 without a matching path. Import commits do not record
an upstream revision. This does not establish the original revision or prove
semantic differences. Phase 12 must recover/pin provenance and operations
before treating an oracle result as a compatibility obligation.

The proposed runner records assertion coverage and separates semantic
mismatch, unsupported checks, version mismatch, crashes and resource limits.
A regression baseline is useful during migration but cannot count as full
conformance. Stronger checks may uncover defects among today's passes.

## Memory evidence

Built a temporary counting-allocator probe in tmp/odoo-probe and ran it
sequentially with just safe, using a 3 GiB process-tree cap for the known
original baseline. Logs: tmp/phase12-profile-refs.log and
tmp/phase12-profile-original.log. The probe and logs are ignored diagnostics.

Refs retains 560,017,858 requested heap bytes after loading, from
2,731,395,553 allocated bytes across 33,320,418 allocation/reallocation calls.
Original retains 1,920,130,355 bytes, from 7,346,730,147 allocated bytes across
94,103,146 calls. Refs exports 981,976 bytes of compact JSON. Both runs return
to approximately 600 bytes after evaluator/output drop.

Traversing semantic values plus recipe captures reaches 101,113 values and
11,218 environments for refs, and 412,813 values and 41,833 environments for
original. Captures retain 820,873 and 1,941,538 bindings respectively.
These counts exclude private evaluator roots and unreachable arena entries;
they are not a complete live/dead inventory. Requested allocation bytes
exclude allocator overhead and are not RSS. Probe timing is not a benchmark.

No retained-after-drop leak was demonstrated. Allocation attribution is
still needed before selecting reclamation or interning strategies. The
proposed targets are 300 MiB refs, 200 MiB literal and 512 MiB original peak
RSS, with correct results and no more than 20% median time regression.
These are acceptance proposals, not achieved improvements.

## Architecture and integration

eval.rs and unify.rs combine multiple concerns; recipes clone AST subtrees
and capture scope maps. Public values reach mutable execution state, while
ValueId does not carry arena identity. Pending work and semantic errors
share Bottom. Plan domain boundaries around compiled source, lexical binding,
evaluation session, owned values, unification and observation. Introduce
them alongside semantic and memory fixes rather than performing a file-only
split at the end.

Enve directly consumes PackageLoader, loading/origin options, EvalError,
eval_to_json and validate_json. Enact currently invokes enve cue export;
its integration gate is CLI JSON and exit behavior. Include workspace CLI,
derive and wasm consumers in API migration. No sibling repository changed.

Corrected a stale follow-up: validate_json already uses unify_and_rederive.
Its remaining nested/incomplete validation semantics still need auditing.
SOURCE_MAP.md also has stale ownership and conformance claims; phase 15
will rewrite it against the final implementation.

## Validation and next step

This change is planning/documentation only. Checked diff whitespace and
reviewed the phase order and acceptance criteria; workspace tests were not
rerun for documentation. Request roadmap approval under the parent AGENTS.md
before starting nontrivial implementation.
