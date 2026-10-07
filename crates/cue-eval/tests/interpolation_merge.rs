//! An interpolation over a value that is not concrete yet waits for the merge
//! that supplies it, exactly like `cue export`.
//!
//! `t: {p: string, l: "c \(p)"}` met with `{p: "x"}` exports `l: "c x"` also
//! when another conjunct of `l` (`l: string`, `l?: _`, a `#Task` branch of a
//! disjunction) or an embedding (`_onMac: {worker: "mac", ...}`) has the
//! template's fields evaluated before `p` arrives: the interpolation is
//! incomplete then, not a conflict. Hidden, regular and definition embeddings,
//! embeddings under a guard and templates instantiated in comprehensions,
//! dynamic labels, guards, list elements and `let` bindings are covered. An
//! operand never made concrete still fails (`x.l: invalid interpolation`), and
//! a struct operand is still an error. The goldens under
//! `fixtures/interpolation_merge` come from the official `cue` binary;
//! regenerate them with its `regen.py`.

mod support;

#[test]
fn interpolation_merge_matches_cue_export() {
    support::match_cue_export("interpolation_merge", 25, &[]);
}
