//! Closing a definition's value.
//!
//! A definition is closed recursively: referencing `#A` yields a value that
//! rejects any regular field `#A` does not declare, at every depth of struct
//! literal it holds. The definition as stored stays open, so its own body can
//! embed other values and name itself while it is being built; closing happens
//! where it is read.
//!
//! What is not closed: a struct written with `...`, whose nested structs still
//! are; a value reached through a recursive reference, which is closed when that
//! reference is itself read; hidden fields and definitions, which closedness
//! never constrains.

use crate::value::{
    Conjunct, DeclRecipe, DeclSource, DisjunctionBranch, MetadataSource, PatternConstraint,
    Provisional, StructValue, Thunk, Value, ValueArena, ValueId,
};
use cue_syntax::ast::{BinaryOp, Decl, Expr, Label};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Closure {
    Recursive,
    Outer,
    Contents,
}

impl Closure {
    pub(crate) fn apply(self, arena: &mut ValueArena, value: ValueId) -> ValueId {
        match self {
            Self::Recursive => ClosedCopies::default().close(arena, value),
            Self::Outer => reclose(arena, value),
            Self::Contents => {
                let closed = ClosedCopies::default().close(arena, value);
                open_for_embedding(arena, closed).0
            }
        }
    }
}

/// Closed copies already made, keyed by the value they close.
///
/// A definition is read at every use, so its closed copy is shared. A key whose
/// node a disjunction rollback removed no longer resolves in the arena and is
/// made again.
#[derive(Debug, Default)]
pub struct ClosedCopies {
    copies: HashMap<ValueId, ValueId>,
}

/// What reading `name` yields: a definition's closed copy, anything else as
/// it is. Embeddings at the file root read definitions open.
pub(crate) fn read_definition(
    arena: &mut ValueArena,
    closed: &mut ClosedCopies,
    reading_root_embedding: bool,
    name: &str,
    val: ValueId,
) -> ValueId {
    if !reading_root_embedding && (name.starts_with('#') || name.starts_with("_#")) {
        closed.close(arena, val)
    } else {
        val
    }
}

impl ClosedCopies {
    pub fn close(&mut self, arena: &mut ValueArena, id: ValueId) -> ValueId {
        if let Some(&copy) = self.copies.get(&id)
            && arena.get(copy).is_some()
            && arena.get(id).is_some()
        {
            return copy;
        }
        let mut in_progress = HashMap::new();
        let copy = close_deep(arena, id, &mut in_progress);
        self.copies.insert(id, copy);
        copy
    }
}

