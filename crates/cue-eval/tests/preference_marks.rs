//! `*` is a unary operator. It marks a default only where it is itself a
//! disjunct (`*a | b`, `**a | b`, `*(a | b) | c`); anywhere else - inside a
//! conjunction, alone, in a list, under another operator - cue rejects the
//! file at compile time: `preference mark not allowed at this position`. The
//! parser took a `*` at the start of a branch as marking the whole branch, so
//! `*1 & int | 2` defaulted to `1 & int`, and evaluation let a stray mark pass
//! through. The goldens come from cue v0.17.1; regenerate them with `regen.py`
//! in `fixtures/preference_marks`.

mod support;

#[test]
fn preference_marks_match_cue_export() {
    support::match_cue_export("preference_marks", 17, &[]);
}

/// The path-only comparison above would accept any error at the right field;
/// a misplaced mark must be reported as one.
#[test]
fn a_misplaced_mark_is_reported_as_such() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/preference_marks");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "err") {
            continue;
        }
        let cue = std::fs::read_to_string(&path).unwrap();
        let Some(first) = cue.lines().next().filter(|l| l.contains("preference mark")) else {
            continue;
        };
        let source = std::fs::read_to_string(path.with_extension("cue")).unwrap();
        let error = cue_eval::eval_to_json(&source).unwrap_err().to_string();
        let expected = first.trim_end_matches(':');
        assert!(error.contains(expected), "{}: {error}", path.display());
        checked += 1;
    }
    assert!(checked >= 12, "fixtures missing");
}
