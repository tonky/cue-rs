//! Session-owned immutable syntax and direct dependencies, independent of scopes.
use crate::deps::{direct_deps, expand_lets};
use cue_syntax::ast::Expr;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Default)]
pub(crate) struct ExpressionStore {
    expressions: HashMap<Rc<Expr>, Rc<HashSet<String>>>,
}

pub(crate) struct RecipeSource {
    pub expr: Rc<Expr>,
    pub deps: Rc<HashSet<String>>,
}

impl ExpressionStore {
    /// Exact AST equality permits storage sharing only. Evaluation still uses
    /// each recipe's own lexical environment, imports and dependency closure.
    pub(crate) fn intern(&mut self, expr: &Expr) -> Rc<Expr> {
        if let Some((stored, _)) = self.expressions.get_key_value(expr) {
            return stored.clone();
        }
        self.insert(expr, direct_deps(expr)).expr
    }

    /// Binding-free expressions need no recipe. Direct dependencies are cached,
    /// while a referenced local let/alias is expanded within this lexical scope.
    pub(crate) fn prepare_recipe(
        &mut self,
        expr: &Expr,
        lets: &[(String, Rc<Expr>)],
    ) -> Option<RecipeSource> {
        let mut source = if let Some((expr, deps)) = self.expressions.get_key_value(expr) {
            RecipeSource {
                expr: expr.clone(),
                deps: deps.clone(),
            }
        } else {
            let deps = direct_deps(expr);
            if deps.is_empty() {
                return None;
            }
            self.insert(expr, deps)
        };
        if source.deps.is_empty() {
            return None;
        }
        if lets.iter().any(|(name, _)| source.deps.contains(name)) {
            let expanded = expand_lets((*source.deps).clone(), lets);
            if expanded != *source.deps {
                source.deps = Rc::new(expanded);
            }
        }
        Some(source)
    }

    fn insert(&mut self, expr: &Expr, deps: HashSet<String>) -> RecipeSource {
        let expr = Rc::new(expr.clone());
        let deps = Rc::new(deps);
        self.expressions.insert(expr.clone(), deps.clone());
        RecipeSource { expr, deps }
    }
}
