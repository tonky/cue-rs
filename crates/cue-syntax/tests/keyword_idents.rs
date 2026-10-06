//! Keywords double as identifiers upstream: `in.b`, `a.in`,
//! `for import in x`, `(t)!: v`, and `{let ...}` yields in list
//! comprehensions all parse.

use cue_syntax::ast::{Decl, Expr};

#[test]
fn keywords_stand_where_identifiers_belong() {
    let file = cue_syntax::parse_file("in: 1\na: {x: in}\nb: a.in\n").unwrap();
    assert_eq!(file.decls.len(), 3);
    let [_, Decl::Field(a), Decl::Field(b)] = file.decls.as_slice() else {
        panic!("expected fields: {:?}", file.decls);
    };
    assert!(matches!(a.value, Expr::Struct(_)), "x: in: {:?}", a.value);
    assert!(
        matches!(b.value, Expr::Selector { .. }),
        "a.in: {:?}",
        b.value
    );
}

#[test]
fn for_variables_may_be_named_like_keywords() {
    let file = cue_syntax::parse_file("a: [for import in x {import}]\n").unwrap();
    let [Decl::Field(field)] = file.decls.as_slice() else {
        panic!("expected one field: {:?}", file.decls);
    };
    let Expr::List(list) = &field.value else {
        panic!("expected list: {:?}", field.value);
    };
    let [Expr::ListComp(comp)] = list.elements.as_slice() else {
        panic!("expected one comprehension element: {:?}", list.elements);
    };
    assert_eq!(comp.clauses.len(), 1);
    assert!(
        matches!(
            &comp.clauses[0],
            cue_syntax::ast::ComprehensionClause::For { value, .. } if value == "import"
        ),
        "keyword loop variable: {:?}",
        comp.clauses[0]
    );
}

#[test]
fn required_markers_work_on_dynamic_labels() {
    let file = cue_syntax::parse_file("a: {(t1)?: (t2)!: 3}\n").unwrap();
    let [Decl::Field(field)] = file.decls.as_slice() else {
        panic!("expected one field: {:?}", file.decls);
    };
    let Expr::Struct(lit) = &field.value else {
        panic!("expected struct: {:?}", field.value);
    };
    assert_eq!(lit.decls.len(), 1);
    assert!(
        matches!(&lit.decls[0], Decl::Field(f) if f.optional),
        "optional outer label: {:?}",
        lit.decls[0]
    );
}

#[test]
fn list_comprehension_bodies_accept_let_yields() {
    let file = cue_syntax::parse_file("a: [if c {let x = 1\nx}, 2]\n").unwrap();
    let [Decl::Field(field)] = file.decls.as_slice() else {
        panic!("expected one field: {:?}", file.decls);
    };
    assert!(
        matches!(&field.value, Expr::List(_)),
        "expected list: {:?}",
        field.value
    );
}
