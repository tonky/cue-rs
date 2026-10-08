//! A quoted label is a regular field, whatever its text spells: `"#e"` is not
//! the definition `#e`, `"_x"` not the hidden `_x`, `"if"` not a keyword. The
//! parser read `a."#e"` and `a.#e` to one selector, so the evaluator looked the
//! name up in every section and a definition or hidden field could answer for
//! the field (or the field for them), and a regular field read that way was
//! closed as if it were a definition. The goldens come from cue v0.17.1;
//! regenerate them with `regen.py` in `fixtures/quoted_labels`.

mod support;

#[test]
fn quoted_labels_match_cue_export() {
    support::match_cue_export("quoted_labels", 10, &[]);
}
