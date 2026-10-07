//! A builtin applied to a value that is not resolved yet must wait for it, as
//! cue does: `z: len(s)` above `s: [1]`, `strings.Join(L, ",")` over a `let`
//! comprehension whose source is declared further down, `len(_all)` inside a
//! `for` over hidden fields, `close(s)` and `div(a, 2)` over later fields. An
//! argument that is an error is the call's error (`signature::before_call` for
//! a stdlib builtin, the top of `builtins::call` for the others), so one not
//! resolved yet keeps its kind and the pass that resolves it retries the call,
//! instead of the builtin failing for good on the placeholder.
//! `error("...\(x)")` keeps its message: it is the one builtin that takes a
//! failed argument. The goldens come from cue v0.17.1; regenerate them with
//! `regen.py` in `fixtures/pending_arguments`.

mod support;

#[test]
fn pending_arguments_match_cue_export() {
    support::match_cue_export("pending_arguments", 19, &[]);
}
