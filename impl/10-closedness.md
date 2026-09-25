# Closed definitions, and provenance the caller asks for

`StructValue.is_closed` and the "not allowed" check in `unify_structs_inner` both
existed, but only the `close()` builtin set the flag. Definitions were never
closed, and `...` was dropped, so `#A & {x: 1, bogus: 2}` exported. Upstream
`cue` v0.16.1 reports `a.bogus: field not allowed`. Every schema built on
definitions (enve's `#Service`, enact's `#Pipeline`) let typos through.

Two things hid this:
- The txtar harness counted any expected error as a pass. 64 of the 547 corpus
  files cover "not allowed", and none of them was checked.
- The conformance tracker listed closed definitions as "100% complete".

A second defect was found in the same load path. `PackageLoader` added
`originDir: <package dir>` to every struct that had `command`, `image`,
`healthCheck`, `readinessProbe`, `lifecycle`, or `name` together with `port`.
- That put an absolute host path into enact's jobs, codegen and probes, and into
  files committed from those exports.
- It skipped every struct built from enve's own `#Service`, because the schema
  declares `originDir?: string`, and a declared field counted as present.
- It mutated shared arena nodes in place.

Only enve needs the value, for a service's working directory.

## What upstream does

Measured with `cue export` v0.16.1 over 41 probes on 2026-09-25. Every shape is
in `tests/closedness.rs`.

- A definition is closed where it is **read**, recursively: through fields,
  pattern-constraint targets, disjunction branches and list elements.
- `...` leaves a struct open. Its children are still closed.
- Hidden fields and definitions are exempt. An optional field on the other side
  of the unification is allowed.
- Embedding a closed value in a literal: the literal's own fields are allowed,
  and the result is closed. This holds whether the embedded value comes from a
  definition, a regular field, a hidden field or a selector.
- The file root is never closed, so `#A` embedded at the top level constrains
  nothing the file declares.

## What changed

1. **Harness.** `test-txtar --strict-errors` fails a case with `out/errors.txt`
   when evaluation succeeds. It checks for "not allowed" when every recorded
   error is a closedness error. It also exports every file of the archive, not
   just the last one.
2. **Closedness** (`closedness.rs`, `eval.rs`, `unify.rs`):
   - `ClosedCopies` memoises a deep-closed copy per definition value. It never
     mutates the shared id, and it re-validates a cached copy after an arena
     rollback.
   - A struct written with `...` (`StructValue.is_open`) stays open.
   - Embedding opens the top level (`open_for_embedding`, which follows a
     `RecursiveRef` target), unifies, then calls `reclose`.
   - The file root and definitions read while evaluating a root embedding stay
     open.
   - `disallowed_field` checks both directions and skips optional fields.
3. **Origin annotation** (`package.rs`): the key heuristic is gone.
   - `LoadOptions { origin: Some(OriginAnnotation { marker, field }) }` annotates
     only structs whose hidden fields include `marker`, with the directory of
     the package that first produced the value.
   - It is copy-on-write, and walks fields, lists and disjunctions.
   - An optional declaration of `field` does not count as set.
   - The option is off by default. enve passes `{_enveService, originDir}`, and
     marks `#Service` with `_enveService: true`.
4. **Import errors.** A package that resolves but fails to load now fails the
   load as `import "<path>": <error>`. It used to fail later as
   `reference <alias> not found`.

## Results

- 146 workspace tests (up from 133, 8 of them in `closedness.rs`). Clippy and
  `fmt --check` are clean.
- Corpus 527/547, 21 → 20 failures (`definitions_root6` fixed), none new.
- Strict harness: 48 failures against 65 for the evaluator before this phase
  (17 fixed, none new).
- 38 of the 41 probes match upstream, counting 3 shapes on verdict only.
- Cost unchanged: corpus 0.54 s / 202 MB against HEAD's 0.54 s / 202 MB.
- enve with this tree: 737 tests green. One fixture changed:
  - the `pristine_stdout` config was invalid CUE and now declares `name`;
  - the `locked_env` binding digest changed because its `#Service` services
    now carry `originDir`.
- enact usecase exports: `originDir` now appears only on services. Before, it
  also appeared on every job, codegen and probe.

## Stated divergences (follow_up.md)

- **Merged closed structs** allow the union of their fields.
  `#A & #B & {x: 1}` with `#B` not declaring `x` is accepted. Upstream closes
  per conjunct (`closeInfo`).
- **Errors inside an unused definition** are not reported. Upstream fails with
  `#B.y: field not allowed`.
- **A disjunction** reports `no matching disjunction branch: [...]` with the
  field named per branch. Upstream names the field once.
- **Root-embedding suppression** is an approximation: every definition read
  while evaluating a file-root embedding stays open, not only the embedded one.
- **Error paths** name the leaf (`bogus`), not the path (`a.bogus`).
