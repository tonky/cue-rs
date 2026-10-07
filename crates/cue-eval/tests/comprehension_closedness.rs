//! Closed definitions whose fields come from comprehensions, and the path an
//! error names, must behave exactly like `cue export`.
//!
//! A field an `if` or `for` in a definition's own body adds is part of that
//! definition, also when the guard reads a field the definition is unified
//! with later, and in embedded, nested, listed, pattern-matched and disjoined
//! definitions. A field the definition never adds stays refused. An error
//! names the full path of the field that became bottom (`x.o.1.t: ...`), as
//! cue does. A generated field reads the struct it was merged into, as any
//! other field does. The goldens under `fixtures/comprehension_closedness`,
//! `fixtures/comprehension_sibling_reads`, `fixtures/error_paths` and
//! `fixtures/issue3851` come from the official `cue` binary; regenerate them
//! with `regen.py` in each.

mod support;

/// Fixtures cue-rs still evaluates differently, each with the reason. The test
/// asserts they *still* differ, so a fix fails here and forces its entry out.
const KNOWN_DIVERGENCES: &[(&str, &str)] = &[
    (
        "conflict_after_incomplete.cue",
        "cue reports the conflict at `t.a.b` before the incomplete `s.a.b`; cue-rs reports the first field it exports",
    ),
    (
        "def_for_adds_under_dynamic_nested_def.cue",
        "both refuse `lab` under the nested #X; cue-rs fails the whole evaluation (the `for` source re-derived to that error) instead of naming `a.s.admin.lab`",
    ),
];

#[test]
fn comprehension_closedness_matches_cue_export() {
    support::match_cue_export("comprehension_closedness", 60, KNOWN_DIVERGENCES);
}

/// A field a comprehension generates reads the merged struct: a sibling of the
/// literal the comprehension is written in (`if c {x: a}` with `a` overridden),
/// and what a `for` or `let` clause bound from one, when the merge leaves the
/// clauses deciding as they did - in plain structs and definitions, nested
/// comprehensions, several merges in a row and listed or embedded definitions;
/// and a pattern's target meeting the fields it matches the same way.
#[test]
fn comprehension_sibling_reads_match_cue_export() {
    support::match_cue_export("comprehension_sibling_reads", 42, KNOWN_DIVERGENCES);
}

#[test]
fn error_paths_match_cue_export() {
    support::match_cue_export("error_paths", 16, KNOWN_DIVERGENCES);
}

/// Reductions of upstream `disjunctions/issue3857` (issue3851 t2): a root
/// pattern beside a definition whose `conf` pattern holds `#Config`, with the
/// `one` entry added by an `if` or declared directly, and a disjunction (or a
/// field the definition does not allow) added through the `["one"]` pattern.
/// `r5`/`r7` (`env1.conf.one.disj: field not allowed`) match cue.
const ISSUE3851_DIVERGENCES: &[(&str, &str)] = &[
    (
        "r8.cue",
        "both refuse a field under the closed #Env; cue names `env1.conf.one.disj`, cue-rs `env1.conf.two`",
    ),
    (
        "x3.cue",
        "`#Env & #T` with `#T` open: cue refuses `#T`'s `disj` under `#Config`, cue-rs exports it",
    ),
];

#[test]
fn issue3851_reductions_match_cue_export() {
    support::match_cue_export("issue3851", 38, ISSUE3851_DIVERGENCES);
}
