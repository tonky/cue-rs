//! A value read from a partial - a field still pending in the relaxation loop -
//! must not be taken for the finished one.
//!
//! enact's pipelines read their job names back out of the pipeline they are
//! declaring: `let J = pipeline.#jobs`, where `#jobs` is a comprehension over
//! `components`, and `J.test` inside that same pipeline. Two things passed a
//! partial off as resolved:
//!
//! - A merge re-derives the fields that read what it changed. A recipe that
//!   could not be derived yet kept the value it had *before* the merge - here the
//!   schema's own `#jobs`, empty - so `J` bound to it for good and `J.test` was
//!   `undefined field: test`.
//! - A definition read before its declaration was evaluated is a placeholder,
//!   and unifying with one yields the placeholder. `pipeline: #Pipeline & {…}`
//!   written above `#Pipeline` became that placeholder, was judged resolved, and
//!   exported as a structural cycle.
//!
//! Every expectation is upstream `cue export` v0.16.1's, recorded on 2026-09-25.

use cue_eval::eval_to_json;
use serde_json::json;

/// The schema first, as an imported package is; a component reads a field
/// declared below the pipeline, so the comprehension waits a pass for it.
const JOBS_FROM_A_PENDING_COMPREHENSION: &str = r#"
#Slots: ["lint", "typecheck", "test"]
#Pipeline: {
	caches?: [string]: used_by: [...string]
	components: [string]: {typecheck?: string, test?: string, service?: {...}}
	#jobs: {
		for _, c in components for k, v in c {
			for s in #Slots if k == s {(k): k}
			if k == "service" for a, _ in v if a == "test" {(a): a}
		}
	}
}
let J = pipeline.#jobs
pipeline: #Pipeline & {
	caches: {
		tsc: used_by: [J.typecheck]
		e2e: used_by: [J.test]
	}
	components: {
		api: {typecheck: "tsc", service: env.services.api}
		web: {test: "playwright"}
	}
}
env: services: api: {test: "curl", port: 8080}
"#;

#[test]
fn a_lookup_into_a_comprehension_waits_for_it() {
    let json = eval_to_json(JOBS_FROM_A_PENDING_COMPREHENSION).unwrap();
    assert_eq!(
        json["pipeline"]["caches"],
        json!({"tsc": {"used_by": ["typecheck"]}, "e2e": {"used_by": ["test"]}})
    );
}

/// The same with the schema below its use, as one file of a package may have it.
#[test]
fn the_same_with_the_schema_declared_last() {
    let (schema, rest) = JOBS_FROM_A_PENDING_COMPREHENSION
        .split_at(JOBS_FROM_A_PENDING_COMPREHENSION.find("let J").unwrap());
    let json = eval_to_json(&format!("{rest}\n{schema}")).unwrap();
    assert_eq!(
        json["pipeline"]["caches"],
        json!({"tsc": {"used_by": ["typecheck"]}, "e2e": {"used_by": ["test"]}})
    );
    assert_eq!(
        json["pipeline"]["components"]["api"]["service"],
        json!({"test": "curl", "port": 8080})
    );
}

/// Components built from a definition declared below them.
#[test]
fn a_definition_declared_below_its_use_is_waited_for() {
    let json = eval_to_json(
        r#"
let J = pipeline.#jobs
pipeline: #Pipeline & {
	stages: [{name: "pre", select: [J.lint]}]
	components: {api: #Python & {name: "api"}, web: #Python & {name: "web", jobs: e2e: {}}}
}
#Python: {name: string, lint: "ruff", jobs?: [string]: {}}
#Pipeline: {
	components: [string]: {...}
	stages: [...{name: string, select: [...string]}]
	#jobs: {for _, c in components for k, v in c {(k): k, if k == "jobs" for n, _ in v {(n): n}}}
}
"#,
    )
    .unwrap();
    assert_eq!(
        json,
        json!({"pipeline": {
            "components": {
                "api": {"name": "api", "lint": "ruff"},
                "web": {"name": "web", "lint": "ruff", "jobs": {"e2e": {}}},
            },
            "stages": [{"name": "pre", "select": ["lint"]}],
        }})
    );
}

/// A name the comprehension never generates is still an error once it settles.
#[test]
fn a_job_no_component_has_is_still_undefined() {
    let source = JOBS_FROM_A_PENDING_COMPREHENSION.replace("[J.test]", "[J.tset]");
    let error = eval_to_json(&source).unwrap_err().to_string();
    assert!(error.contains("tset"), "{error}");
}
