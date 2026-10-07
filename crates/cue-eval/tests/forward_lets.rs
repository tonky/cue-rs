//! A `let` or field read before its declaration, from a comprehension body or
//! from another `let`, must take the value cue gives it - also after the struct
//! is merged with an override.
//!
//! A comprehension whose generated fields read a name of its own literal that
//! is still pending (`if true {x: a}` above `a: 1`, `let d = a + 1` above `a`)
//! is retried by a later relaxation pass instead of keeping the pending value.
//! Re-derivation after a merge derives a struct's lets in dependency order, so
//! `let b = a2` above `let a2 = p` sees the merged `p` rather than the `a2`
//! captured before the merge. The goldens come from cue v0.17.1; regenerate them
//! with `regen.py` in `fixtures/forward_lets`.

mod support;

/// Fixtures cue-rs still evaluates differently, each with the reason. The test
/// asserts they *still* differ, so a fix fails here and forces its entry out.
const KNOWN_DIVERGENCES: &[(&str, &str)] = &[(
    "let_cycle.cue",
    "both reject `let a = b; let b = a`; cue names the cycle at `P.let[]`, cue-rs the unresolved reference at its use `P.x`",
)];

#[test]
fn forward_lets_match_cue_export() {
    support::match_cue_export("forward_lets", 30, KNOWN_DIVERGENCES);
}
