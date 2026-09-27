//! An empty struct is a valid list-comprehension body:
//! `[for y in src {}]` yields empty structs.

use cue_syntax::ast::{Decl, Expr};

#[test]
fn empty_struct_bodies_parse_in_list_comprehensions() {
    let file = cue_syntax::parse_file("a: [for y in [1, 2] {}]").unwrap();
    let [Decl::Field(field)] = file.decls.as_slice() else {
        panic!("expected one field: {:?}", file.decls);
    };
    let Expr::ListComp(comp) = &field.value else {
        panic!("expected list comprehension: {:?}", field.value);
    };
    assert_eq!(comp.clauses.len(), 1);
    let Expr::Struct(body) = comp.expr.as_ref() else {
        panic!("expected struct body: {:?}", comp.expr);
    };
    assert!(body.decls.is_empty());
}
