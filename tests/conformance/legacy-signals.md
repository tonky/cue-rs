# The 43 legacy failure signals

These are the original phase-11 failure names, retained without deleting or
rewriting fixtures. Counts describe observations, not independent bugs: one
package-load failure can fail many checks. Unsupported adapters are verification
debt, not semantic passes.

| Case | New check outcomes |
| --- | --- |
| upstream_cue_testdata_builtins_056_issue314.txtar | 3 mismatch, 2 not_applicable, 8 unsupported |
| upstream_cue_testdata_builtins_issue299.txtar | 1 mismatch, 1 not_applicable, 2 unsupported |
| upstream_cue_testdata_builtins_or.txtar | 1 not_applicable, 4 unsupported |
| upstream_cue_testdata_comprehensions_015_list_comprehension.txtar | 3 mismatch, 1 not_applicable, 1 passed |
| upstream_cue_testdata_comprehensions_issue1732.txtar | 1 mismatch, 2 not_applicable, 1 passed, 3 unsupported |
| upstream_cue_testdata_comprehensions_issue837.txtar | 2 mismatch, 2 not_applicable, 1 passed, 12 unsupported |
| upstream_cue_testdata_definitions_032_definitions_with_embedding.txtar | 2 not_applicable, 3 unsupported |
| upstream_cue_testdata_definitions_036_closing_with_failed_optional.txtar | 1 mismatch, 2 not_applicable, 5 passed, 7 unsupported |
| upstream_cue_testdata_definitions_037_conjunction_of_optional_sets.txtar | 2 not_applicable, 5 unsupported |
| upstream_cue_testdata_definitions_defembed.txtar | 1 mismatch, 2 not_applicable, 1 passed, 2 unsupported |
| upstream_cue_testdata_definitions_embed.txtar | 1 mismatch, 2 not_applicable, 6 unsupported |
| upstream_cue_testdata_definitions_exclude.txtar | 1 mismatch, 3 not_applicable, 1 passed, 7 unsupported |
| upstream_cue_testdata_definitions_hidden.txtar | 3 not_applicable, 2 unsupported |
| upstream_cue_testdata_definitions_root4.txtar | 1 mismatch, 2 not_applicable, 3 unsupported |
| upstream_cue_testdata_disjunctions_embed.txtar | 1 not_applicable, 10 unsupported |
| upstream_cue_testdata_disjunctions_propagate_err.txtar | 1 mismatch, 1 not_applicable, 3 passed, 2 unsupported |
| upstream_cue_testdata_eval_basictypes.txtar | 1 not_applicable, 2 unsupported |
| upstream_cue_testdata_eval_insertion.txtar | 3 mismatch, 1 not_applicable, 5 unsupported |
| upstream_cue_testdata_eval_issue2568.txtar | 1 mismatch, 2 not_applicable, 2 passed, 4 unsupported |
| upstream_cue_testdata_eval_issue2649.txtar | 1 not_applicable, 4 passed |
| upstream_cue_testdata_eval_v0.7.txtar | 26 mismatch, 2 not_applicable, 13 unsupported |
| upstream_cue_testdata_export_000.txtar | 1 not_applicable, 1 passed, 2 unsupported |
| upstream_cue_testdata_export_001.txtar | 1 not_applicable, 1 passed, 2 unsupported |
| upstream_cue_testdata_export_002.txtar | 1 not_applicable, 1 passed, 2 unsupported |
| upstream_cue_testdata_export_003.txtar | 1 not_applicable, 1 passed, 2 unsupported |
| upstream_cue_testdata_fulleval_012_disjunctions_of_lists.txtar | 1 not_applicable, 1 passed |
| upstream_cue_testdata_fulleval_014_default_disambiguation_and_elimination.txtar | 1 not_applicable, 5 unsupported |
| upstream_cue_testdata_fulleval_029_Issue_#94.txtar | 3 mismatch, 1 not_applicable, 6 passed, 9 unsupported |
| upstream_cue_testdata_fulleval_032_or_builtin_should_not_fail_on_non-concrete_empty_list.txtar | 1 not_applicable, 2 passed, 5 unsupported |
| upstream_cue_testdata_fulleval_034_label_and_field_aliases.txtar | 5 mismatch, 2 not_applicable, 3 unsupported |
| upstream_cue_testdata_fulleval_049_alias_reuse_in_nested_scope.txtar | 1 not_applicable, 1 passed, 4 unsupported |
| upstream_cue_testdata_fulleval_051_detectIncompleteYAML.txtar | 1 not_applicable, 1 passed, 7 unsupported |
| upstream_cue_testdata_fulleval_052_detectIncompleteJSON.txtar | 1 not_applicable, 1 passed, 5 unsupported |
| upstream_cue_testdata_fulleval_055_issue318.txtar | 1 not_applicable, 3 passed, 2 unsupported |
| upstream_cue_testdata_interpolation_issue487.txtar | 1 not_applicable, 5 passed, 2 unsupported |
| upstream_cue_testdata_interpolation_scalars.txtar | 7 mismatch, 2 not_applicable, 4 unsupported |
| upstream_cue_testdata_packages_embed.txtar | 1 mismatch, 1 not_applicable, 1 unsupported |
| upstream_cue_testdata_packages_issue398.txtar | 1 mismatch, 2 not_applicable, 1 unsupported |
| upstream_cue_testdata_packages_sub.txtar | 1 not_applicable, 1 passed, 1 unsupported |
| upstream_cue_testdata_resolve_010_optional_field_resolves_to_incomplete.txtar | 2 mismatch, 1 not_applicable, 4 unsupported |
| upstream_cue_testdata_resolve_025_definitions.txtar | 5 mismatch, 2 not_applicable, 1 passed, 9 unsupported |
| upstream_cue_testdata_scalars_embed_bound.txtar | 1 not_applicable, 1 passed, 2 unsupported |
| upstream_cue_testdata_scalars_emptystruct.txtar | 1 not_applicable, 9 unsupported |

Original complaints are in `legacy-signals.json`. All 547 cases and check IDs
are recorded in `baseline.json`; regenerate full diagnostics with
`just safe just conformance` (written to `tmp/conformance-report.json`).

Whole-file missing-error complaints are not preserved as assertions. Inline
errors are checked at their annotated paths: incomplete definitions can coexist
with a valid export. Scalar-root and package-loading differences remain visible
under package/export operations.

Phase 13 must address confirmed observation mismatches and the individually
recorded abstract-value, diagnostic, golden and configuration adapter debt.
This matrix does not declare every old signal to be an evaluator bug.
