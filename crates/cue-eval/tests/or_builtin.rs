//! `or(list)` is the disjunction of the list's elements.
//!
//! Its usual argument is a comprehension - `or([for k, _ in #jobs {k}])` - over
//! fields a later conjunct has yet to add. So a `for` over a source that is not
//! resolved yet keeps the list pending rather than yielding nothing, and `or([])`
//! is incomplete, as upstream reports it, rather than a conflict: a schema read
//! on its own does not fail for want of the values its instances supply.
//!
//! Every expectation is upstream `cue export` v0.16.1's, recorded on 2026-09-25.

use cue_eval::eval_to_json;
use serde_json::json;

#[test]
fn or_accepts_an_element_of_the_list() {
    assert_eq!(
        eval_to_json(r#"m: "b" & or(["a", "b"])"#).unwrap(),
        json!({"m": "b"})
    );
}

/// Upstream: `m: 2 errors in empty disjunction: conflicting values "a" and "c" …`
#[test]
fn or_rejects_anything_else() {
    let error = eval_to_json(r#"m: "c" & or(["a", "b"])"#)
        .unwrap_err()
        .to_string();
    assert!(error.contains("conflicting values"), "{error}");
}

/// Upstream: `empty list in call to or`.
#[test]
fn or_of_an_empty_list_is_an_error() {
    let error = eval_to_json("m: or([])").unwrap_err().to_string();
    assert!(error.contains("empty list in call to or"), "{error}");
}

/// `#Later` is still a placeholder when `names` is first evaluated.
#[test]
fn a_list_comprehension_over_a_definition_declared_later_waits_for_it() {
    assert_eq!(
        eval_to_json("names: [for k, _ in #Later {k}]\n#Later: {p: 1, q: 2, r?: 3}").unwrap(),
        json!({"names": ["p", "q"]})
    );
}

/// The shape enact's pipeline schema has: a stage may select only the jobs its
/// components define, and those are computed from the instance.
const PIPELINE: &str = r#"
#Slots: ["lint", "fmt", "test", "build"]
#Component: {lint?: string, fmt?: string, test?: string, build?: string, jobs?: [string]: {tasks: [...string]}}
#Pipeline: {
	components: [string]: #Component
	#jobs: {
		for _, c in components for k, v in c {
			for s in #Slots if k == s {(k): k}
			if k == "jobs" for n, _ in v {(n): n}
		}
	}
	workflows?: [string]: stages: [...{name: string, select?: [...or([for k, _ in #jobs {k}])]}]
}
pipeline: #Pipeline & {
	components: {api: {lint: "ruff", jobs: unit: tasks: ["pytest"]}, web: {test: "vitest", lint: "eslint"}}
	workflows: ci: stages: [{name: "pre", select: [SELECT]}]
}
"#;

/// The same, finding the jobs with an existence check instead of the loop key.
const PIPELINE_CHECKED: &str = r#"
#Component: {lint?: string, jobs?: [string]: {tasks: [...string]}}
#Pipeline: {
	components: [string]: #Component
	#jobs: {
		for _, c in components {
			if c.lint != _|_ {lint: "lint"}
			if c.jobs != _|_ for n, _ in c.jobs {(n): n}
		}
	}
	workflows?: [string]: stages: [...{name: string, select?: [...or([for k, _ in #jobs {k}])]}]
}
pipeline: #Pipeline & {
	components: {api: {lint: "ruff", jobs: unit: tasks: ["pytest"]}, web: {}}
	workflows: ci: stages: [{name: "pre", select: [SELECT]}]
}
"#;

#[test]
fn a_stage_selects_only_the_jobs_its_components_define() {
    for (case, schema) in [("slots", PIPELINE), ("existence check", PIPELINE_CHECKED)] {
        let accepted = eval_to_json(&schema.replace("SELECT", r#""lint", "unit""#))
            .unwrap_or_else(|e| panic!("{case}: {e}"));
        assert_eq!(
            accepted["pipeline"]["workflows"]["ci"]["stages"][0]["select"],
            json!(["lint", "unit"]),
            "{case}"
        );

        // Upstream: `3 errors in empty disjunction: conflicting values "lint" and "unti" …`
        let error = eval_to_json(&schema.replace("SELECT", r#""lint", "unti""#))
            .expect_err(case)
            .to_string();
        assert!(error.contains("\"unti\""), "{case}: {error}");
    }
}
