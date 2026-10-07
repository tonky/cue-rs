//! A builtin over an argument that is not concrete yet waits for the merge
//! that supplies it, exactly like `cue export`.
//!
//! `_t: {n: string, u: strings.ToUpper(n)}` met with `{n: "a"}` exports
//! `u: "A"` also when another conjunct of `u` (`u: string`, `u?: _`) or an
//! embedding in the template has `u` evaluated before `n` arrives: a call
//! whose argument may still take the kind the builtin reads is incomplete
//! then, not a conflict. That holds for strings, numbers, bounds, typed list
//! elements, data inside a marshalled value, `len`, definitions, split
//! declarations and templates instantiated in comprehensions. An argument of
//! a kind the builtin can never read (`int` for a string, `[int, ...string]`
//! for a list of strings, `2.5` for an int) is still an error, and so is one
//! never made concrete. The goldens under `fixtures/builtin_arguments` come
//! from the official `cue` binary; regenerate them with its `regen.py`.

mod support;

#[test]
fn builtin_arguments_match_cue_export() {
    support::match_cue_export("builtin_arguments", 22, &[]);
}