/// `id` closed recursively, or `id` itself when nothing in it needed closing.
fn close_deep(
    arena: &mut ValueArena,
    id: ValueId,
    in_progress: &mut HashMap<ValueId, ValueId>,
) -> ValueId {
    if let Some(&copy) = in_progress.get(&id) {
        return copy;
    }
    // A cyclic graph reaches a node again before its copy exists; answering
    // with the node itself ends the walk there.
    in_progress.insert(id, id);
    if let Some(mut metadata) = arena.metadata(id).cloned() {
        if arena.has_embedded_recipe(id) {
            if matches!(
                metadata.source,
                MetadataSource::Closed {
                    closure: Closure::Recursive,
                    ..
                }
            ) {
                return id;
            }
            let conjuncts = match metadata.source {
                MetadataSource::Embedded(conjuncts) => conjuncts,
                _ => vec![Conjunct::Value(id)],
            };
            let payload = crate::metadata::payload(arena, id);
            let closed = close_deep(arena, payload, in_progress);
            metadata.view = Some(close_deep(
                arena,
                metadata.view.unwrap_or(metadata.fields),
                in_progress,
            ));
            metadata.source = MetadataSource::Closed {
                conjuncts,
                closure: Closure::Recursive,
            };
            let value = arena.get(closed).expect("closed payload exists").clone();
            let result = arena.alloc_with_metadata(value, metadata);
            in_progress.insert(id, result);
            return result;
        }
        let payload = crate::metadata::payload(arena, id);
        let closed = close_deep(arena, payload, in_progress);
        let fields = close_deep(arena, metadata.fields, in_progress);
        if closed == payload && fields == metadata.fields {
            return id;
        }
        metadata.fields = fields;
        let value = arena.get(closed).expect("closed payload exists").clone();
        let result = arena.alloc_with_metadata(value, metadata);
        in_progress.insert(id, result);
        return result;
    }
    let closed = match arena.get(id).cloned() {
        Some(Value::Struct(mut s)) => {
            // A marker survives only outside merges: a literal embedding a
            // closed value and then naming `...` (`{Old1, ...}`) still
            // mixes, while a merge already consumed its marker. Spread
            // permission outlives the marker but not a definition read of
            // a marker-less merge. All three shapes are oracle-verified.
            let mut changed = s.is_closed != !s.is_open;
            s.is_closed = !s.is_open;
            if s.is_closed && !s.is_open && s.spread_open {
                // Reading a definition closes over spread permission: with
                // `#S: #Def... & {b: 2}` a later `& {c}` is rejected, while
                // the bare `#S: #Def...` still shows the marker and stays
                // mixable. Both shapes are oracle-verified.
                s.spread_open = false;
                changed = true;
            }
            // Its declarations generate the definition's fields when a merge
            // runs them again.
            for recipe in &mut s.recipes {
                changed |= !recipe.closes;
                recipe.closes = true;
            }
            for entry in s.fields.values_mut() {
                let val = close_deep(arena, entry.val, in_progress);
                changed |= val != entry.val;
                entry.val = val;
                // A merge derives the field again from these, and what it
                // derives is the definition's too: `#P & {a: 1} & {n: d: 3}`
                // re-derives `n` for `a` and must still refuse `d`.
                if let Some(conjuncts) = definition_conjuncts(&entry.conjuncts) {
                    entry.conjuncts = conjuncts;
                    changed = true;
                }
            }
            let constraints = std::mem::take(&mut s.pattern_constraints);
            s.pattern_constraints = constraints
                .into_iter()
                .map(|pc| {
                    let target_val = close_deep(arena, pc.target_val, in_progress);
                    changed |= target_val != pc.target_val;
                    PatternConstraint { target_val, ..pc }
                })
                .collect();
            if changed {
                arena.alloc(Value::Struct(s))
            } else {
                id
            }
        }
        Some(Value::Disjunction { branches }) => {
            let closed: Vec<DisjunctionBranch> = branches
                .iter()
                .map(|b| DisjunctionBranch {
                    default: b.default,
                    val: close_deep(arena, b.val, in_progress),
                })
                .collect();
            if closed == branches {
                id
            } else {
                arena.alloc(Value::Disjunction { branches: closed })
            }
        }
        Some(Value::List { elements, ellipsis }) => {
            let closed: Vec<ValueId> = elements
                .iter()
                .map(|&e| close_deep(arena, e, in_progress))
                .collect();
            let closed_ellipsis = ellipsis.map(|e| close_deep(arena, e, in_progress));
            if closed == elements && closed_ellipsis == ellipsis {
                id
            } else {
                arena.alloc(Value::List {
                    elements: closed,
                    ellipsis: closed_ellipsis,
                })
            }
        }
        _ => id,
    };
    in_progress.insert(id, closed);
    closed
}

/// A closed field's recipes, each marked as its definition's; `None` when
/// every one already is.
pub(crate) fn definition_conjuncts(conjuncts: &Rc<[Conjunct]>) -> Option<Rc<[Conjunct]>> {
    let mut changed = false;
    let closed: Rc<[Conjunct]> = conjuncts
        .iter()
        .map(|conjunct| match conjunct {
            Conjunct::Value(id) => {
                changed = true;
                Conjunct::Closed(*id)
            }
            Conjunct::Closed(id) => Conjunct::Closed(*id),
            Conjunct::Thunk(thunk) => {
                changed |= !thunk.closes;
                Conjunct::Thunk(Thunk {
                    closes: true,
                    ..thunk.clone()
                })
            }
        })
        .collect();
    changed.then_some(closed)
}

