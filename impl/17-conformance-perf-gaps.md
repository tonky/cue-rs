# Conformance + Perf Gaps For The Odoo Usecase

Measured 2026-09-27 on master `4e298d4`. Method: `cargo run -p cue-cli --
conformance tests/testdata --manifest tests/conformance/manifest.json
--baseline tests/conformance/baseline.json` (547 cases, pinned Go oracle
`tmp/conformance-oracle` rev `635e4bb4`), plus release-binary timing of the
enact odoo repro at `/home/tonky/projects/cue-rs-odoo-repro` (662 components,
115 KB CUE) with `/usr/bin/time`. Upstream reference: `cue` v0.16.1.

Our usecase: evaluating enact's odoo pipeline configs — must accept valid
configs with byte-identical output, reject invalid ones with actionable
errors, and stay inside a 2 GiB cap (`just safe`).

## 1. Conformance state

`Migration baseline matched` — no regressions from recent work.

| checks passed | mismatched | unsupported | non-portable | oracle disagreement |
|---|---|---|---|---|
| 749 | 749 (173 archives) | 3,383 | 603 | 1 |

28/547 cases fully verified. Mismatches by operation: `error_code` 321,
`value` 303, `export` 87, `closed` 22, `error_paths` 15, `error_present` 1.

Densest fixtures (one semantic gap fails the whole file):

| fixture | mismatches | likely gap |
|---|---|---|
| `definitions_explicitopen` | 100 | open/closed definition semantics |
| `eval_bounds` | 100 | bound validation / error codes |
| `comprehensions_try` | 94 | `try`-comprehension (probably unimplemented) |
| `eval_v0`, `references_self` | 27, 23 | self-reference / cycle classification |

`error_code` samples cluster in two buckets: "expected X, observed null"
(missing error detection) and wrong-category errors (`incomplete` vs `eval`
vs `cycle`) — see `basicrewrite_012_selecting` (`incomplete` vs `eval`) and
`builtins_and` (`incomplete` vs `cycle`). This points at the
`BottomReason`/`BottomKind` → oracle-code translation layer, not 321
independent bugs. `value` samples: f64 display precision (`6666666666666666e-16`
vs `...667e-34`) and unimplemented features leaking as values
(`text/template` leaving `{{.s}}` raw).

## 2. Perf state (release binary)

| variant | Sep-25 cue-rs | now | upstream Go |
|---|---|---|---|
| `literal/` | 0.7 s, 350 MB | 0.35 s, 89 MB | 0.9 s, 225 MB |
| `refs/` | 1.1 s, 575 MB | 0.63 s, 139 MB | 0.9 s, 270 MB |
| `original/` | 4.2 s, 2.2 GB, wrongly exports | 2.3 s, 710 MB, correctly rejects (cycle) | 0.9 s, 240 MB, rejects |

`refs/` output is byte-identical to the recorded Sep-25 output: the speedup
cost no correctness. `refs/` still uses ~1.5x upstream memory per the Sep-25
notes (arena not reused across passes, struct cloning on unification,
whole-file retry passes — C0 perf notes).

## 3. Usecase bug status (repro `bugs/`)

- `shadow.cue` (comprehension field shadows `for` variable): FIXED since
  Sep-25. Now rejects: `cycle with field: name`. Upstream:
  `out.a.name: invalid interpolation: cycle with field: name` + position.
  Verdict matches; our message lacks the source position.
- `defaults.cue` (two conflicting defaults): verdict FIXED (export now fails
  instead of picking one). Upstream: `x: incomplete value string | "b" | "a"`.
  Ours: `cannot export non-concrete disjunction at 'x' to JSON`. Same verdict,
  different message and no branch listing.

So for the usecase, accept/reject verdicts are now right; the remaining gap
is diagnostic quality (positions, branch listings, error categories) — which
is exactly the `error_code` bucket above. Usecase work and conformance work
converge there.

## 4. Progress 2026-09-27: bound targets constrain base kind (done)

Probed the oracle: every bound target constrains the value's type,
including `!=` (`"foo" & !=5` and `3 & !="a"` both reject upstream).
Two gaps in `unify_bounds` (`crates/cue-eval/src/unify.rs`), both fixed:

- Bounds-vs-Bounds merge kept constraints with incompatible implied
  bases (`!="a" & <5` stayed a constraint). Now conflicts eagerly via
  `bound_kinds_conflict` (number vs string), against both the merged base
  and previously seen demands. Non-concrete targets (null, references,
  disjunctions) still demand nothing.
- The String arm silently skipped non-string targets (`"foo" & >5`
  evaluated to `"foo"`). Concrete int/float/bool targets now conflict
  with `type mismatch: expected string, found {int,float,bool}`.

Deliberately NOT changed: `false & !=true` stays rejected (matches
upstream's verdict via the catch-all arm — adding a Bool arm would
diverge), `!=null` handling untouched (`!=null & <5` still accepts 3,
as upstream does).

