use crate::unify::unify;
use crate::value::{
    BoundOp, DisjunctionBranch as ValueBranch, StructValue, TypeKind, Value, ValueArena, ValueId,
};
use cue_syntax::ast::*;
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EvalError {
    #[error("Parse error: {0}")]
    Parse(#[from] cue_syntax::parser::ParseError),
    #[error("Evaluation error: {0}")]
    Evaluation(String),
}

pub struct Evaluator {
    pub arena: ValueArena,
    scopes: Vec<HashMap<String, ValueId>>,
    resolving_symbols: HashSet<String>,
    pub placeholders: HashMap<String, ValueId>,
    pub import_aliases: HashMap<String, String>,
    pub imported_packages: HashMap<String, ValueId>,
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl Evaluator {
    pub fn new() -> Self {
        let mut evaluator = Self {
            arena: ValueArena::new(),
            scopes: vec![HashMap::new()],
            resolving_symbols: HashSet::new(),
            placeholders: HashMap::new(),
            import_aliases: HashMap::new(),
            imported_packages: HashMap::new(),
        };
        evaluator.register_builtins();
        evaluator
    }

    fn register_builtins(&mut self) {
        // Top-level type builtins
        let top = self.arena.alloc(Value::Type(TypeKind::Top));
        let null = self.arena.alloc(Value::Type(TypeKind::Null));
        let bool_t = self.arena.alloc(Value::Type(TypeKind::Bool));
        let int_t = self.arena.alloc(Value::Type(TypeKind::Int));
        let uint_t = self.arena.alloc(Value::Type(TypeKind::Uint));
        let uint8_t = self.arena.alloc(Value::Type(TypeKind::Uint8));
        let uint16_t = self.arena.alloc(Value::Type(TypeKind::Uint16));
        let uint32_t = self.arena.alloc(Value::Type(TypeKind::Uint32));
        let uint64_t = self.arena.alloc(Value::Type(TypeKind::Uint64));
        let int8_t = self.arena.alloc(Value::Type(TypeKind::Int8));
        let int16_t = self.arena.alloc(Value::Type(TypeKind::Int16));
        let int32_t = self.arena.alloc(Value::Type(TypeKind::Int32));
        let int64_t = self.arena.alloc(Value::Type(TypeKind::Int64));
        let float_t = self.arena.alloc(Value::Type(TypeKind::Float));
        let float32_t = self.arena.alloc(Value::Type(TypeKind::Float32));
        let float64_t = self.arena.alloc(Value::Type(TypeKind::Float64));
        let num_t = self.arena.alloc(Value::Type(TypeKind::Number));
        let string_t = self.arena.alloc(Value::Type(TypeKind::String));
        let bytes_t = self.arena.alloc(Value::Type(TypeKind::Bytes));

        self.insert_binding("_", top);
        self.insert_binding("null", null);
        self.insert_binding("bool", bool_t);
        self.insert_binding("int", int_t);
        self.insert_binding("uint", uint_t);
        self.insert_binding("uint8", uint8_t);
        self.insert_binding("uint16", uint16_t);
        self.insert_binding("uint32", uint32_t);
        self.insert_binding("uint64", uint64_t);
        self.insert_binding("int8", int8_t);
        self.insert_binding("int16", int16_t);
        self.insert_binding("int32", int32_t);
        self.insert_binding("int64", int64_t);
        self.insert_binding("float", float_t);
        self.insert_binding("float32", float32_t);
        self.insert_binding("float64", float64_t);
        self.insert_binding("number", num_t);
        self.insert_binding("string", string_t);
        self.insert_binding("bytes", bytes_t);
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn insert_binding(&mut self, name: &str, val: ValueId) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), val);
        }
    }

    pub fn lookup_binding(&self, name: &str) -> Option<ValueId> {
        for scope in self.scopes.iter().rev() {
            if let Some(&val) = scope.get(name) {
                return Some(val);
            }
        }
        None
    }

    /// Evaluate an entire CUE source file.
    pub fn eval_file(&mut self, file: &SourceFile) -> Result<ValueId, EvalError> {
        for imp in &file.imports {
            let pkg_name = if let Some(alias) = &imp.alias {
                alias.clone()
            } else {
                imp.path.split('/').next_back().unwrap_or(&imp.path).to_string()
            };
            self.import_aliases.insert(pkg_name, imp.path.clone());
        }

        let mut root_struct = StructValue::new(false);
        self.eval_decls_into_struct(&file.decls, &mut root_struct)?;
        Ok(self.arena.alloc(Value::Struct(root_struct)))
    }

    pub fn eval_decls_into_struct(
        &mut self,
        decls: &[Decl],
        target_struct: &mut StructValue,
    ) -> Result<(), EvalError> {
        // Pass 1: Pre-register definition placeholders for recursive and forward references
        for decl in decls {
            if let Decl::Field(f) = decl
                && let Some(name) = f.label.name()
                    && f.label.is_definition()
                        && !self.placeholders.contains_key(name) {
                            let placeholder = self.arena.alloc(Value::RecursiveRef {
                                name: name.to_string(),
                                target: None,
                            });
                            self.insert_binding(name, placeholder);
                            self.placeholders.insert(name.to_string(), placeholder);
                        }
        }

        // Multi-pass relaxation loop for forward / order-independent references
        let mut pending_decls: Vec<&Decl> = decls.iter().collect();
        let max_iterations = decls.len() + 3;
        let mut iteration = 0;

        while !pending_decls.is_empty() && iteration < max_iterations {
            iteration += 1;
            let mut next_pending = Vec::new();
            let mut made_progress = false;

            for &decl in &pending_decls {
                let resolved = self.eval_single_decl(decl, target_struct)?;
                if resolved {
                    made_progress = true;
                } else {
                    next_pending.push(decl);
                }
            }

            if !made_progress {
                // Saturated / cannot resolve further; final evaluation accepts bottom errors
                for &decl in &pending_decls {
                    self.eval_single_decl(decl, target_struct)?;
                }
                break;
            }

            pending_decls = next_pending;
        }

        Ok(())
    }

    fn is_unresolved(&self, val_id: ValueId) -> bool {
        if let Some(Value::Bottom(reason)) = self.arena.get(val_id) {
            reason.message.contains("unresolved reference")
        } else {
            false
        }
    }

    fn eval_single_decl(
        &mut self,
        decl: &Decl,
        target_struct: &mut StructValue,
    ) -> Result<bool, EvalError> {
        match decl {
            Decl::Field(f) => match &f.label {
                Label::Pattern(pattern_expr) => {
                    let pattern_val = self.eval_expr(pattern_expr)?;
                    let target_val = self.eval_expr(&f.value)?;
                    target_struct.add_pattern_constraint(pattern_val, target_val);
                    Ok(!self.is_unresolved(target_val))
                }
                Label::Dynamic(dyn_expr) => {
                    let label_val_id = self.eval_expr(dyn_expr)?;
                    if let Some(Value::String(name)) = self.arena.get(label_val_id) {
                        let name = name.clone();
                        let val_id = self.eval_expr(&f.value)?;
                        let is_unresolved = self.is_unresolved(val_id);
                        target_struct.insert_field(name.clone(), val_id, f.optional);
                        self.insert_binding(&name, val_id);
                        Ok(!is_unresolved)
                    } else if self.is_unresolved(label_val_id) {
                        Ok(false)
                    } else {
                        Ok(true)
                    }
                }
                _ => {
                    let val_id = self.eval_expr(&f.value)?;
                    let is_unresolved = self.is_unresolved(val_id);
                    if let Some(name) = f.label.name() {
                        if f.label.is_definition() {
                            if let Some(&placeholder_id) = self.placeholders.get(name)
                                && let Some(Value::RecursiveRef { target, .. }) =
                                    self.arena.get_mut(placeholder_id)
                            {
                                *target = Some(val_id);
                            }
                            target_struct.insert_def(name.to_string(), val_id, f.optional);
                            self.insert_binding(name, val_id);
                        } else if f.label.is_hidden() {
                            target_struct.insert_hidden(name.to_string(), val_id, f.optional);
                            self.insert_binding(name, val_id);
                        } else {
                            target_struct.insert_field(name.to_string(), val_id, f.optional);
                            self.insert_binding(name, val_id);
                        }
                    }
                    Ok(!is_unresolved)
                }
            },
            Decl::Alias { ident, expr } => {
                let val_id = self.eval_expr(expr)?;
                let is_unresolved = self.is_unresolved(val_id);
                self.insert_binding(ident, val_id);
                Ok(!is_unresolved)
            }
            Decl::Let { ident, expr } => {
                let val_id = self.eval_expr(expr)?;
                let is_unresolved = self.is_unresolved(val_id);
                self.insert_binding(ident, val_id);
                Ok(!is_unresolved)
            }
            Decl::Embedding(expr) => {
                let embedded_id = self.eval_expr(expr)?;
                let is_unresolved = self.is_unresolved(embedded_id);
                let current_id = self.arena.alloc(Value::Struct(target_struct.clone()));
                let unified_id = unify(&mut self.arena, current_id, embedded_id);
                if let Some(Value::Struct(s)) = self.arena.get(unified_id) {
                    *target_struct = s.clone();
                }
                Ok(!is_unresolved)
            }
            Decl::Comprehension(comp) => {
                self.eval_comprehension(comp, target_struct)?;
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    fn eval_comprehension(
        &mut self,
        comp: &ComprehensionDecl,
        target_struct: &mut StructValue,
    ) -> Result<(), EvalError> {
        if comp.clauses.is_empty() {
            return Ok(());
        }

        self.eval_comprehension_clause(0, comp, target_struct)
    }

    fn eval_comprehension_clause(
        &mut self,
        clause_idx: usize,
        comp: &ComprehensionDecl,
        target_struct: &mut StructValue,
    ) -> Result<(), EvalError> {
        if clause_idx >= comp.clauses.len() {
            // Reached the body: evaluate decls in struct_lit
            self.eval_decls_into_struct(&comp.struct_lit.decls, target_struct)?;
            return Ok(());
        }

        match &comp.clauses[clause_idx] {
            ComprehensionClause::If { condition } => {
                let cond_val = self.eval_expr(condition)?;
                if let Some(Value::Bool(true)) = self.arena.get(cond_val) {
                    self.eval_comprehension_clause(clause_idx + 1, comp, target_struct)?;
                }
            }
            ComprehensionClause::Let { ident, expr } => {
                let val_id = self.eval_expr(expr)?;
                self.push_scope();
                self.insert_binding(ident, val_id);
                self.eval_comprehension_clause(clause_idx + 1, comp, target_struct)?;
                self.pop_scope();
            }
            ComprehensionClause::For { key, value, source } => {
                let src_id = self.eval_expr(source)?;
                let src_val = match self.arena.get(src_id) {
                    Some(v) => v.clone(),
                    None => return Ok(()),
                };

                match src_val {
                    Value::List { elements, .. } => {
                        for (idx, &elem_id) in elements.iter().enumerate() {
                            self.push_scope();
                            self.insert_binding(value, elem_id);
                            if let Some(k_name) = key {
                                let k_id = self.arena.int(idx as i64);
                                self.insert_binding(k_name, k_id);
                            }
                            self.eval_comprehension_clause(clause_idx + 1, comp, target_struct)?;
                            self.pop_scope();
                        }
                    }
                    Value::Struct(s) => {
                        for (k, entry) in &s.fields {
                            self.push_scope();
                            self.insert_binding(value, entry.val);
                            if let Some(k_name) = key {
                                let k_id = self.arena.string(k.clone());
                                self.insert_binding(k_name, k_id);
                            }
                            self.eval_comprehension_clause(clause_idx + 1, comp, target_struct)?;
                            self.pop_scope();
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn eval_list_comprehension_clause(
        &mut self,
        clause_idx: usize,
        comp: &cue_syntax::ast::ListComprehension,
        elements: &mut Vec<ValueId>,
    ) -> Result<(), EvalError> {
        if clause_idx >= comp.clauses.len() {
            let val_id = self.eval_expr(&comp.expr)?;
            elements.push(val_id);
            return Ok(());
        }

        match &comp.clauses[clause_idx] {
            ComprehensionClause::If { condition } => {
                let cond_val = self.eval_expr(condition)?;
                if let Some(Value::Bool(true)) = self.arena.get(cond_val) {
                    self.eval_list_comprehension_clause(clause_idx + 1, comp, elements)?;
                }
            }
            ComprehensionClause::Let { ident, expr } => {
                let val_id = self.eval_expr(expr)?;
                self.push_scope();
                self.insert_binding(ident, val_id);
                self.eval_list_comprehension_clause(clause_idx + 1, comp, elements)?;
                self.pop_scope();
            }
            ComprehensionClause::For { key, value, source } => {
                let src_id = self.eval_expr(source)?;
                let src_val = match self.arena.get(src_id) {
                    Some(v) => v.clone(),
                    None => return Ok(()),
                };

                match src_val {
                    Value::List {
                        elements: src_elems,
                        ..
                    } => {
                        for (idx, &elem_id) in src_elems.iter().enumerate() {
                            self.push_scope();
                            self.insert_binding(value, elem_id);
                            if let Some(k_name) = key {
                                let k_id = self.arena.int(idx as i64);
                                self.insert_binding(k_name, k_id);
                            }
                            self.eval_list_comprehension_clause(clause_idx + 1, comp, elements)?;
                            self.pop_scope();
                        }
                    }
                    Value::Struct(s) => {
                        for (k, entry) in &s.fields {
                            self.push_scope();
                            self.insert_binding(value, entry.val);
                            if let Some(k_name) = key {
                                let k_id = self.arena.string(k.clone());
                                self.insert_binding(k_name, k_id);
                            }
                            self.eval_list_comprehension_clause(clause_idx + 1, comp, elements)?;
                            self.pop_scope();
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Evaluate an AST Expression.
    pub fn eval_expr(&mut self, expr: &Expr) -> Result<ValueId, EvalError> {
        match expr {
            Expr::Bottom => Ok(self.arena.bottom("explicit bottom")),
            Expr::Top => Ok(self.arena.top()),
            Expr::Null => Ok(self.arena.null()),
            Expr::Bool(b) => Ok(self.arena.bool(*b)),
            Expr::Number(n) => self.eval_number(n),
            Expr::String(s) => Ok(self.arena.string(s.clone())),
            Expr::Bytes(b) => Ok(self.arena.alloc(Value::Bytes(b.as_bytes().to_vec()))),
            Expr::Ident(id)
            | Expr::DefIdent(id)
            | Expr::HiddenIdent(id)
            | Expr::HiddenDefIdent(id) => {
                if self.resolving_symbols.contains(id) {
                    if id.starts_with('#') || id.starts_with("_#") {
                        return Ok(self.arena.alloc(Value::RecursiveRef {
                            name: id.clone(),
                            target: self.lookup_binding(id),
                        }));
                    } else {
                        return Ok(self.arena.bottom(format!(
                            "cycle error: cyclic value dependency detected on '{id}'"
                        )));
                    }
                }

                if let Some(val) = self.lookup_binding(id) {
                    Ok(val)
                } else {
                    Ok(self.arena.bottom(format!("unresolved reference '{id}'")))
                }
            }
            Expr::Struct(s) => {
                let mut struct_val = StructValue::new(false);
                self.push_scope();
                self.eval_decls_into_struct(&s.decls, &mut struct_val)?;
                self.pop_scope();
                Ok(self.arena.alloc(Value::Struct(struct_val)))
            }
            Expr::List(l) => {
                let mut elements = Vec::new();
                for elem in &l.elements {
                    elements.push(self.eval_expr(elem)?);
                }
                let ellipsis = if let Some(el) = &l.ellipsis {
                    Some(self.eval_expr(el)?)
                } else {
                    None
                };
                Ok(self.arena.alloc(Value::List { elements, ellipsis }))
            }
            Expr::Binary { op, left, right } => {
                let left_id = self.eval_expr(left)?;
                let right_id = self.eval_expr(right)?;

                match op {
                    BinaryOp::Unify => Ok(unify(&mut self.arena, left_id, right_id)),
                    BinaryOp::Disjoin => {
                        let branches = vec![
                            ValueBranch {
                                default: false,
                                val: left_id,
                            },
                            ValueBranch {
                                default: false,
                                val: right_id,
                            },
                        ];
                        Ok(self.arena.alloc(Value::Disjunction { branches }))
                    }
                    _ => Ok(self.eval_binary_arithmetic(*op, left_id, right_id)),
                }
            }
            Expr::Unary { op, expr } => {
                let target_id = self.eval_expr(expr)?;
                match op {
                    UnaryOp::Neg => match self.arena.get(target_id) {
                        Some(Value::Int(i)) => Ok(self.arena.int(-i)),
                        Some(Value::Float(f)) => Ok(self.arena.float(-f)),
                        _ => Ok(self.arena.bottom("cannot negate non-numeric value")),
                    },
                    UnaryOp::Pos => Ok(target_id),
                    UnaryOp::Not => match self.arena.get(target_id) {
                        Some(Value::Bool(b)) => Ok(self.arena.bool(!b)),
                        _ => Ok(self.arena.bottom("cannot apply '!' to non-boolean value")),
                    },
                    UnaryOp::Less => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::Less, target_id)],
                    })),
                    UnaryOp::LessEqual => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::LessEqual, target_id)],
                    })),
                    UnaryOp::Greater => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::Greater, target_id)],
                    })),
                    UnaryOp::GreaterEqual => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::GreaterEqual, target_id)],
                    })),
                    UnaryOp::NotEqual => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::NotEqual, target_id)],
                    })),
                    UnaryOp::RegexMatch => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::RegexMatch, target_id)],
                    })),
                    UnaryOp::RegexNotMatch => Ok(self.arena.alloc(Value::Bounds {
                        base_type: None,
                        constraints: vec![(BoundOp::RegexNotMatch, target_id)],
                    })),
                    _ => Ok(target_id),
                }
            }
            Expr::Disjunction { branches } => {
                let mut eval_branches = Vec::new();
                for b in branches {
                    let val_id = self.eval_expr(&b.expr)?;
                    eval_branches.push(ValueBranch {
                        default: b.default,
                        val: val_id,
                    });
                }
                Ok(self.arena.alloc(Value::Disjunction {
                    branches: eval_branches,
                }))
            }
            Expr::Selector { expr, field } => {
                if let Expr::Ident(pkg_name) = expr.as_ref() {
                    let canonical_pkg = self
                        .import_aliases
                        .get(pkg_name)
                        .map(|s| s.as_str())
                        .unwrap_or(pkg_name.as_str());
                    if let Some(&pkg_struct_id) = self.imported_packages.get(canonical_pkg)
                        && let Some(Value::Struct(s)) = self.arena.get(pkg_struct_id)
                            && let Some(f) = s.fields.get(field).or_else(|| s.definitions.get(field)) {
                                return Ok(f.val);
                            }
                    if let Ok(res) = crate::stdlib::call_stdlib_func(&mut self.arena, canonical_pkg, field, &[]) {
                        return Ok(res);
                    }
                }
                let val_id = self.eval_expr(expr)?;
                if let Some(Value::Bottom(_)) = self.arena.get(val_id) {
                    return Ok(val_id);
                }
                if let Some(Value::Struct(s)) = self.arena.get(val_id) {
                    if let Some(f) = s.fields.get(field).or_else(|| s.definitions.get(field)) {
                        Ok(f.val)
                    } else {
                        Ok(self.arena.bottom(format!("unresolved reference '{field}'")))
                    }
                } else {
                    Ok(self.arena.bottom("selector on non-struct"))
                }
            }
            Expr::Index { expr, index } => {
                let target_id = self.eval_expr(expr)?;
                if let Some(Value::Bottom(_)) = self.arena.get(target_id) {
                    return Ok(target_id);
                }
                let index_id = self.eval_expr(index)?;
                if let Some(Value::Bottom(_)) = self.arena.get(index_id) {
                    return Ok(index_id);
                }

                match (self.arena.get(target_id), self.arena.get(index_id)) {
                    (Some(Value::List { elements, .. }), Some(Value::Int(i))) => {
                        if let Some(idx) = i.to_usize() {
                            if let Some(&elem) = elements.get(idx) {
                                Ok(elem)
                            } else {
                                Ok(self.arena.bottom(format!(
                                    "list index {idx} out of bounds (len: {})",
                                    elements.len()
                                )))
                            }
                        } else {
                            Ok(self.arena.bottom("invalid list index"))
                        }
                    }
                    (Some(Value::Struct(s)), Some(Value::String(key))) => {
                        if let Some(f) = s.fields.get(key).or_else(|| s.definitions.get(key)) {
                            Ok(f.val)
                        } else {
                            Ok(self.arena.bottom(format!("unresolved reference '{key}'")))
                        }
                    }
                    _ => Ok(self.arena.bottom("indexing unsupported on target")),
                }
            }
            Expr::Slice { expr, low, high } => {
                let target_id = self.eval_expr(expr)?;
                if let Some(Value::List { elements, ellipsis }) = self.arena.get(target_id).cloned() {
                    let len = elements.len();
                    let start = if let Some(l) = low {
                        let l_id = self.eval_expr(l)?;
                        if let Some(Value::Int(i)) = self.arena.get(l_id) {
                            i.to_usize().unwrap_or(0).min(len)
                        } else {
                            0
                        }
                    } else {
                        0
                    };

                    let end = if let Some(h) = high {
                        let h_id = self.eval_expr(h)?;
                        if let Some(Value::Int(i)) = self.arena.get(h_id) {
                            i.to_usize().unwrap_or(len).min(len)
                        } else {
                            len
                        }
                    } else {
                        len
                    };

                    if start <= end {
                        let sliced = elements[start..end].to_vec();
                        Ok(self.arena.alloc(Value::List {
                            elements: sliced,
                            ellipsis,
                        }))
                    } else {
                        Ok(self.arena.bottom(format!(
                            "invalid slice range: [{start}:{end}]"
                        )))
                    }
                } else {
                    Ok(self.arena.bottom("slice unsupported on non-list"))
                }
            }
            Expr::Call { func, args } => self.eval_call(func, args),
            Expr::Interpolation { parts } => {
                let mut result_str = String::new();
                for part in parts {
                    match part {
                        InterpolationPart::Lit(s) => result_str.push_str(s),
                        InterpolationPart::Expr(e) => {
                            let val_id = self.eval_expr(e)?;
                            match self.arena.get(val_id) {
                                Some(Value::String(s)) => result_str.push_str(s),
                                Some(Value::Int(i)) => result_str.push_str(&i.to_string()),
                                Some(Value::Float(f)) => result_str.push_str(&f.to_string()),
                                Some(Value::Bool(b)) => result_str.push_str(&b.to_string()),
                                Some(Value::Bottom(_)) => return Ok(val_id),
                                _ => {
                                    return Ok(self.arena.bottom(
                                        "string interpolation requires concrete scalar value",
                                    ))
                                }
                            }
                        }
                    }
                }
                Ok(self.arena.string(result_str))
            }
            Expr::ListComp(comp) => {
                let mut elements = Vec::new();
                self.eval_list_comprehension_clause(0, comp, &mut elements)?;
                Ok(self.arena.alloc(Value::List {
                    elements,
                    ellipsis: None,
                }))
            }
        }
    }

    fn eval_call(&mut self, func_expr: &Expr, arg_exprs: &[Expr]) -> Result<ValueId, EvalError> {
        // Builtin functions dispatch: strings.*, math.*, list.*, regexp.*
        if let Expr::Selector { expr, field } = func_expr
            && let Expr::Ident(pkg) = &**expr {
                let canonical_pkg = self
                    .import_aliases
                    .get(pkg)
                    .cloned()
                    .unwrap_or_else(|| pkg.clone());
                let mut evaluated_args = Vec::new();
                for a in arg_exprs {
                    evaluated_args.push(self.eval_expr(a)?);
                }

                match crate::stdlib::call_stdlib_func(
                    &mut self.arena,
                    &canonical_pkg,
                    field,
                    &evaluated_args,
                ) {
                    Ok(res_id) => return Ok(res_id),
                    Err(err) => return Ok(self.arena.bottom(err)),
                }
            }

        Ok(self.arena.bottom("unsupported function call"))
    }

    fn eval_number(&mut self, n_str: &str) -> Result<ValueId, EvalError> {
        let cleaned = n_str.replace('_', "");

        // Hex, binary, octal
        if (cleaned.starts_with("0x") || cleaned.starts_with("0X"))
            && let Ok(i) = i64::from_str_radix(&cleaned[2..], 16)
        {
            return Ok(self.arena.int(i));
        } else if (cleaned.starts_with("0b") || cleaned.starts_with("0B"))
            && let Ok(i) = i64::from_str_radix(&cleaned[2..], 2)
        {
            return Ok(self.arena.int(i));
        } else if (cleaned.starts_with("0o") || cleaned.starts_with("0O"))
            && let Ok(i) = i64::from_str_radix(&cleaned[2..], 8)
        {
            return Ok(self.arena.int(i));
        }

        // SI multipliers
        let multiplier: Option<i64> = if cleaned.ends_with("Ki") {
            Some(1024)
        } else if cleaned.ends_with("Mi") {
            Some(1024 * 1024)
        } else if cleaned.ends_with("Gi") {
            Some(1024 * 1024 * 1024)
        } else if cleaned.ends_with("Ti") {
            Some(1024 * 1024 * 1024 * 1024)
        } else if cleaned.ends_with("Pi") {
            Some(1024 * 1024 * 1024 * 1024 * 1024)
        } else if cleaned.ends_with('k') || cleaned.ends_with('K') {
            Some(1000)
        } else if cleaned.ends_with('M') {
            Some(1_000_000)
        } else if cleaned.ends_with('G') {
            Some(1_000_000_000)
        } else if cleaned.ends_with('T') {
            Some(1_000_000_000_000)
        } else if cleaned.ends_with('P') {
            Some(1_000_000_000_000_000)
        } else {
            None
        };

        if let Some(mult) = multiplier {
            let num_part = if cleaned.ends_with("Ki")
                || cleaned.ends_with("Mi")
                || cleaned.ends_with("Gi")
                || cleaned.ends_with("Ti")
                || cleaned.ends_with("Pi")
            {
                &cleaned[..cleaned.len() - 2]
            } else {
                &cleaned[..cleaned.len() - 1]
            };
            if let Ok(base) = num_part.parse::<i64>() {
                return Ok(self.arena.int(base * mult));
            }
        }

        if (cleaned.contains('.') || cleaned.contains('e') || cleaned.contains('E'))
            && let Ok(f) = cleaned.parse::<f64>() {
                return Ok(self.arena.float(f));
            }
        if let Ok(i) = BigInt::from_str(&cleaned) {
            return Ok(self.arena.int(i));
        }
        Ok(self.arena.bottom(format!("invalid number '{n_str}'")))
    }

    fn eval_binary_arithmetic(&mut self, op: BinaryOp, left: ValueId, right: ValueId) -> ValueId {
        let l_val = match self.arena.get(left) {
            Some(Value::Bottom(_)) => return left,
            Some(v) => v.clone(),
            None => return self.arena.bottom("invalid left operand"),
        };
        let r_val = match self.arena.get(right) {
            Some(Value::Bottom(_)) => return right,
            Some(v) => v.clone(),
            None => return self.arena.bottom("invalid right operand"),
        };

        match (op, l_val, r_val) {
            // Arithmetic
            (BinaryOp::Add, Value::Int(a), Value::Int(b)) => self.arena.int(a + b),
            (BinaryOp::Sub, Value::Int(a), Value::Int(b)) => self.arena.int(a - b),
            (BinaryOp::Mul, Value::Int(a), Value::Int(b)) => self.arena.int(a * b),
            (BinaryOp::Div, Value::Int(a), Value::Int(b)) => {
                if b != 0.into() {
                    self.arena.int(a / b)
                } else {
                    self.arena.bottom("division by zero")
                }
            }
            (BinaryOp::Add, Value::Float(a), Value::Float(b)) => self.arena.float(a + b),
            (BinaryOp::Sub, Value::Float(a), Value::Float(b)) => self.arena.float(a - b),
            (BinaryOp::Mul, Value::Float(a), Value::Float(b)) => self.arena.float(a * b),
            (BinaryOp::Div, Value::Float(a), Value::Float(b)) => self.arena.float(a / b),

            // Mixed Int & Float
            (BinaryOp::Add, Value::Int(a), Value::Float(b)) => {
                self.arena.float(a.to_f64().unwrap_or(0.0) + b)
            }
            (BinaryOp::Add, Value::Float(a), Value::Int(b)) => {
                self.arena.float(a + b.to_f64().unwrap_or(0.0))
            }
            (BinaryOp::Sub, Value::Int(a), Value::Float(b)) => {
                self.arena.float(a.to_f64().unwrap_or(0.0) - b)
            }
            (BinaryOp::Sub, Value::Float(a), Value::Int(b)) => {
                self.arena.float(a - b.to_f64().unwrap_or(0.0))
            }
            (BinaryOp::Mul, Value::Int(a), Value::Float(b)) => {
                self.arena.float(a.to_f64().unwrap_or(0.0) * b)
            }
            (BinaryOp::Mul, Value::Float(a), Value::Int(b)) => {
                self.arena.float(a * b.to_f64().unwrap_or(0.0))
            }
            (BinaryOp::Div, Value::Int(a), Value::Float(b)) => {
                self.arena.float(a.to_f64().unwrap_or(0.0) / b)
            }
            (BinaryOp::Div, Value::Float(a), Value::Int(b)) => {
                self.arena.float(a / b.to_f64().unwrap_or(1.0))
            }

            (BinaryOp::Add, Value::String(a), Value::String(b)) => {
                self.arena.string(format!("{a}{b}"))
            }
            (BinaryOp::Mul, Value::String(a), Value::Int(b)) => {
                if let Some(count) = b.to_usize() {
                    self.arena.string(a.repeat(count))
                } else {
                    self.arena.bottom("invalid string repetition factor")
                }
            }
            (BinaryOp::Mul, Value::Int(a), Value::String(b)) => {
                if let Some(count) = a.to_usize() {
                    self.arena.string(b.repeat(count))
                } else {
                    self.arena.bottom("invalid string repetition factor")
                }
            }

            // List Concatenation and Repetition
            (
                BinaryOp::Add,
                Value::List {
                    elements: mut e1,
                    ellipsis: _,
                },
                Value::List {
                    elements: e2,
                    ellipsis,
                },
            ) => {
                e1.extend(e2);
                self.arena.alloc(Value::List {
                    elements: e1,
                    ellipsis,
                })
            }
            (
                BinaryOp::Mul,
                Value::List {
                    elements: e1,
                    ellipsis,
                },
                Value::Int(b),
            ) => {
                if let Some(count) = b.to_usize() {
                    let mut repeated = Vec::new();
                    for _ in 0..count {
                        repeated.extend(e1.clone());
                    }
                    self.arena.alloc(Value::List {
                        elements: repeated,
                        ellipsis,
                    })
                } else {
                    self.arena.bottom("invalid list repetition factor")
                }
            }

            // Comparisons
            (BinaryOp::Equal, Value::Int(a), Value::Int(b)) => self.arena.bool(a == b),
            (BinaryOp::NotEqual, Value::Int(a), Value::Int(b)) => self.arena.bool(a != b),
            (BinaryOp::Less, Value::Int(a), Value::Int(b)) => self.arena.bool(a < b),
            (BinaryOp::LessEqual, Value::Int(a), Value::Int(b)) => self.arena.bool(a <= b),
            (BinaryOp::Greater, Value::Int(a), Value::Int(b)) => self.arena.bool(a > b),
            (BinaryOp::GreaterEqual, Value::Int(a), Value::Int(b)) => self.arena.bool(a >= b),

            (BinaryOp::Equal, Value::Float(a), Value::Float(b)) => {
                self.arena.bool((a - b).abs() < f64::EPSILON)
            }
            (BinaryOp::NotEqual, Value::Float(a), Value::Float(b)) => {
                self.arena.bool((a - b).abs() >= f64::EPSILON)
            }
            (BinaryOp::Less, Value::Float(a), Value::Float(b)) => self.arena.bool(a < b),
            (BinaryOp::LessEqual, Value::Float(a), Value::Float(b)) => self.arena.bool(a <= b),
            (BinaryOp::Greater, Value::Float(a), Value::Float(b)) => self.arena.bool(a > b),
            (BinaryOp::GreaterEqual, Value::Float(a), Value::Float(b)) => self.arena.bool(a >= b),

            (BinaryOp::Equal, Value::String(a), Value::String(b)) => self.arena.bool(a == b),
            (BinaryOp::NotEqual, Value::String(a), Value::String(b)) => self.arena.bool(a != b),
            (BinaryOp::Equal, Value::Bool(a), Value::Bool(b)) => self.arena.bool(a == b),
            (BinaryOp::NotEqual, Value::Bool(a), Value::Bool(b)) => self.arena.bool(a != b),

            _ => self.arena.bottom("unsupported binary operation"),
        }
    }

    /// Export evaluated value to JSON if concrete.
    pub fn to_json(&self, val_id: ValueId) -> Result<serde_json::Value, String> {
        match self.arena.get(val_id) {
            Some(Value::Null) => Ok(serde_json::Value::Null),
            Some(Value::Bool(b)) => Ok(serde_json::Value::Bool(*b)),
            Some(Value::Int(i)) => {
                if let Some(n) = i.to_i64() {
                    Ok(serde_json::json!(n))
                } else {
                    Ok(serde_json::Value::String(i.to_string()))
                }
            }
            Some(Value::Float(f)) => Ok(serde_json::json!(f)),
            Some(Value::String(s)) => Ok(serde_json::Value::String(s.clone())),
            Some(Value::List { elements, .. }) => {
                let mut arr = Vec::new();
                for &elem in elements {
                    arr.push(self.to_json(elem)?);
                }
                Ok(serde_json::Value::Array(arr))
            }
            Some(Value::Struct(s)) => {
                let mut map = serde_json::Map::new();
                for (k, entry) in &s.fields {
                    if entry.optional {
                        if let Ok(v) = self.to_json(entry.val) {
                            map.insert(k.clone(), v);
                        }
                    } else {
                        map.insert(k.clone(), self.to_json(entry.val)?);
                    }
                }
                Ok(serde_json::Value::Object(map))
            }
            Some(Value::Disjunction { branches }) => {
                if let Some(default_branch) = branches.iter().find(|b| b.default) {
                    self.to_json(default_branch.val)
                } else if branches.len() == 1 {
                    self.to_json(branches[0].val)
                } else {
                    Err("cannot export non-concrete disjunction to JSON".to_string())
                }
            }
            Some(Value::Bottom(b)) => Err(format!("cannot export bottom: {}", b)),
            Some(Value::Top) => Err("cannot export non-concrete top value to JSON".to_string()),
            Some(Value::Type(t)) => Err(format!("cannot export type {t} to JSON")),
            Some(Value::Bounds { .. }) => Err("cannot export bound constraint to JSON".to_string()),
            Some(Value::BuiltinValidator { .. }) => {
                Err("cannot export validator constraint to JSON".to_string())
            }
            Some(Value::Validators(_)) => {
                Err("cannot export validator constraints to JSON".to_string())
            }
            Some(Value::RecursiveRef { name, .. }) => {
                Ok(serde_json::Value::String(format!("<ref:{name}>")))
            }
            Some(Value::Bytes(b)) => {
                Ok(serde_json::Value::String(String::from_utf8_lossy(b).to_string()))
            }
            None => Err("invalid value id".to_string()),
        }
    }
}
