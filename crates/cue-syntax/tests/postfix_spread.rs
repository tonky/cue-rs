//! Postfix `...` is a spread expression, not a struct openness marker:
//! `b: A... @test(...)` parses the attribute onto the field, while an `...`
//! on its own line stays a `Decl::Ellipsis`.

use cue_syntax::ast::{Decl, Expr};

#[test]
fn spread_binds_inline_and_keeps_trailing_attributes() {
    let file = cue_syntax::parse_file("A: {a: 1}\nb: A... @test(eq, 1)\n").unwrap();
    let [_, Decl::Field(field)] = file.decls.as_slice() else {
        panic!("expected two fields: {:?}", file.decls);
    };
    assert_eq!(field.label.name(), Some("b"));
    assert!(
        matches!(field.value, Expr::Spread { .. }),
        "expected spread: {:?}",
        field.value
    );
    assert_eq!(field.attrs.len(), 1);
}

#[test]
fn file_attributes_may_stand_above_the_package_clause() {
    let file =
        cue_syntax::parse_file("\n@experiment(explicitopen)\n\npackage foo\n\nA: 1\n").unwrap();
    assert_eq!(file.package.as_deref(), Some("foo"));
    assert!(
        file.header
            .iter()
            .any(|decl| matches!(decl, Decl::Attribute(_))),
        "expected attribute in header: {:?}",
        file.header
    );
}

#[test]
fn spread_on_its_own_line_stays_an_openness_marker() {
    let file = cue_syntax::parse_file("a: 1\n...\n").unwrap();
    assert!(
        file.decls
            .iter()
            .any(|decl| matches!(decl, Decl::Ellipsis(_))),
        "expected openness marker: {:?}",
        file.decls
    );
}
