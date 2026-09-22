# Implementation plan

1. [String inequality bounds](01-string-not-equal.md): implement exact string
   inequality and verify evaluator, package export, and upstream compatibility.
   Status: complete; tests, upstream comparison, Clippy and independent review passed.

2. [Repeated-field unification](02-repeated-fields.md): recursively preserve
   repeated field constraints and reject conflicts; validate the enve integration.
   Status: implemented and validated; publication authorized, enve pin user-owned.

3. [Deferred field conjuncts](03-deferred-field-conjuncts.md): keep each field's
   conjuncts so a reader re-derives at the vertex its value is merged into,
   ending stale references to overridden defaults.
   Status: implemented; 527/547 txtar (five fixed, none new), enve's 74
   downstream tests green. Independent review found four regressions the corpus
   does not reach - see its Review outcome section - all fixed by phase 04, which
   this landed with, in 8e8e50d.

4. [Re-derive at the merge](04-rederive-at-merge.md): replace phase 03's
   after-the-fact refresh pass with re-derivation at the merges the evaluator
   initiates, deleting the level, staleness and pass-cap machinery the review's
   four defects live in.
   Status: implemented and validated. 83 workspace tests green, 527/547 txtar
   (five fixed, none new), enve's 74 downstream tests green, corpus at 1.07x
   HEAD. All four phase-03 review defects fixed, and the two blockers this
   phase's own review found fixed in stage 3 - deriving only the recipes that
   read what a merge moved, in dependency order. Four review gaps stay open in
   follow_up.md, none of them regressions. Landed as one change with phase 03
   in 8e8e50d.

5. [Re-derive across the import boundary](05-rederive-across-imports.md): load an
   imported package into the importer's arena so the recipes its fields carry
   survive the import, and move a file's import set onto the environment its
   literals capture.
   Status: implemented and validated. 91 workspace tests green, 527/547 txtar
   with HEAD's twenty failures unchanged, cost indistinguishable from HEAD
   (0.82-0.83 s / 1.60 GB against 0.82-0.85 s / 1.61 GB). `enve`'s 74 enve-cue
   tests green, and its posthog and distributed-monorepo examples gain 24 leaves
   that now match `cue export` v0.16.1 with none regressed. A fourth stage was
   added during the work: a struct swept its own recipes before descending into
   the merges below it, so a field reading a nested override saw the pre-descent
   value - a phase-04 defect the depth-3 import fixture uncovered. Committed
   as 9273b3b.

6. [String literal forms](06-string-literal-forms.md): carry which of CUE's four
   spellings a literal was written in through the lexer, the AST and the
   formatter, decode each under its own escape rules, and keep the comments and
   blank lines a format has to give back.
   Status: implemented and validated. 105 workspace tests green, up from 91;
   Clippy clean. Corpus at 526/547 against HEAD's 527: the one case that moved
   is `upstream_cue_testdata_interpolation_scalars.txtar`, where a bytes
   literal's `\(…)` is now a named refusal instead of the silent wrong value
   HEAD produced - the harness never compared that file's values, so its pass
   was vacuous. Bytes interpolation is the one stated gap, in follow_up.md.
   Written up from the diff rather than planned ahead; the worklog says so.

7. [Optional fields, self-reference and a cycle's verdict](07-optional-cycles-and-self-reference.md):
   stop exporting optional fields, let a field read the field that encloses it,
   and refuse a structural cycle instead of exporting a placeholder string.
   Three upstream divergences found reviewing the 2026-09-22 recursive-schema
   report, all on plain CUE.
   Five stages: a `BottomKind` on `BottomReason` first, so the three existing
   string matches and the new one move onto a type; then the export verdicts,
   the self-reference, and the messages upstream gives each kind of bottom.
   Status: implemented and validated. A sixth stage was added during the work:
   stages 3 and 4 fix self-reference one link deep, and the `needs` DAG a user
   writes is a chain, so a pass that refines a pending declaration's partial
   value now counts as progress - bounded against `MAX_UNRESOLVED_DEPTH`,
   because a partial deeper than that walk is judged resolved and written with a
   bottom inside it. 126 workspace tests green (up from 110), corpus unchanged
   at 526/547 with the same 21 failures by name, Clippy and `fmt --check` clean,
   cost 1.08x time and 1.09x memory. Every recorded shape agrees with `cue
   export` v0.16.1 on the value, and the error cases on the verdict and path.
   Two stated limits, both in follow_up.md: a chain longer than eight links, and
   a non-converging value reported as an unresolved reference rather than as a
   structural cycle.

8. [Disjunction branch dedup](08-disjunction-branch-dedup.md): keep a branch
   only if no branch already kept holds the same content, so unifying a
   disjunction with an equal copy of itself stops doubling its width.
   This is the 2026-09-22 `monorepo-go` abort: four files each unifying the same
   service disjunction take it to 2^23 branches and 3.1 GB before the allocator
   refuses. Status: implemented and validated. The package exports in 0.01 s at
   13 MB where it aborted at 3.1 GB; branch width peaks at 2 instead of 2^23.
   110 workspace tests green (up from 105), Clippy clean, corpus unchanged at
   526/547. Three of the five new tests fail if the deduplication is removed,
   at 7, 7 and 1024 branches.

9. [The shape a file was written in](09-formatter-shape.md): record which
   composites were written on one line, which paths were written without
   braces, and where a disjunction wrapped, and lay a block of fields out in
   the columns `text/tabwriter` puts them in — so a file `cue fmt` has already
   formatted is one `enve cue fmt` leaves alone.
   Status: implemented and validated. 39 of enve's 40 CUE files now come back
   byte-identical to `cue fmt` v0.16.1, and every one of the 40 is a fixed
   point of it; `enve cue fmt --check` names the same 12 files upstream itself
   rewrites, down from 39, which is what `just fmt` was waiting on. Measuring
   it turned up two defects that change what a file means, both fixed here: the
   formatter dropped parentheses, so `(a | b) & c` came back as `a | b & c`,
   and `[...]` came back as `[]` — the latter in the evaluator too, where
   `[...] & [1, 2]` was a length conflict. 132 workspace tests (up from 126),
   corpus unchanged at 526/547, enve green at 693. Four stated divergences in
   follow_up.md, every one of them a shape `cue fmt` accepts back unchanged.