/// Open the outermost struct of an embedded value for the merge into the
/// literal embedding it, reporting whether it was closed.
///
/// Embedding a closed value closes the literal, but the literal's own fields
/// are allowed: `#B: {#A, y: int}` declares `y`. So the literal meets an open
/// copy and is closed afterwards with [`reclose`].
pub fn open_for_embedding(arena: &mut ValueArena, id: ValueId) -> (ValueId, bool) {
    if let Some(mut metadata) = arena.metadata(id).cloned() {
        if arena.has_embedded_recipe(id) {
            let was_closed = match &mut metadata.source {
                MetadataSource::Closed { closure, .. } => {
                    let was_closed = *closure != Closure::Contents;
                    *closure = Closure::Contents;
                    was_closed
                }
                _ => false,
            };
            let payload = crate::metadata::payload(arena, id);
            let (opened, closed) = open_for_embedding(arena, payload);
            let (view, view_closed) =
                open_for_embedding(arena, metadata.view.unwrap_or(metadata.fields));
            metadata.view = Some(view);
            if !was_closed && !closed && !view_closed {
                return (id, false);
            }
            let value = arena.get(opened).expect("opened payload exists").clone();
            return (
                arena.alloc_with_metadata(value, metadata),
                was_closed || closed || view_closed,
            );
        }
        let (fields, closed) = open_for_embedding(arena, metadata.fields);
        let (payload, branch_closed) = if metadata.is_choice_view() {
            let payload = crate::metadata::payload(arena, id);
            open_for_embedding(arena, payload)
        } else {
            (id, false)
        };
        if fields == metadata.fields && !branch_closed {
            return (id, closed);
        }
        metadata.fields = fields;
        let value = arena.get(payload).expect("metadata owner exists").clone();
        return (
            arena.alloc_with_metadata(value, metadata),
            closed || branch_closed,
        );
    }
    match arena.get(id).cloned() {
        Some(Value::Struct(mut s)) if s.is_closed => {
            s.is_closed = false;
            (arena.alloc(Value::Struct(s)), true)
        }
        Some(Value::Disjunction { branches }) => {
            let mut any_closed = false;
            let opened = branches
                .into_iter()
                .map(|b| {
                    let (val, closed) = open_for_embedding(arena, b.val);
                    any_closed |= closed;
                    DisjunctionBranch { val, ..b }
                })
                .collect();
            if any_closed {
                (arena.alloc(Value::Disjunction { branches: opened }), true)
            } else {
                (id, false)
            }
        }
        // A definition read before its declaration resolves through a
        // placeholder, which by now points at the closed value.
        Some(Value::RecursiveRef {
            target: Some(target),
            ..
        }) => open_for_embedding(arena, target),
        _ => (id, false),
    }
}

/// Close the outermost struct of a merged embedding again, or every struct
/// branch when the merge left a disjunction.
pub fn reclose(arena: &mut ValueArena, id: ValueId) -> ValueId {
    if let Some(mut metadata) = arena.metadata(id).cloned() {
        if arena.has_embedded_recipe(id) {
            if matches!(
                metadata.source,
                MetadataSource::Closed {
                    closure: Closure::Recursive | Closure::Outer,
                    ..
                }
            ) {
                return id;
            }
            let conjuncts = match metadata.source {
                MetadataSource::Embedded(conjuncts) => conjuncts,
                _ => vec![Conjunct::Value(id)],
            };
            let payload = crate::metadata::payload(arena, id);
            let closed = reclose(arena, payload);
            metadata.view = Some(reclose(arena, metadata.view.unwrap_or(metadata.fields)));
            metadata.source = MetadataSource::Closed {
                conjuncts,
                closure: Closure::Outer,
            };
            let value = arena.get(closed).expect("closed payload exists").clone();
            return arena.alloc_with_metadata(value, metadata);
        }
        let fields = reclose(arena, metadata.fields);
        let payload = if metadata.is_choice_view() {
            let payload = crate::metadata::payload(arena, id);
            reclose(arena, payload)
        } else {
            id
        };
        if fields == metadata.fields && payload == id {
            return id;
        }
        metadata.fields = fields;
        let value = arena.get(payload).expect("metadata owner exists").clone();
        return arena.alloc_with_metadata(value, metadata);
    }
    match arena.get(id).cloned() {
        Some(Value::Struct(mut s)) if !s.is_closed && !s.is_open => {
            s.is_closed = true;
            arena.alloc(Value::Struct(s))
        }
        Some(Value::Disjunction { branches }) => {
            let closed = branches
                .into_iter()
                .map(|b| DisjunctionBranch {
                    val: reclose(arena, b.val),
                    ..b
                })
                .collect();
            arena.alloc(Value::Disjunction { branches: closed })
        }
        _ => id,
    }
}

