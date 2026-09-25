//! A comprehension costs what it generates, not that squared.
//!
//! Each iteration's body is a struct of its own, merged into the struct the
//! comprehension generates into. Evaluated in place, every iteration cloned and
//! re-derived everything the iterations before it had generated: enact's
//! pipeline schema collects every job name of every component this way, and a
//! pipeline of a few hundred components spent seconds there - quadratic in the
//! component count. The cost is in walking the generated struct, which no
//! counter sees, so this one bounds wall time, generously: linear evaluation
//! takes well under a second here even unoptimised, quadratic took half a
//! minute.

use cue_eval::eval_to_json;
use std::time::{Duration, Instant};

#[test]
fn a_comprehension_over_many_components_is_not_quadratic() {
    let components = 3000;
    let mut source = String::from("components: {\n");
    for i in 0..components {
        source.push_str(&format!(
            "\tc{i}: {{lint: \"ruff\", jobs: {{build: {{}}, \"unit{}\": {{}}}}}}\n",
            i % 3
        ));
    }
    // A computed label beside a nested comprehension: the body shape that
    // re-derived the whole generated struct per iteration.
    source.push_str(
        "}\njobs: {\n\tfor _, c in components for k, v in c {\n\t\t(k): k\n\t\tif k == \"jobs\" for n, _ in v {(n): n}\n\t}\n}\n",
    );

    let started = Instant::now();
    let json = eval_to_json(&source).unwrap();
    let elapsed = started.elapsed();

    assert_eq!(
        json["jobs"],
        serde_json::json!({
            "lint": "lint", "jobs": "jobs", "build": "build",
            "unit0": "unit0", "unit1": "unit1", "unit2": "unit2",
        })
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "{components} components took {elapsed:?}"
    );
}
