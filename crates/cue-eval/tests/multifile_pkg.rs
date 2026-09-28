use cue_eval::PackageLoader;
use serde_json::json;

#[test]
fn test_repro_pkg_order() {
    let module = tempfile::tempdir().expect("temp dir");
    let root = module.path();
    std::fs::create_dir_all(root.join("cue.mod")).unwrap();
    std::fs::write(
        root.join("cue.mod/module.cue"),
        "module: \"example.com/main@v0\"\nlanguage: version: \"v0.16.1\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("pkg")).unwrap();
    std::fs::write(
        root.join("pkg/a.cue"),
        r#"package pkg
#A: {
	workflows?: [string]: #B & {
		stages?: [...{
			select?: [...string]
			...
		}]
		...
	}
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("pkg/z.cue"),
        r#"package pkg
#Stage: {
	select?: [...string]
}
#B: {
	stages?: [...#Stage]
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("in.cue"),
        r#"package main
import "example.com/main/pkg"
out: pkg.#A & {
	workflows: ci: {
		stages: [{
			select: ["lint"]
		}]
	}
}
"#,
    )
    .unwrap();

    let (evaluator, root_id) = PackageLoader::load_file(root.join("in.cue")).unwrap();
    let json_val = evaluator.to_json(root_id).unwrap();
    assert_eq!(
        json_val["out"],
        json!({
            "workflows": {
                "ci": {
                    "stages": [{
                        "select": ["lint"]
                    }]
                }
            }
        })
    );
}

#[test]
fn test_enact_scaffold_with_comprehension_and_disjunction() {
    let module = tempfile::tempdir().expect("temp dir");
    let root = module.path();
    std::fs::create_dir_all(root.join("cue.mod")).unwrap();
    std::fs::write(
        root.join("cue.mod/module.cue"),
        "module: \"example.com/main@v0\"\nlanguage: version: \"v0.16.1\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("pkg")).unwrap();
    std::fs::write(
        root.join("pkg/pipeline.cue"),
        r#"package pkg

#Slots: ["lint", "fmt", "test"]

#jobs: {
	for s in #Slots {(s): s}
}

#jobName: or([for k, _ in #jobs {k}])

#TaskSelector: {
	job?: string
}

#Pipeline: {
	workflows?: [string]: #Workflow & {
		stages?: [...{
			select?: [...(#jobName | #TaskSelector)]
			...
		}]
		...
	}
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("pkg/workflow.cue"),
        r#"package pkg

#Stage: {
	select?: [...string | #TaskSelector]
}

#Workflow: {
	stages?: [...#Stage]
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("in.cue"),
        r#"package main

import "example.com/main/pkg"

out: pkg.#Pipeline & {
	workflows: ci: {
		stages: [{
			select: ["lint"]
		}]
	}
}
"#,
    )
    .unwrap();

    let (evaluator, root_id) = PackageLoader::load_file(root.join("in.cue")).unwrap();
    let json_val = evaluator.to_json(root_id).unwrap();
    assert_eq!(
        json_val["out"],
        json!({
            "workflows": {
                "ci": {
                    "stages": [{
                        "select": ["lint"]
                    }]
                }
            }
        })
    );
}

#[test]
fn test_enact_scaffold_rejects_invalid_job() {
    let module = tempfile::tempdir().expect("temp dir");
    let root = module.path();
    std::fs::create_dir_all(root.join("cue.mod")).unwrap();
    std::fs::write(
        root.join("cue.mod/module.cue"),
        "module: \"example.com/main@v0\"\nlanguage: version: \"v0.16.1\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("pkg")).unwrap();
    std::fs::write(
        root.join("pkg/pipeline.cue"),
        r#"package pkg

#Slots: ["lint", "fmt", "test"]

#jobs: {
	for s in #Slots {(s): s}
}

#jobName: or([for k, _ in #jobs {k}])

#TaskSelector: {
	job?: string
}

#Pipeline: {
	workflows?: [string]: #Workflow & {
		stages?: [...{
			select?: [...(#jobName | #TaskSelector)]
			...
		}]
		...
	}
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("pkg/workflow.cue"),
        r#"package pkg

#Stage: {
	select?: [...string | #TaskSelector]
}

#Workflow: {
	stages?: [...#Stage]
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("in.cue"),
        r#"package main

import "example.com/main/pkg"

out: pkg.#Pipeline & {
	workflows: ci: {
		stages: [{
			select: ["invalid_job_name"]
		}]
	}
}
"#,
    )
    .unwrap();

    let (evaluator, root_id) = PackageLoader::load_file(root.join("in.cue")).unwrap();
    assert!(
        evaluator.to_json(root_id).is_err(),
        "should reject invalid job name"
    );
}