/// An undecided declaration of a closed struct a merge is inside: it may yet
/// generate fields below that struct, so a closed struct beneath it admits
/// what it could generate on credit instead of refusing it.
///
/// A comprehension in a definition's body adds fields that are part of the
/// definition (`#P: {a: int | *0, n: {if a > 0 {c: 2}}}` declares `n.c` once
/// `a > 0`), but whether it adds them is decided by the merge that is checking
/// them: `#P & {a: 1, n: c: 2}` has to admit `c` before the re-derivation that
/// moves `a` runs the comprehension again.
#[derive(Debug, Clone)]
pub(crate) struct Voucher {
    source: DeclSource,
    /// How deep in the merge's path the struct holding it sits.
    depth: usize,
}

impl Voucher {
    pub(crate) fn new(recipe: &DeclRecipe, depth: usize) -> Self {
        Self {
            source: recipe.source.clone(),
            depth,
        }
    }

    pub(crate) fn ptr(&self) -> *const () {
        self.source.ptr()
    }

    /// Whether it could generate the field at `path` below the struct holding
    /// it, judged from its syntax. Over-approximate: anything it cannot see
    /// through (a reference, a call, an embedding) could hold any field.
    pub(crate) fn may_generate(&self, path: &[String], name: &str) -> bool {
        let mut full: Vec<&str> = path
            .get(self.depth..)
            .unwrap_or_default()
            .iter()
            .map(String::as_str)
            .collect();
        full.push(name);
        match &self.source {
            DeclSource::Comprehension(comp) => decls_may_generate(&comp.struct_lit.decls, &full),
            DeclSource::Field(field) => {
                // A label that was not concrete may become any name.
                expr_may_hold(&field.value, &full[1..])
            }
        }
    }
}

impl PartialEq for Voucher {
    fn eq(&self, other: &Self) -> bool {
        self.ptr() == other.ptr() && self.depth == other.depth
    }
}

impl Eq for Voucher {}

fn decls_may_generate(decls: &[Decl], path: &[&str]) -> bool {
    let Some((first, rest)) = path.split_first() else {
        return true;
    };
    decls.iter().any(|decl| match decl {
        Decl::Field(field) => {
            let label = match &field.label {
                Label::Ident(name) | Label::String(name) => name == first,
                Label::Pattern(_) | Label::Dynamic(_) => true,
                // Hidden fields and definitions are never closedness's concern.
                Label::DefIdent(_) | Label::HiddenIdent(_) | Label::HiddenDefIdent(_) => false,
            };
            label && expr_may_hold(&field.value, rest)
        }
        Decl::Embedding(expr) => expr_may_hold(expr, path),
        Decl::Comprehension(comp) => decls_may_generate(&comp.struct_lit.decls, path),
        Decl::Ellipsis(_) => true,
        Decl::Alias { .. }
        | Decl::Let { .. }
        | Decl::Attribute(_)
        | Decl::Comment(_)
        | Decl::BlankLine => false,
    })
}

/// Whether a value written as `expr` could hold a field at `path`.
fn expr_may_hold(expr: &Expr, path: &[&str]) -> bool {
    if path.is_empty() {
        return true;
    }
    match expr {
        Expr::Struct(lit) => decls_may_generate(&lit.decls, path),
        Expr::Binary {
            op: BinaryOp::Unify,
            left,
            right,
        } => expr_may_hold(left, path) || expr_may_hold(right, path),
        Expr::Disjunction { branches } => branches
            .iter()
            .any(|branch| expr_may_hold(&branch.expr, path)),
        Expr::Bottom
        | Expr::Null
        | Expr::Bool(_)
        | Expr::Number(_)
        | Expr::String(_)
        | Expr::Bytes(_)
        | Expr::Interpolation { .. } => false,
        _ => true,
    }
}

/// Admit `name` into a merge with closed struct `closed` on credit, if a
/// declaration of `closed` itself or of a closed struct around it could still
/// generate it. `None` means nothing could: the field is not allowed.
/// Whether `id` holds a comprehension or dynamic field anywhere: something
/// that can generate a field when derived again.
pub(crate) fn holds_recipe(arena: &ValueArena, id: ValueId) -> bool {
    fn walk(arena: &ValueArena, id: ValueId, seen: &mut HashSet<ValueId>) -> bool {
        if !seen.insert(id) {
            return false;
        }
        match arena.get(id) {
            Some(Value::Struct(s)) => {
                !s.recipes.is_empty()
                    || s.fields.values().any(|entry| walk(arena, entry.val, seen))
                    || s.pattern_constraints
                        .iter()
                        .any(|pc| walk(arena, pc.target_val, seen))
            }
            Some(Value::List { elements, ellipsis }) => {
                elements.iter().any(|&element| walk(arena, element, seen))
                    || ellipsis.is_some_and(|element| walk(arena, element, seen))
            }
            Some(Value::Disjunction { branches }) => {
                branches.iter().any(|branch| walk(arena, branch.val, seen))
            }
            _ => false,
        }
    }
    walk(arena, id, &mut HashSet::new())
}

