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
`crates/cue-eval/tests/scalar_operators.rs`. Conformance: +23 fixes
(18 mismatch→passed, 5 unsupported→passed), fully-verified cases 28 → 29.

A `is_closed` refinement was tried and REVERTED: reporting `Conflict` for
missing fields on closed bases fixed `references_errors` but left
`fulleval_055_issue318` (nested selects) red — trading one pair for another.
Probes showed why it could never work: even a top-level `#T` definition body
ends `is_closed == false` when it contains errors, because error-containing
definitions never settle their placeholders and `close()` never runs on
them. `is_closed` does not mean "under a definition"; it means "passed
through `close()`", which happens inconsistently. Keying oracle codes off it
would keep surprising.

Remaining gap, one mechanism: selects of missing fields anywhere under a
definition report `incomplete` where upstream says `eval`
(`fulleval_055_issue318` nested selects, `references_errors` top-level
selects — 4 checks). Fixing it is the close-timing project: settle and close
error-containing definitions so nested lookups see closed bases. Verdicts
already reject; only the category is off.

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

## 8. Progress 2026-09-27 (cont.): `==` equality bounds (done, mostly)

89 checks in two fixtures failed to parse on unary `==` (the lexer already
had `EqualEqual`; only `parse_unary` lacked the arm). One parse failure
kills every check in a fixture file, and nothing in those files passed
before, so unlocking parse could only add passes. Changes:

- `crates/cue-syntax`: `UnaryOp::Equal`, parser arm, `Bound::Equal` with
  `Display`/`TryFrom<UnaryOp>`/`From` conversions, formatter emission,
  roundtrip-test coverage. Binary `==` stays a comparison, not a bound
  (pinned by test).
- `crates/cue-eval/src/unify.rs`: `Equal` in the Int/Float/String arms
  (absorb on equality, conflict otherwise); new `Bool` arm (`Equal` and
  `NotEqual` — `false & !=true` accepts, matching the pinned oracle; the
  old catch-all wrongly rejected it); new `List|Struct` arm comparing via
  the existing budgeted `equivalent()` (`=={a:1} & {a:1,b:2}` conflicts,
  `[1,2] & !=[3]` accepts). Target-kind demands (§4) apply automatically
  since they are op-agnostic.

Deliberately deferred: bare `==` with a never-concrete target (e.g.
`==_int` with `_int: int`) stays a lazy bound instead of `Incomplete` —
resolving it needs a final-pass sweep so forward references keep working;
those checks moved mismatch→unsupported, not to passed. Relatedly
unblocked but not done: `binary.cue` "incompatible number bounds" range
analysis (`<1 & >2`), now visible past the parse failure.

Tests: `equality_bounds_meet_values` in
`crates/cue-eval/tests/scalar_operators.rs` (6 accepts + 6 rejects).
Conformance: +8 error_code and +11 value fixes from this slice alone.

Session tally: 54 checks improved (33 error_code mismatch→passed,
6 unsupported→passed, 11 value→passed, 4 value→unsupported), 2 known
regressions (issue318 nested-close, §5).

## 9. Progress 2026-09-27 (cont.): `error()` builtin (done, mostly)

`error("msg")` did not exist: the 5 `builtins_error` checks observed null.
Upstream semantics: the branch loses to succeeding siblings, lends its
message to a lone surviving failure while adopting that failure's code
(`incomplete` for `x+1 | error(m)`), and is definite alone. Changes:

- `crates/cue-eval/src/value.rs`: new `BottomKind::Custom` (never retried).
- `crates/cue-eval/src/builtins.rs`: `error` arm — a concrete (already
  interpolated) message becomes `Custom`; a bottom message falls back to
  its diagnosis; anything else is a `Conflict`.
- `crates/cue-eval/src/unify.rs` `settle_disjunction`: the 0-branch arm
  prefers the custom message (when nothing can still resolve); the 1-branch
  arm lets a surviving *failure* adopt it while keeping its code, and a
  surviving value drops it.
- `crates/cue-eval/src/eval.rs`: a `Custom`-containing disjunction settles
  definite branches eagerly at derivation (other disjunctions untouched);
  needed because plain disjunctions never revisit settling otherwise.
- `crates/cue-cli/src/conformance.rs`: lone `Custom` observes as `eval`.

