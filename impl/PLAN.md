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
   this lands with. Not committed.

4. [Re-derive at the merge](04-rederive-at-merge.md): replace phase 03's
   after-the-fact refresh pass with re-derivation at the merges the evaluator
   initiates, deleting the level, staleness and pass-cap machinery the review's
   four defects live in.
   Status: implemented and validated. 83 workspace tests green, 527/547 txtar
   (five fixed, none new), enve's 74 downstream tests green, corpus at 1.07x
   HEAD. All four phase-03 review defects fixed, and the two blockers this
   phase's own review found fixed in stage 3 - deriving only the recipes that
   read what a merge moved, in dependency order. Four review gaps stay open in
   follow_up.md, none of them regressions. Lands as one change with phase 03;
   not committed.