pub(crate) fn admit_on_credit(
    closed: &StructValue,
    around: &[Voucher],
    path: &[String],
    name: &str,
) -> Option<Provisional> {
    let own = closed
        .recipes
        .iter()
        .map(|recipe| Voucher::new(recipe, path.len()));
    let vouchers: Vec<*const ()> = around
        .iter()
        .cloned()
        .chain(own)
        .filter(|voucher| voucher.may_generate(path, name))
        .map(|voucher| voucher.ptr())
        .collect();
    (!vouchers.is_empty()).then(|| Provisional {
        name: name.to_string(),
        vouchers: vouchers.into(),
    })
}

/// Settle the fields admitted on credit that `generated` - what the
/// declaration at `source` generated when it ran again - now declares, in
/// `target` and in the structs below it.
pub(crate) fn vouch(
    arena: &mut ValueArena,
    target: ValueId,
    generated: ValueId,
    source: *const (),
) -> ValueId {
    if arena.metadata(target).is_some() {
        return target;
    }
    let Some(Value::Struct(s)) = arena.get(target) else {
        return target;
    };
    let mut s = s.as_ref().clone();
    if vouch_struct(arena, &mut s, generated, source) {
        arena.alloc(Value::Struct(Box::new(s)))
    } else {
        target
    }
}

/// [`vouch`] on a struct held in place. Reports whether anything settled.
pub(crate) fn vouch_struct(
    arena: &mut ValueArena,
    s: &mut StructValue,
    generated: ValueId,
    source: *const (),
) -> bool {
    let Some(declared) = arena.fields(generated).cloned() else {
        return false;
    };
    let before = s.provisional.len();
    s.provisional.retain(|credit| {
        !(credit.vouchers.contains(&source)
            && (declared.fields.contains_key(&credit.name)
                || declared.pattern_constraints.iter().any(|pc| {
                    crate::unify::field_matches_pattern(arena, pc.pattern_val, &credit.name)
                })))
    });
    let mut changed = s.provisional.len() != before;
    for (name, entry) in &declared.fields {
        if let Some(own) = s.fields.get_mut(name) {
            let settled = vouch(arena, own.val, entry.val, source);
            changed |= settled != own.val;
            own.val = settled;
        }
    }
    changed
}

/// The path of the first field admitted on credit that nothing generated, if
/// any: a field the closed struct holding it does not allow.
pub fn unvouched_field(arena: &ValueArena, id: ValueId) -> Option<Vec<String>> {
    match arena.get(id)? {
        Value::Struct(s) => {
            if let Some(credit) = s.provisional.first() {
                return Some(vec![credit.name.clone()]);
            }
            s.fields
                .iter()
                .filter(|(_, entry)| !entry.optional)
                .find_map(|(name, entry)| {
                    let mut path = unvouched_field(arena, entry.val)?;
                    path.insert(0, name.clone());
                    Some(path)
                })
        }
        Value::List { elements, .. } => elements.iter().enumerate().find_map(|(idx, &elem)| {
            let mut path = unvouched_field(arena, elem)?;
            path.insert(0, idx.to_string());
            Some(path)
        }),
        // A disjunction fails only when every branch does.
        Value::Disjunction { branches } => {
            let mut paths = branches
                .iter()
                .map(|branch| unvouched_field(arena, branch.val));
            let first = paths.next()??;
            paths.all(|path| path.is_some()).then_some(first)
        }
        _ => None,
    }
}

/// The branches of a disjunction that hold no field admitted on credit and
/// never generated: a branch that does was not allowed after all, as a
/// branch a closed definition refuses at the merge is.
pub(crate) fn allowed_branches<'a>(
    arena: &ValueArena,
    branches: &'a [DisjunctionBranch],
) -> Vec<&'a DisjunctionBranch> {
    branches
        .iter()
        .filter(|branch| unvouched_field(arena, branch.val).is_none())
        .collect()
}