Tests: `crates/cue-eval/tests/error_builtin.rs` (drop, eval-adopt with
kind assertion, incomplete-adopt with kind assertion, lone). Conformance:
+3 fixes. Remaining in the file: `substituteFail` (needs final-pass
collapse of multi-pending-branch disjunctions), `errorSelf` (needs `self`),
`indirect.y` (needs a `User` code for calling the `error` value;
currently unsupported, not mismatched).

## 10. Progress 2026-09-27 (cont.): empty comprehension bodies (done)

`[for y in src {}]` failed to parse (`parse_list_comprehension_body`
required an inner expression when the body was not label-led). Seven-line
fix in `crates/cue-syntax/src/parser.rs`; unlocked the whole
`checkdefined` fixture: +16 value fixes. Test:
`crates/cue-syntax/tests/comprehension_bodies.rs`.

Session tally: 73 checks improved, 4 known regressions sharing one
mechanism (missing-field selects under definitions report `incomplete`
instead of `eval`; the close-timing project in §5).

## 11. Progress 2026-09-27 (cont.): range analysis + nested patterns (done)

`incompatible_*_bounds` fired only without a base type, and only for
numbers/strings: the integer section (pattern-applied `int` base) and the
byte section stayed red. Changes:

- `crates/cue-eval/src/unify.rs` `incompatible_range`: one range tracker
  for all three groups (numbers as exact rationals, strings/bytes as byte
  vectors), strongest-lower/strongest-upper pair, bound text remembered at
  insertion so the message renders the written forms (`<1 and >2`, `<'a'`
  with single quotes for bytes). Integer base snaps bounds to
  ceil/floor before comparing (upstream: `>1 & <2 & int` conflicts);
  float base keeps rationals. Group message names `bytes` when any endpoint
  is a bytes literal, else `string`/`number`/`integer` by base.
- `crates/cue-eval/src/eval.rs` `eval_bound`: bytes targets valid for order
  bounds (were `Incomplete`).
- `crates/cue-eval/src/unify.rs` `field_matches_pattern`: `[_]` evaluates
  to bare `Value::Top`, which the declaration-site check (plain unification
  against the field name) accepts but the merge-site check rejected — so a
  pattern target carrying its own pattern (`[_]: [_]: int`) never applied
  through a struct meet. One-arm fix (`Top => true`); the two predicates
  now agree.
- `crates/cue-eval/src/unify.rs` `unify_structs_inner` pattern loop,
  first-failure-wins: a field that already failed keeps its own error
  (skip re-application), and a pattern-induced failure stays on the field
  (`break`, no struct collapse). Collapsing hid the error path from the
  observer (`assertion path missing`) and regressed 8 integer checks
  before the rule landed.

Tests: `incompatible_byte_ranges_conflict` (byte err shapes,
`crates/cue-eval/tests/string_not_equal.rs`),
`a_nested_pattern_target_applies_through_a_merge`
(`crates/cue-eval/tests/rederived_at_merge.rs`). Conformance: eval_bounds
has zero mismatches (100 passed; rest are err/contains, err/pos, eq-abstract
adapters). Full-suite diff vs baseline: 159 mismatch→passed, 10
unsupported→passed, 5 mismatch→unsupported (all wrong-answer→abstain: `==`
now parses in references_value/issue494 so `self`-alias values observe
abstract instead of parse-error, plus one optional_expanded value; the
pending `self` feature owns them), 4 passed→mismatch (the known
close-timing items from §5, unchanged). Gates green: workspace tests,
clippy 0, fmt clean.

The `==` non-concrete sweep (§8 follow-up) is done by the same work: all
`nonConcrete` error_code checks pass. Remaining: close-timing project
(§5: issue318, references_errors, substituteFail final-collapse),
`self` + `User` code (§9: errorSelf, indirect.y), message-order cosmetics
(upper-first vs written order; err/contains is adapter-unsupported).

## 12. Progress 2026-09-27 (cont.): close-timing + final-collapse (done)

Missing-field selects through definitions reported `incomplete` instead of
`eval` (issue318 x2, references_errors x2 — the 4 known regressions), and a
custom disjunction with only pending branches waited forever
(substituteFail). Changes:

- `crates/cue-eval/src/value.rs`: new `BottomKind::UndefinedFieldDefinite`
  (base decided: closed copy or select under a definition). Retryable like
  `UndefinedField` — a merge may still supply the field — but survivors
  observe as `eval`.
- `crates/cue-eval/src/eval.rs`: new `Evaluator::in_definition` flag,
  threaded into `select`/`index`.