Tests: new `bound_targets_constrain_base_kind` in
`crates/cue-eval/tests/string_not_equal.rs` (4 rejections + 3 positives).
`tests/conformance/baseline.json` left untouched throughout — still records
the pre-fix statuses.

## 5. Progress 2026-09-27 (cont.): undefined-field categories (done)

Upstream reports a missing struct field as `incomplete` (open struct, may
gain the field later) but as `eval` on lists and under definitions (closed,
definite). Three changes:

- `crates/cue-cli/src/conformance.rs`: `UndefinedField` now observes as
  `incomplete` (was `eval`). This matches the kind's own retry semantics in
  `may_resolve_later`.
- `crates/cue-eval/src/eval.rs`: missing field on a loaded (complete)
  package is now `Conflict`, not `UndefinedField`.
- `crates/cue-eval/src/operators.rs`: `select` off a list base and
  struct indexed by int (`{a:1}[4]`) are now `Conflict`; missing field on a
  closed struct base is `Conflict`, on an open one stays `UndefinedField`
  (same split in `index`).

Tests: `missing_field_kind_depends_on_base_shape` in
`crates/cue-eval/tests/scalar_operators.rs` (2+3 kinds, incl. closed
definition). Conformance: +23 fixes (18 mismatch→passed, 5
unsupported→passed), fully-verified cases 28 → 29.

Two regressions, same fixture, mechanism understood: `fulleval_055_issue318`
selects a missing field of a struct *nested* inside a definition. Nested
structs are built open and only closed by a later `close_deep` pass, so the
select still sees `is_closed == false` (top-level definition bodies are
closed in time — `references_errors` was fixed by this). Evidence points at
close being skipped for definitions that contain errors, or running after
the final derivation. That is closedness-layer work, not error-code mapping:
recorded, not pursued here. Verdicts still reject; only the category is off.

## 6. Progress 2026-09-27 (cont.): concrete integer labels (done)

`{(2): string}` produced `Unresolved` ("unresolved reference or ...");
upstream reports `eval` ("integer fields not supported"). Two changes:

- `crates/cue-eval/src/relaxation.rs`: a concrete-`Int` dynamic label
  returns `EvalError::Evaluation` (give-up) instead of `Unresolved` (retry).
- `crates/cue-eval/src/eval.rs`: the nested-struct boundary now maps a
  definite `Evaluation` error to a `Conflict` bottom at its own struct
  instead of aborting the enclosing file — siblings still evaluate. Only
  other nested producer is the recursion-depth limit, which is also
  definite, so the mapping is safe.

Tests: `concrete_integer_labels_are_definite_errors` in
`crates/cue-eval/tests/repeated_fields.rs` (+ string-label control).
Conformance: +2 fixes (`eval_fields`, `comprehensions_errors` intField).

Session tally for the error-code slice: 25 fixes, 2 known regressions
(issue318 nested-close, §5). Remaining families for later slices: ~150
parse-failure checks (parser scope: `==`, `in`, `?`, `import` forms),
~85 genuine observed-null (missing error detection, needs per-family
analysis), arithmetic-circularity classification (`E: {a: c-b, ...}` wants
`incomplete`, non-structural direct cycles want `eval` — semantic work),
nested closedness propagation (§5).

## 7. Progress 2026-09-27 (cont.): strict slice bounds (done)

`resolve_018_slice` expects `eval` for negative, out-of-range, and mistyped
bounds; `slice()` in `crates/cue-eval/src/operators.rs` clamped them
(`to_usize().unwrap_or(default).min(len)`, non-int fell back to the end).
Now an omitted bound still defaults, but a present bound must be an integer
in `[0, len]` — low reported before high, messages mirroring upstream
("index {n} out of range", "cannot convert negative number to uint64",
"cannot use {s:?} (type string) as type int in slice index",
"invalid slice index: {start} > {end}"). All slice bottoms are `Conflict`,
not `Other` (the `arena.bottom()` constructor makes `Other`, which observes
as unsupported — first run fixed the values but scored unsupported).

Tests: extended the unit test to `slice_rejects_bad_bounds_and_reversed_ranges`
(kept valid-slice + bound-at-`len` positives). Conformance: +6 fixes (all of
e1/e2/e4/e6/e7 plus e3/e5 moving from unsupported).

Session tally: 31 fixes, 2 known regressions (issue318 nested-close, §5).

## 8. Proposed order (usecase-first)

1. Error-code translation layer (`BottomReason`/`BottomKind` → categories +
   positions): fixes usecase diagnostics AND the largest mismatch bucket.
2. `original/` rejection cost (710 MB vs 240 MB): early cycle detection in
   scheduling, before sweeping 662 components through a doomed comprehension.
3. Dense fixtures one at a time (`comprehensions_try`, `eval_bounds`), only
   where the usecase needs them.
4. Breadth (3,383 unsupported): only on usecase demand.
