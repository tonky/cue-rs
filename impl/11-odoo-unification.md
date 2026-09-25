# Odoo unification, scope and bounded cycle diagnostics

Status: approved by the user, implemented and validated.

## Reproduction and evidence

The standalone snapshot is `/home/tonky/projects/cue-rs-odoo-repro`.
The checkout already contains uncommitted changes in `eval.rs`, `unify.rs`
and `value.rs` for shadowing and disjunction defaults. Preserve and finish
that work rather than discard it.

Executable investigations run in a systemd scope with `MemoryMax=2G` by
default, `MemorySwapMax=0`, bounded process count and a timeout. Builds use
one job, tests one thread. Initial allocator-heavy repros also used a 1.5 GiB
data limit. After the runaway was removed, the full original completed in a
3 GiB scope with a 2.75 GiB data limit. The reusable runner disables core
dumps. Bubblewrap is not the memory boundary.

Starting-checkout measurements on 2026-09-25, optimized build:

| Case | Result | Peak RSS |
| --- | --- | --- |
| minimal shadow | rejects, matching upstream verdict | 5 MB |
| minimal conflicting defaults | rejects, matching upstream verdict | 5 MB |
| literal | JSON equals upstream | 347 MB |
| refs | JSON equals upstream | 574 MB |
| original | allocation abort at imposed limit | 1,507 MB |

The first original run reached the aggregate scope limit and terminated;
the second aborted at the smaller allocation limit. Neither was unbounded.

The two-line reduction in `tmp/merged-cycle.cue` has one generated component,
a schema with `name?: string`, and `name: "\(name)"` plus an interpolated
title. It produces **1,024 derivations, two unsettled structs, and a 6,006-byte
export result**. Each interpolation of `BottomKind::Cycle` prefixes the error
again. Value comparison includes the error message, so each pass appears to
change the value and reaches the 256-sweep backstop. Arena allocations and
ever-growing diagnostics accumulate.

Two further holes are reproduced:

- A direct comprehension body (`for name, v in {a: 1} {name: "\(name)"}`)
  shares its frame with the loop variable and exports `name: "a"`. Upstream
  reports a cycle.
- `(*"a" | "b") & ("a" | *"b")` has no winning default, but JSON export
  falls back to the first branch and exports `"a"`. Upstream rejects it as
  incomplete.

## Implementation stages (completed)

1. **Make cycle propagation converge.** Preserve an existing bottom and its
   typed cause through interpolation rather than repeatedly decorating its
   message. Check evaluator context restoration on error as well as success.
   Pin the one-component failure with bounded derivation counts, no unsettled
   structs and a bounded diagnostic; cover a larger generated set too.
2. **Finish lexical shadowing.** Give comprehension bodies their own lexical
   frame and preserve the distinction between fields declared in that frame,
   loop bindings and enclosing fields. Exercise direct and nested bodies,
   forward references, explicit constraints and merged field recipes. Any
   broader scope-model change must be justified by a minimized failure.
3. **Finish disjunction defaults.** Retain the in-progress intersection of
   defaults. Export or interpolate only an unambiguous selection; an
   unresolved multi-branch disjunction must not silently choose its first
   branch. Cover equal/different defaults, defaults on one side, explicit
   overrides, operand order and ordinary unmarked ambiguity.
4. **Validate the supplied packages and document the result.** Add focused
   integration regressions that do not depend on the sibling repository.
   Run workspace tests, Clippy, formatting, and compare corpus failures by
   name against the starting checkout. Rerun all Odoo variants with caps;
   `original` must reject without runaway derivation, and `refs`/`literal`
   must retain upstream-equivalent JSON. Add a reusable capped `just` recipe.

## Scope and limits

Do not change the supplied Odoo configs to hide the invalid inputs. A general
arena collector or redesign of all retry passes is outside this fix. Record
remaining valid-input memory cost separately, with measurements. No commit or
push without the user's request.

## Implementation decisions

- Interpolation propagates the existing bottom unchanged. That preserves its
  kind and path and makes repeated derivation compare equal.
- Comprehension bodies use the same scope-entry helper as struct literals.
  Reserve a pending binding when a field shadows a known outer name, so
  nested readers wait for the right declaration. Names without an outer
  binding already wait on a missing lookup and need no extra allocation.
- A bare self-reference in a conjunction contributes no constraint
  (x: x & 1). Cycles inside arithmetic or interpolation remain errors; this
  does not introduce a general recursive expression solver.
- Intersect defaults when both disjunctions have them; use the marked side
  when only one has defaults. Export and interpolation reject ambiguity.
  Literal branches share the unifier's deduplication so equal alternatives
  still export.
- Restore field, scope and import evaluation context on error as well as
  success. No smaller sweep cap masks the convergence bug.
- Two old width tests exported unresolved alternatives while their comments
  described upstream rejecting them. They now assert rejection and export
  only the concrete fields they intend to check.

## Validation

Six new integration tests cover the minimal bugs, 1- and 100-component merged
cycles, nested forward shadowing, direct self-reference constraints, equal
alternatives, operand order and unambiguous defaults.

- 179 workspace tests pass.
- Workspace Clippy with warnings denied and formatting checks pass.
- Strict corpus: cue-rs passes 504 and fails 43 of the 547 local fixtures.
  There are 429 upstream-prefixed imports and 118 other fixtures; all failures
  are in the imported subset. Failure names match the starting checkout (which
  included the user's uncommitted fixes). This measures cue-rs conformance,
  not failures of the upstream CUE binary.
- Final optimized literal: 0.70 s / 348,692 KiB, JSON identical to upstream.
- Final optimized refs: 1.16 s / 576,956 KiB, JSON identical to upstream.
- Final optimized original: 4.12 s / 2,184,656 KiB, rejects at
  pipeline.components.account.name. Upstream also rejects that field;
  diagnostic wording differs. This is approximately 2.08 GiB and needs the
  3 GiB runner setting. The earlier full diagnostic run after fixing error
  growth recorded zero unsettled structs, versus the two unsettled structs
  from just one generated component before the correction.

The safe recipe preserves argument boundaries, disables core dumps, sets a
three-minute timeout, and runs builds/tests serially. Its default cgroup
was checked from inside the command: memory.max=2147483648,
memory.swap.max=0, core limits=(0, 0). CUE_SAFE_MEMORY=3G selects the bounded
limit used for the original package; the default remains 2 GiB.

Temporary logs and repro reductions stay under the ignored tmp directory.
The supplied sibling snapshot is unchanged.