- `crates/cue-eval/src/operators.rs`: `select`/`index` emit
  `UndefinedFieldDefinite` when the base struct `is_closed` (read through
  a definition closes it) or the select runs under a definition; the
  open-world miss is unchanged. Unit test pins both arms.
- `crates/cue-eval/src/relaxation.rs`: the flag is set around a
  definition's body evaluation (nested literals inherit; save/restore).
- `crates/cue-cli/src/conformance.rs`: the new kind observes as `eval`.
- `crates/cue-eval/src/eval.rs` disjunction arm: with a custom branch
  present and every kept branch already a bottom, fold to one survivor
  (most-decided first: Definite, Incomplete, UndefinedField, other) so the
  existing 1-branch adopt reports instead of leaving an unsettled
  disjunction. The survivor stays retryable, so a later succeeding branch
  still heals; values beside customs are untouched (`1 | error(m)` still
  drops the error). The 0-arm mapping is unchanged (unify callers rely on
  pending → `Unresolved`).

Tests: `crates/cue-eval/tests/definition_selects.rs` (4: regular stays
incomplete, through/nested/under definition definite),
`all_pending_branches_beside_custom_collapse_to_incomplete`
(`error_builtin.rs`), unit arms in `operators.rs`. One stale expectation
updated with reason: `missing_field_kind_depends_on_base_shape`
(`scalar_operators.rs`) pinned `#a.b` as `UndefinedField`; upstream
`references_errors` says `eval`. Gates green. Full-suite diff vs
baseline: 159 mismatch→passed (incl. substituteFail), 10
unsupported→passed, 5 mismatch→unsupported (unchanged),
**zero regressions**.

## 13. Parked: `self` + `User` code (errorSelf, indirect.y)

Verdict 2026-09-27: neither enact nor enve uses them — 0 hits for the
`self` keyword (only `"self-hosted"` inside string literals), 0 hits for
`error(`, no `@experiment` anywhere, no `self:`/`error:` field names
across 1204 `.cue` files; enve embeds cue-rs via `enve-cue` but exercises
neither feature. Parked until consumer or usecase demand; the analysis
below is the restart point.

Analysis 2026-09-27, not yet implemented:

- `self` does not exist in cue-rs (`reference "self" not found`); upstream
  gates it behind `@experiment(aliasv2)` (without the experiment the
  oracle itself errors `predeclared identifier "self" requires ...`).
  `let X = self` shapes (references_value structShorthand/listValueAlias,
  issue494 — today's 5 wrong-answer→abstain) need it as a lazy alias to
  the enclosing struct, not an eager value (the struct is still being
  built; cf. the sibling-partial machinery in `self_reference.rs`).
- errorSelf (`v: "x"|"y"|error("bad: \(self)")` + `v: "z"`) drops the
  custom branch at construction (kept holds 2 values, customs go to
  errors, 2-branch settle ignores them), then reports the conflict
  summary. Upstream keeps the custom (`bad: {v: ...}`). Fix direction:
  keep a custom branch beside 2+ surviving values so a later meet can
  still surface it (single surviving value still drops it — the
  `error_branch_loses_to_a_succeeding_sibling` invariant); plus `self`
  must resolve (else the message interpolates `not found`).
- The errorSelf check path is `['errorSelf']` (the parent, not the leaf):
  the struct stands with a bottom field, so `error_code` observes null.
  Needs parent aggregation in `conformance.rs observe` (a struct
  containing an error IS erroneous upstream): only triggers on non-bottom
  values, so no passing check can flip; mixed subtrees need a severity
  order (eval > incomplete > cycle).
- indirect.y (`x: error`, `y: x("msg")` → oracle code `user`): builtins
  are not bindable values (`reference "error" not found`), and there is
  no `User` error code. Oracle taxonomy from `out/errors.txt`: direct
  `error("...")` naming a failed disjunction → `eval` (adopts the
  disjunction's code); the error value CALLED (`x("msg")`) → `user`.
  Needs bindable builtins, call-through-reference, and the code split.
  All `contains`/`pos`/`suberr` adapters stay unsupported, so only codes
  score.

## 12. Proposed order (usecase-first)

1. Error-code translation layer (`BottomReason`/`BottomKind` → categories +
   positions): fixes usecase diagnostics AND the largest mismatch bucket.
2. `original/` rejection cost (710 MB vs 240 MB): early cycle detection in
   scheduling, before sweeping 662 components through a doomed comprehension.
3. Dense fixtures one at a time (`comprehensions_try`, `eval_bounds`), only
   where the usecase needs them.
4. Breadth (3,383 unsupported): only on usecase demand.

## 14. Progress 2026-09-27 (cont.): parser slice, keyword idents + `!` labels + listcomp `let` (done)

Oracle-verified: every keyword (`in let for if import package`, plus
`div/quo/rem/mod` which lex as plain idents) doubles as a field label AND a
value identifier. Four gaps in `crates/cue-syntax/src/parser.rs`, one shared
helper (`keyword_ident_name`):

- `parse_primary`: keywords in operand position → `Expr::Ident`
  (`{_in: in}`, `[in, let, for, ...]`; fixes ~90 `'in'` + chained.cue's
  `in.b.actual`).
- `parse_postfix` selector: `a.in` selects a keyword-named field.
- `parse_for_vars` (key + value): `for import in _imports` (all 12
  `'import'` errors were this one shape in nested2/issue4094).
- `is_label_ahead` (`(`/`[` branches): skip `!` like `?` — the simple-label
  branch already did. `(t1)?: (t2)!: 3` (all 7 `':'` errors, one
  dynamic_field source).
- `parse_list_comprehension_body` `{...}` branch: `KwLet` peek opens struct
  decls — `is_label_ahead` needs a colon, `let x =` has none. All 11
  `'let'` errors were `{let elems = ...}` after an `if` clause (issue3672).

Conformance (`/tmp/full5.json` → `/tmp/full6.json`): 47 mismatch→passed,
47 mismatch→unsupported (now parse, hit honest eval gaps), zero
regressions. Parse-error mismatches 221 → 101.

Parked with consumer evidence (0 enact/enve hits): `?` ×94 (`try`
experiment), bytes-interpolation ×7 (`'\(b1)'`). New test
`crates/cue-syntax/tests/keyword_idents.rs` (4 tests). Gates: workspace
tests ok (46 suites), clippy clean, fmt clean.

Known follow-up (eval, not parser): `FieldDecl.optional: bool` folds `!`
(required) into optional — `bar!` parses but evaluates as optional.

## 15. Progress 2026-09-27 (cont.): observed-null buckets, closedness half (done)

Two fixes, 20 mismatch→passed total (`/tmp/full6.json` → `/tmp/full9.json`),
zero passed→mismatch.

**Spread permission split from the open marker** (`value.rs`,
`unify.rs`, `closedness.rs`, `builtins.rs`, `relaxation.rs`,
`crates/cue-eval/tests/closedness.rs`). One flag carried two meanings —
"has a `...` marker, don't auto-close" and "a spread reopened this, allow
new fields" — and the `&` merge OR-ed it into results, so `#x & {...}`
wrongly accepted new fields downstream (issue3778 family: 13 closedness
nulls). Oracle probes pinned the model:

- `is_open` is now conjunctive over `&` (marker memory only); new
  `StructValue::spread_open` is disjunctive and is the only merge
  permission `disallowed_field` honours.
- `close_deep` (definition read) revokes `spread_open` on closed structs:
  `#S: #Def... & {b: 2}` rejects a later `& {c}`, while bare `#S: #Def...`
  reopens (formula unchanged) and stays mixable — both oracle-verified.
- `close()` maps marker-openness into the spread flag (`close({...}) &
  {a: 5}` accepts `a` while staying closed — upstream issue3572, which the
  first revision broke and the 4 builtins_closed flips caught).
- The embed-violation check also honours `spread_open` (7 explicitopen
  spread-embed flips caught that).

New test `only_a_spread_reopens_a_closed_struct` covers all three
oracle probes.

**Parent error aggregation** (`crates/cue-cli/src/conformance.rs`): a
struct standing over an error leaf is erroneous upstream, but only the
leaf is bottom — `error_code`/`error_present` now report the severest
code in the value subtree (eval > incomplete > cycle >
structural_cycle; disjunction interiors skipped, arena cycles cut). This
is the §13 mechanism; it fixed issue3778/1867/3924/852, `errorSelf`,
`cycleErr`, `shareCycle` (19 checks) and cannot flip a pass (fires only
on non-bottom values).

Remaining observed-null: eval 21, incomplete 39, cycle 1 — scattered
(pushdown 7, required 7, rest ≤3 per fixture), each needing its own
eval root cause; next slice.

## 16. Progress 2026-09-27 (cont.): perf vs 2 GiB cap (done, holds)

Release binary, 3-run medians via `tools/benchmark-odoo.py`:

| variant | cue-rs | upstream Go | verdict |
|---|---|---|---|
| `literal/` | 0.55 s, 160 MB | 1.66 s, 223 MB | faster, byte-identical |
| `refs/` | 0.84 s, 208 MB | 1.73 s, 260 MB | faster, byte-identical |
| `original/` | 2.69 s, 821 MB, correctly rejects (cycle) | 1.56 s, 248 MB, rejects | 3.3x memory on the reject path |

Usecase verdict: valid configs byte-identical, invalid rejected with an
actionable error, peak 821 MB < 2 GiB cap — all three hold, so no
optimization is owed today. (The closedness rework regressed `refs/`
mid-slice — `#ServiceSpec & {port}` lost its openness and rejected
`database`; model B in §15 fixed it. The repro is the backstop that
caught it.) The `original/` reject-path gap (early cycle detection
before sweeping 662 components) stays future work per §12.2.

## 17. Progress 2026-09-27 (cont.): reject-path profiling + allocation wins (done, partially)

`/usr/bin/time` on the release binary (`cue` oracle for comparison):

| variant | cue-rs | upstream Go | verdict |
|---|---|---|---|
| `literal/` | 0.38 s, 141 MB | 0.51 s, 144 MB | faster, same memory, semantic-equal output |
| `refs/` | 0.59 s, 189 MB | 0.79 s, 194 MB | faster, same memory, semantic-equal output |
| `original/` | ~2.2 s, 788 MB, correctly rejects (cycle) | 1.56 s, 248 MB, rejects | correct verdict, 1.4x time, 3.2x memory |

Valid-path outputs are semantic-equal to `cue export` (key order differs:
export walks `BTreeMap`; pre-existing, untouched). `literal/` and `refs/`
outputs are mutually md5-identical. Conformance rerun: migration baseline
matched (zero drift). Suite: 46 suites green, clippy `-D warnings` clean,
fmt clean.

Banked (all in `cue-eval`):
- `collect_field_declarations` returns `Cow<[Decl]>`: borrowed when no
  name repeats (nearly every literal), owned fold only on repeats; fold
  keys borrow labels instead of allocating `String`s. Valid-path time
  -25%, reject path -0.4 s. Test: `unrepeated_fields_borrow_without_cloning`.
- Thread-local compiled-regex cache (`cached_regex`, cap 4096) for the
  four `Regex::new`-per-call sites (unify bounds, `field_matches_pattern`,
  `time.Time`). Behavior-neutral; regex-heavy files stop recompiling.
- Pre-existing stack kept: precise `seed_moved` (refinements 4648 → 14),
  cycle-bottom earns no refinement credit, `speculate`/`commit_speculation`
  (trail cleared at outermost close), disjunction sites converted.

Tried and reverted (measured nil or negative on `original/`):
- Ordered-pair unify memo in the arena (generation-guarded): pairs never
  repeat — every re-derivation rebuilds the world with fresh ids.
- Tail Equal-guard on pattern writeback: pure overhead (deep compare per
  application), no id stability to exploit (see above).
- Scope-map freelist: most maps end up captured by recipes (retained, not
  churn).
- Narrow tail seeds (seed = tail-touched only): narrow tails derive zero
  recipes, but wall time flat — the cost is nested-merge settling below
  touched fields, which needs path-granular (not name-granular) deps.
- Settle-check hoist above the rederive clone: seeds are broad, rarely empty.

Profiling notes: samply blocked (`perf_event_paranoid=2`, no sudo);
`pprof` signal sampling yields zero samples in this sandbox; DHAT heap
(`dhat` feature, since reverted) + phase-timer probes carried the analysis.
Phase split of the 2.2 s wall: declaration loop + re-derivation ~all;
pattern application 0.01 s (noise); recipe evaluation dominates merges
60:1; 330k literal evals, 202k struct merges (~32% reproduce an input
exactly — first-sight no-ops no memo can catch), 48k re-derivations
(~4 writes).

Structural remainder (future project, not patches): the arena never frees,
so every per-pass struct node and every discarded Equal-merge orphan is
retained (~540 MB over upstream — needs GC or compacting arena); the
fixpoint re-evaluates recipes from scratch with fresh ids (needs id-stable
fixpoint or hash-consing for memoization to bite); sub-field-insensitive
readers re-derive on whole-name moves (needs path-granular deps).