#[test]
fn test_enact_schema_direct() {
    let enact_schema = std::path::Path::new("/home/tonky/projects/enact/schema");
    if !enact_schema.exists() {
        return;
    }
    let res = PackageLoader::load_dir(enact_schema);
    assert!(res.is_ok(), "failed to load enact schema: {:?}", res.err());
}

#[test]
fn test_enact_pipeline_workflow_instantiation() {
    let enact_schema = std::path::Path::new("/home/tonky/projects/enact/schema");
    if !enact_schema.exists() {
        return;
    }
    let module = tempfile::tempdir().expect("temp dir");
    let root = module.path();
    std::fs::create_dir_all(root.join("cue.mod")).unwrap();
    std::fs::write(
        root.join("cue.mod/module.cue"),
        "module: \"enact.dev@v0\"\nlanguage: version: \"v0.16.1\"\n",
    )
    .unwrap();
    let schema_dest = root.join("schema");
    std::fs::create_dir_all(&schema_dest).unwrap();
    for entry in std::fs::read_dir(enact_schema).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().and_then(|s| s.to_str()) == Some("cue") {
            std::fs::copy(entry.path(), schema_dest.join(entry.file_name())).unwrap();
        }
    }

    let test_file = root.join("test_pipe.cue");
    std::fs::write(
        &test_file,
        r#"package main

import "enact.dev/schema"

pipe: schema.#Pipeline & {
	name: "test-pipeline"
	workflows: ci: {
		stages: [{
			name: "test"
			select: ["lint"]
		}]
	}
}
"#,
    )
    .unwrap();
    let (evaluator, root_id) = PackageLoader::load_file(&test_file).expect("load test_pipe.cue");
    let json_val = evaluator.to_json(root_id).expect("export to json");
    assert_eq!(json_val["pipe"]["name"], "test-pipeline");
    assert_eq!(
        json_val["pipe"]["workflows"]["ci"]["stages"][0]["select"],
        json!(["lint"])
    );
}

#[test]
fn test_enact_pipeline_workflow_rejects_invalid_job() {
    let enact_schema = std::path::Path::new("/home/tonky/projects/enact/schema");
    if !enact_schema.exists() {
        return;
    }
    let module = tempfile::tempdir().expect("temp dir");
    let root = module.path();
    std::fs::create_dir_all(root.join("cue.mod")).unwrap();
    std::fs::write(
        root.join("cue.mod/module.cue"),
        "module: \"enact.dev@v0\"\nlanguage: version: \"v0.16.1\"\n",
    )
    .unwrap();
    let schema_dest = root.join("schema");
    std::fs::create_dir_all(&schema_dest).unwrap();
    for entry in std::fs::read_dir(enact_schema).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().and_then(|s| s.to_str()) == Some("cue") {
            std::fs::copy(entry.path(), schema_dest.join(entry.file_name())).unwrap();
        }
    }

    let test_file = root.join("test_pipe.cue");
    std::fs::write(
        &test_file,
        r#"package main

import "enact.dev/schema"

pipe: schema.#Pipeline & {
	name: "test-pipeline"
	workflows: ci: {
		stages: [{
			name: "test"
			select: ["invalid_job_name"]
		}]
	}
}
"#,
    )
    .unwrap();
    let res = PackageLoader::load_file(&test_file);
    if let Ok((evaluator, root_id)) = res {
        assert!(evaluator.to_json(root_id).is_err(), "should reject invalid job name");
    }
}





