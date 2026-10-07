//! A field declared thousands of times, and an expression chaining hundreds of
//! binary operators, evaluate as cue does - no depth limit.
//!
//! `eval_expr` caps its recursion at `MAX_EXPR_DEPTH`; repeated declarations
//! used to fold into one left-deep `&` chain, so the 65th declaration of a
//! field (and the 65th operand of `1 + 1 + ...`) exceeded the cap. A chain of
//! binary operators is now evaluated as a loop over its left spine, and repeated
//! declarations fold into a balanced tree, keeping the depth logarithmic. A field
//! a pattern constraint matches meets that pattern once per merge, not again for
//! every declaration of the field, so the `#Svc` defaults of one service never
//! conflict with another's values. The goldens come from cue v0.17.1;
//! regenerate them with `regen.py` in `fixtures/long_conjunctions`.

mod support;

#[test]
fn long_conjunctions_match_cue_export() {
    support::match_cue_export("long_conjunctions", 13, &[]);
}
