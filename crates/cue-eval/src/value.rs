#[cfg(feature = "memory-profile")]
mod profile;

use crate::scope::ScopeFrame;
use cue_syntax::ast::Expr;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use slotmap::{SlotMap, new_key_type};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

new_key_type! {
    pub struct ValueId;
}

/// How deep the unresolved-reference walk descends. The value graph it walks
/// can be cyclic, and the walk carries no visited set because it runs on every
/// declaration of every relaxation pass.
pub(crate) const MAX_UNRESOLVED_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeKind {
    Top,
    Bottom,
    Null,
    Bool,
    Number(NumberKind),
    String,
    Bytes,
    List,
    Struct,
}

/// The numeric lattice: `number` above the int and float families, `int`
/// above every integer, `uint` above the unsigned integers, `float` above the
/// float widths, and each width accepting only itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NumberKind {
    Number,
    Int,
    Uint,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Float,
    Float32,
    Float64,
}

impl NumberKind {
    /// Integer family: `int`, `uint` and every width. Plain `number` is not
    /// one — it is their parent, accepting floats too.
    pub fn is_integer(self) -> bool {
        matches!(
            self,
            NumberKind::Int
                | NumberKind::Uint
                | NumberKind::Uint8
                | NumberKind::Uint16
                | NumberKind::Uint32
                | NumberKind::Uint64
                | NumberKind::Int8
                | NumberKind::Int16
                | NumberKind::Int32
                | NumberKind::Int64
        )
    }

    pub fn is_unsigned(self) -> bool {
        matches!(
            self,
            NumberKind::Uint
                | NumberKind::Uint8
                | NumberKind::Uint16
                | NumberKind::Uint32
                | NumberKind::Uint64
        )
    }

    pub fn is_float(self) -> bool {
        matches!(
            self,
            NumberKind::Float | NumberKind::Float32 | NumberKind::Float64
        )
    }

    /// Whether this kind accepts every value `other` accepts: the Type-vs-Type
    /// meet rule. The narrower side wins; unrelated kinds do not subsume.
    pub fn subsumes(self, other: Self) -> bool {
        self == other
            || matches!(self, NumberKind::Number)
            || matches!(self, NumberKind::Int) && other.is_integer()
            || matches!(self, NumberKind::Uint) && other.is_unsigned()
            || matches!(self, NumberKind::Float) && other.is_float()
    }

    /// Whether an integer value is in this kind's range. Float kinds accept
    /// no integer.
    pub fn contains_int(self, i: &BigInt) -> bool {
        use num_traits::ToPrimitive;
        match self {
            NumberKind::Number | NumberKind::Int => true,
            NumberKind::Uint => i.sign() != num_bigint::Sign::Minus,
            NumberKind::Uint8 => matches!(i.to_i64(), Some(n) if (0..=255).contains(&n)),
            NumberKind::Uint16 => matches!(i.to_i64(), Some(n) if (0..=65535).contains(&n)),
            NumberKind::Uint32 => {
                matches!(i.to_i64(), Some(n) if (0..=4294967295).contains(&n))
            }
            NumberKind::Uint64 => i.sign() != num_bigint::Sign::Minus && i.to_u64().is_some(),
            NumberKind::Int8 => matches!(i.to_i64(), Some(n) if (-128..=127).contains(&n)),
            NumberKind::Int16 => {
                matches!(i.to_i64(), Some(n) if (-32768..=32767).contains(&n))
            }
            NumberKind::Int32 => {
                matches!(i.to_i64(), Some(n) if (-2147483648..=2147483647).contains(&n))
            }
            NumberKind::Int64 => i.to_i64().is_some(),
            NumberKind::Float | NumberKind::Float32 | NumberKind::Float64 => false,
        }
    }
}

impl TypeKind {
    pub fn is_integer(&self) -> bool {
        matches!(self, TypeKind::Number(kind) if kind.is_integer())
    }

    pub fn is_unsigned_integer(&self) -> bool {
        matches!(self, TypeKind::Number(kind) if kind.is_unsigned())
    }

    pub fn is_float(&self) -> bool {
        matches!(self, TypeKind::Number(kind) if kind.is_float())
    }

    /// The numeric kind, if this type is a numeric one.
    pub fn number_kind(&self) -> Option<NumberKind> {
        match self {
            TypeKind::Number(kind) => Some(*kind),
            _ => None,
        }
    }
}

impl fmt::Display for NumberKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NumberKind::Number => write!(f, "number"),
            NumberKind::Int => write!(f, "int"),
            NumberKind::Uint => write!(f, "uint"),
            NumberKind::Uint8 => write!(f, "uint8"),
            NumberKind::Uint16 => write!(f, "uint16"),
            NumberKind::Uint32 => write!(f, "uint32"),
            NumberKind::Uint64 => write!(f, "uint64"),
            NumberKind::Int8 => write!(f, "int8"),
            NumberKind::Int16 => write!(f, "int16"),
            NumberKind::Int32 => write!(f, "int32"),
            NumberKind::Int64 => write!(f, "int64"),
            NumberKind::Float => write!(f, "float"),
            NumberKind::Float32 => write!(f, "float32"),
            NumberKind::Float64 => write!(f, "float64"),
        }
    }
}

impl fmt::Display for TypeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeKind::Top => write!(f, "_"),
            TypeKind::Bottom => write!(f, "_|_"),
            TypeKind::Null => write!(f, "null"),
            TypeKind::Bool => write!(f, "bool"),
            TypeKind::Number(kind) => write!(f, "{kind}"),
            TypeKind::String => write!(f, "string"),
            TypeKind::Bytes => write!(f, "bytes"),
            TypeKind::List => write!(f, "list"),
            TypeKind::Struct => write!(f, "struct"),
        }
    }
}

/// Bound operators on evaluated values. The single definition lives in
/// `cue-syntax` beside the syntax positions that convert into it.
pub use cue_syntax::Bound;

/// Why a value is bottom, kept beside the message rather than recovered from it.
///
/// The evaluator has to tell one kind of failure from another - the relaxation
/// loop retries an unresolved reference and gives up on a conflict - and until
/// this existed it did so by reading English back out of `message`. A kind is
/// decided where the bottom is built, which is the only place that knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BottomKind {
    /// Nothing in scope defines this name.
    ReferenceNotFound,
    /// The base resolves; it has no such field.
    UndefinedField,
    /// The base resolves, it has no such field, and the base is decided: a
    /// closed copy (read through a definition) or a select evaluated under a
    /// definition. Still retried while passes run, because a merge may yet
    /// supply the field; a survivor past the fixpoint is `eval`, where the
    /// open-world [`Self::UndefinedField`] stays `incomplete`.
    UndefinedFieldDefinite,
    /// Not resolved *yet*. The relaxation loop may resolve it on a later pass,
    /// and one that survives the final pass is a cycle.
    Unresolved,
    /// Not concrete enough to decide, and no reference to wait for: `or([])`
    /// over a comprehension whose source the instance has yet to fill. Unlike
    /// [`Self::Unresolved`], the relaxation loop does not wait on it - the merge
    /// that supplies the source re-derives it - and it does not collapse the
    /// struct holding it, which is a schema, not a conflict.
    Incomplete,
    /// An infinite value: a recursive reference that reached export.
    StructuralCycle,
    /// A self-referential cycle within a struct.
    Cycle,
    /// Two values that cannot both hold.
    Conflict,
    /// A user-supplied `error("message")` branch. It loses to any
    /// succeeding sibling at settle time and lends its message to a
    /// lone surviving failure, adopting that failure's code. Never
    /// retried: the message is final, only its code is contextual.
    Custom,
    /// A failure that is none of the above - a depth limit, a failed validator,
    /// an operation on the wrong type. These differ in wording, not in what the
    /// evaluator does with them.
    Other,
}

impl BottomKind {
    /// Whether another pass of the relaxation loop might still resolve this.
    ///
    /// All four reference failures qualify, including the two upstream reports
    /// as final. cue-rs decides the wording where the bottom is built, and at
    /// that moment a name the enclosing literal has not reached yet looks
    /// exactly like a name that does not exist; only the pass that finds no new
    /// information settles which it was. Retrying costs a pass, and not
    /// retrying would break every forward reference.
    pub fn may_resolve_later(self) -> bool {
        matches!(
            self,
            BottomKind::Unresolved
                | BottomKind::ReferenceNotFound
                | BottomKind::UndefinedField
                | BottomKind::UndefinedFieldDefinite
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BottomReason {
    pub kind: BottomKind,
    pub message: String,
    /// Where, below the value holding this bottom, it arose: the labels (and
    /// list indexes) of the fields that collapsed into it, outermost first.
    /// `#T & {o: timeout: "x"}` is bottom with path `o.timeout`, which export
    /// joins to the path it found the bottom at, as upstream reports it.
    pub path: Vec<String>,
}

impl fmt::Display for BottomReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "_|_ ({})", self.message)
        } else {
            write!(f, "_|_ ({}: {})", self.path.join("."), self.message)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bottom(BottomReason),
    Top,
    Null,
    Bool(bool),
    Int(BigInt),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Type(TypeKind),
    Bounds {
        base_type: Option<TypeKind>,
        constraints: Vec<(Bound, ValueId)>,
    },
    List {
        elements: Vec<ValueId>,
        ellipsis: Option<ValueId>,
    },
    /// Boxed: a struct's three field maps would otherwise set the size of
    /// every value in the arena.
    Struct(Box<StructValue>),
    Disjunction {
        branches: Vec<DisjunctionBranch>,
    },
    BuiltinValidator {
        name: String,
        target: ValueId,
    },
    Validators(Vec<ValueId>),
    /// Reference for recursive / cyclic schema graphs
    RecursiveRef {
        name: String,
        target: Option<ValueId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisjunctionBranch {
    pub default: bool,
    pub val: ValueId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PatternConstraint {
    pub pattern_val: ValueId,
    /// What the pattern adds to a field it matches. With aliases, this is the
    /// pattern evaluated with its label alias standing for the pattern itself
    /// (`{name: string}` for `[N=string]: {name: N}`): what any field it
    /// matches is at least. The evaluator derives the exact value per label.
    pub target_val: ValueId,
    /// Set when the pattern names the label it matches or the field it meets:
    /// its value is a different one for every label.
    pub aliases: Option<PatternAliases>,
}

/// The aliases of a pattern constraint (`[K=string]`, `[string]~(K,V)`,
/// `[string]: V=value`) and the value they are bound in.
#[derive(Debug, Clone)]
pub struct PatternAliases {
    /// Bound to the label a field matched with.
    pub label: Option<String>,
    /// Bound to the value of the field the pattern meets.
    pub field: Option<String>,
    /// The pattern's value, in the literal it was written in.
    pub recipe: Thunk,
}

/// The same pattern of the same literal: its value is shared, never copied.
impl PartialEq for PatternAliases {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label
            && self.field == other.field
            && Rc::ptr_eq(&self.recipe.expr, &other.recipe.expr)
            && Rc::ptr_eq(&self.recipe.env, &other.recipe.env)
    }
}

/// A struct's fields in arc order: the order upstream creates them while
/// evaluating, which is the order a comprehension over the struct yields.
/// Equality ignores the order, as upstream's does.
pub type FieldMap = indexmap::IndexMap<String, FieldEntry>;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StructValue {
    pub fields: FieldMap,
    pub definitions: FieldMap,
    pub hidden: FieldMap,
    pub pattern_constraints: Vec<PatternConstraint>,
    pub is_closed: bool,
    /// Written with `...`: closing the definition it belongs to leaves this
    /// struct open, though the structs inside it are closed as usual.
    pub is_open: bool,
    /// A postfix `...` spread explicitly reopened this value: unifications
    /// may add fields it does not declare. Unlike `is_open` (which an open
    /// literal also carries, and which only blocks auto-closing), this is
    /// the merge permission — and it survives `&`, per the oracle.
    pub spread_open: bool,
    /// Comprehensions and dynamic fields this struct's literals declared, with
    /// what each generated. A merge that moves what one of them reads derives
    /// it again: a default overridden, a value made concrete, a source that
    /// grew. One that could not decide yet leaves the struct incomplete.
    pub recipes: Vec<DeclRecipe>,
    /// Fields this closed struct admitted on credit: a merge brought them, the
    /// struct does not declare them, but a comprehension or dynamic field of
    /// its own (or of a closed struct around it) had not decided yet and could
    /// generate them. Running that declaration again settles each one (see
    /// [`crate::closedness::vouch`]); a field still here at export is not
    /// allowed.
    pub(crate) provisional: Vec<Provisional>,
}

/// A field a closed struct admitted before its declarations decided whether
/// they generate it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Provisional {
    pub(crate) name: String,
    /// The declarations that could still generate it, by source.
    pub(crate) vouchers: Rc<[*const ()]>,
}

/// A declaration whose fields depend on values a merge can still change.
#[derive(Debug, Clone)]
pub struct DeclRecipe {
    pub(crate) source: DeclSource,
    /// The literal it was written in. Deriving it again binds that literal's
    /// fields to their merged values, as a field's recipe does.
    pub(crate) env: Rc<ThunkEnv>,
    /// The names its clauses or label read. Only a merge that moves one of
    /// these can change what it generates.
    pub(crate) deps: Rc<HashSet<String>>,
    /// What it generated the last time it ran.
    pub(crate) outcome: Rc<DeclOutcome>,
    /// Read through a definition: what it generates when it runs again is
    /// closed, as the fields it generated the first time were.
    pub(crate) closes: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum DeclSource {
    Comprehension(Rc<cue_syntax::ast::ComprehensionDecl>),
    /// A field with a dynamic label that was not concrete.
    Field(Rc<cue_syntax::ast::FieldDecl>),
}

#[derive(Debug, Default)]
pub(crate) struct DeclOutcome {
    /// Each field it generated, with the recipes it contributed to it.
    pub fields: Vec<(crate::schedule::Section, String, Rc<[Conjunct]>)>,
    /// The recipes of the literals it generated, which leave with it.
    pub recipes: Vec<DeclRecipe>,
    /// Set when a guard, source or label was not concrete: the struct is
    /// incomplete until a merge decides it.
    pub incomplete: Option<String>,
}

impl DeclSource {
    /// The declaration's identity: syntax is shared, never copied.
    pub(crate) fn ptr(&self) -> *const () {
        match self {
            DeclSource::Comprehension(comp) => Rc::as_ptr(comp).cast(),
            DeclSource::Field(field) => Rc::as_ptr(field).cast(),
        }
    }
}

impl DeclRecipe {
    pub(crate) fn source_ptr(&self) -> *const () {
        self.source.ptr()
    }

    pub fn incomplete(&self) -> Option<&str> {
        self.outcome.incomplete.as_deref()
    }

    /// Whether a clause binds a name its body reads (`for`, `let`): the values
    /// it generated hold what that name was bound to when it ran.
    pub(crate) fn binds_clause_names(&self) -> bool {
        match &self.source {
            DeclSource::Comprehension(comp) => comp.clauses.iter().any(|clause| {
                matches!(
                    clause,
                    cue_syntax::ast::ComprehensionClause::For { .. }
                        | cue_syntax::ast::ComprehensionClause::Let { .. }
                )
            }),
            DeclSource::Field(_) => false,
        }
    }

    /// Whether running it again generated the same fields: the same
    /// declaration (syntax is interned), deciding the same way. The values of
    /// those fields are their own recipes' business.
    pub(crate) fn same_decision(&self, other: &Self) -> bool {
        self.source_ptr() == other.source_ptr()
            && self.outcome.incomplete.is_some() == other.outcome.incomplete.is_some()
            && self.outcome.fields.len() == other.outcome.fields.len()
            && self
                .outcome
                .fields
                .iter()
                .zip(&other.outcome.fields)
                .all(|((a, x, _), (b, y, _))| a == b && x == y)
    }
}

/// One run of a declaration: the same recipe only if it is the same outcome.
impl PartialEq for DeclRecipe {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.outcome, &other.outcome)
    }
}

impl Conjunct {
    /// The same recipe, not merely an equal one: what a retraction removes.
    pub(crate) fn same(&self, other: &Self) -> bool {
        match (self, other) {
            // A definition's contribution is the same one, closed or not:
            // a recipe records it before the definition is closed.
            (
                Conjunct::Value(a) | Conjunct::Closed(a),
                Conjunct::Value(b) | Conjunct::Closed(b),
            ) => a == b,
            (Conjunct::Thunk(a), Conjunct::Thunk(b)) => {
                Rc::ptr_eq(&a.expr, &b.expr) && Rc::ptr_eq(&a.env, &b.env)
            }
            _ => false,
        }
    }
}

/// The packages one file imported.
///
/// An import declaration binds an identifier in file scope, so which package a
/// name means is a property of the file the name was written in, not of the
/// evaluator. Two files may bind one identifier to two different packages, and
/// a recipe derived again after it crossed an import boundary has to resolve
/// its own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Imports {
    /// The identifier an import declaration binds, to the path it names.
    pub aliases: HashMap<String, String>,
    /// Import path to the package value loaded for it. Absent for a stdlib
    /// package, which is answered by a builtin rather than by a value.
    pub packages: HashMap<String, ValueId>,
}

impl Imports {
    /// The import path an identifier names, which is the identifier itself when
    /// no declaration bound it - a stdlib package is written under its own name.
    pub fn path_of<'a>(&'a self, name: &'a str) -> &'a str {
        self.aliases.get(name).map(String::as_str).unwrap_or(name)
    }

    /// The package value an identifier names, if one was loaded for it.
    pub fn package_of(&self, name: &str) -> Option<ValueId> {
        self.packages.get(self.path_of(name)).copied()
    }
}

/// Lexical environment a field expression was written in.
///
/// Shared by every thunk of one struct literal, so capturing it costs one clone
/// per literal rather than one per field.
#[derive(Debug, Clone)]
pub struct ThunkEnv {
    /// The scope stack as it stood where the literal was written. Deriving a
    /// field again runs its expression here, under one frame holding the merged
    /// values of the names this literal declares.
    pub scopes: Vec<ScopeFrame>,
    /// This literal's `let` and alias declarations, in source order. They are
    /// scope bindings rather than fields, so a thunk cannot read them back from
    /// a struct and derives them again beside the field that reads one.
    pub lets: Vec<(String, Rc<Expr>)>,
    /// Field names this literal declares. Only these are taken from the merged
    /// struct: a name another conjunct contributed is not one this literal
    /// wrote, so it keeps resolving in the scope it was written in.
    pub own_fields: RefCell<HashSet<String>>,
    /// The packages the file holding this literal imported. Shared with every
    /// other literal of that file, so capturing it is a refcount bump.
    pub imports: Rc<Imports>,
    /// Set on a comprehension body: the literal the comprehension is written
    /// in. The body declares into that literal's struct, so the names that
    /// literal declares are read from the merged struct too, in the frame of
    /// `scopes` they were bound in.
    pub(crate) enclosing: Option<Enclosing>,
}

/// The literal around a comprehension body, and the index into the body's
/// scope stack of the frame that literal's names are bound in.
#[derive(Debug, Clone)]
pub(crate) struct Enclosing {
    pub(crate) env: Rc<ThunkEnv>,
    pub(crate) frame: usize,
}

impl ThunkEnv {
    pub fn new(
        scopes: Vec<ScopeFrame>,
        lets: Vec<(String, Rc<Expr>)>,
        own_fields: HashSet<String>,
        imports: Rc<Imports>,
    ) -> Self {
        Self {
            scopes,
            lets,
            own_fields: RefCell::new(own_fields),
            imports,
            enclosing: None,
        }
    }

    pub(crate) fn with_enclosing(mut self, enclosing: Option<Enclosing>) -> Self {
        self.enclosing = enclosing;
        self
    }

    pub fn owns_field(&self, name: &str) -> bool {
        self.own_fields.borrow().contains(name)
    }

    /// The literals a comprehension body is nested in, innermost first.
    pub(crate) fn enclosing_literals(&self) -> impl Iterator<Item = &Enclosing> {
        std::iter::successors(self.enclosing.as_ref(), |outer| {
            outer.env.enclosing.as_ref()
        })
    }

    /// The `let` bindings a recipe written here can read: this literal's, and,
    /// in a comprehension body, those of every literal around it. A recipe
    /// that reads one reads what the binding reads.
    pub(crate) fn reachable_lets(&self) -> std::borrow::Cow<'_, [(String, Rc<Expr>)]> {
        if self
            .enclosing_literals()
            .all(|outer| outer.env.lets.is_empty())
        {
            return std::borrow::Cow::Borrowed(&self.lets);
        }
        let mut lets = self.lets.clone();
        for outer in self.enclosing_literals() {
            lets.extend(outer.env.lets.iter().cloned());
        }
        std::borrow::Cow::Owned(lets)
    }
}

/// An unevaluated field expression together with the environment it was read in.
#[derive(Debug, Clone)]
pub struct Thunk {
    pub expr: Rc<Expr>,
    pub env: Rc<ThunkEnv>,
    /// Every name the expression mentions, with the literal's `let` bindings
    /// followed through. Deriving this recipe again can only produce something
    /// new if one of these moved, so a merge elsewhere in the struct leaves it
    /// alone. Over-approximate by construction - see [`crate::deps`].
    pub deps: Rc<HashSet<String>>,
    /// Read through a definition: what it derives is closed, as the value it
    /// derives again was (see [`crate::closedness`]).
    pub(crate) closes: bool,
}

impl Thunk {
    /// Whether any of the given names is one this recipe could read.
    pub fn reads_any(&self, names: &HashSet<String>) -> bool {
        if names.len() < self.deps.len() {
            names.iter().any(|name| self.deps.contains(name))
        } else {
            self.deps.iter().any(|dep| names.contains(dep))
        }
    }
}

/// One of the values a field is the unification of.
///
/// Unifying two structs concatenates their conjunct lists, so an override keeps
/// its place beside the expression it overrides and a re-forced thunk cannot
/// discard it.
#[derive(Debug, Clone)]
pub enum Conjunct {
    Value(ValueId),
    Thunk(Thunk),
    /// A value a definition contributed, as written (open). The field derives
    /// again as the union of what its definition conjuncts give, closed once,
    /// and then meets the rest: a definition is closed over all of its
    /// declarations for a field, not over each one.
    Closed(ValueId),
}

impl Conjunct {
    /// Whether this is a definition's contribution to the field (see
    /// [`Conjunct::Closed`] and [`Thunk::closes`]).
    pub(crate) fn closes(&self) -> bool {
        match self {
            Conjunct::Value(_) => false,
            Conjunct::Closed(_) => true,
            Conjunct::Thunk(thunk) => thunk.closes,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FieldEntry {
    /// Unification of the conjuncts, cached so readers and export are unchanged.
    pub val: ValueId,
    pub optional: bool,
    /// Immutable recipe sequence shared by copies of this field.
    pub conjuncts: Rc<[Conjunct]>,
}

impl FieldEntry {
    /// An entry whose recipe is the value itself: a field from a builtin, an
    /// imported package or the unifier, which nothing re-derives but which must
    /// still survive a merge with a field that is re-derived.
    pub fn value(val: ValueId, optional: bool) -> Self {
        Self {
            val,
            optional,
            conjuncts: Rc::from([Conjunct::Value(val)]),
        }
    }

    pub fn with_conjuncts(
        val: ValueId,
        optional: bool,
        conjuncts: impl Into<Rc<[Conjunct]>>,
    ) -> Self {
        Self {
            val,
            optional,
            conjuncts: conjuncts.into(),
        }
    }

    /// Extend only this field, preserving recipes held by earlier snapshots.
    pub(crate) fn extend_conjuncts(&mut self, added: impl IntoIterator<Item = Conjunct>) {
        self.conjuncts = self.conjuncts.iter().cloned().chain(added).collect();
    }

    pub fn has_thunk(&self) -> bool {
        self.conjuncts
            .iter()
            .any(|c| matches!(c, Conjunct::Thunk(_)))
    }

    /// Whether any recipe of this field could read one of the given names.
    pub fn reads_any(&self, names: &HashSet<String>) -> bool {
        self.conjuncts.iter().any(|conjunct| match conjunct {
            Conjunct::Value(_) | Conjunct::Closed(_) => false,
            Conjunct::Thunk(thunk) => thunk.reads_any(names),
        })
    }

    /// The names this field's recipes could read.
    pub fn deps(&self) -> impl Iterator<Item = &str> {
        self.conjuncts
            .iter()
            .filter_map(|conjunct| match conjunct {
                Conjunct::Value(_) | Conjunct::Closed(_) => None,
                Conjunct::Thunk(thunk) => Some(thunk.deps.iter().map(String::as_str)),
            })
            .flatten()
    }
}

/// Conjuncts record how a value was derived, not what it is, so they stay out of
/// value equality: two fields holding the same value are equal however each was
/// written.
impl PartialEq for FieldEntry {
    fn eq(&self, other: &Self) -> bool {
        self.val == other.val && self.optional == other.optional
    }
}

impl StructValue {
    pub fn new(is_closed: bool) -> Self {
        Self {
            fields: FieldMap::new(),
            definitions: FieldMap::new(),
            hidden: FieldMap::new(),
            pattern_constraints: Vec::new(),
            is_closed,
            is_open: false,
            spread_open: false,
            recipes: Vec::new(),
            provisional: Vec::new(),
        }
    }

    /// Keep a declaration's recipe, once.
    pub(crate) fn add_recipe(&mut self, recipe: DeclRecipe) {
        if !self.recipes.contains(&recipe) {
            self.recipes.push(recipe);
        }
    }

    /// Why the struct is incomplete, if a declaration in it is undecided.
    pub fn incomplete(&self) -> Option<&str> {
        self.recipes.iter().find_map(DeclRecipe::incomplete)
    }

    pub fn insert_field(&mut self, name: String, val: ValueId, optional: bool) {
        self.fields.insert(name, FieldEntry::value(val, optional));
    }

    pub fn insert_def(&mut self, name: String, val: ValueId, optional: bool) {
        self.definitions
            .insert(name, FieldEntry::value(val, optional));
    }

    pub fn insert_hidden(&mut self, name: String, val: ValueId, optional: bool) {
        self.hidden.insert(name, FieldEntry::value(val, optional));
    }

    pub fn add_pattern_constraint(&mut self, pattern_val: ValueId, target_val: ValueId) {
        self.pattern_constraints.push(PatternConstraint {
            pattern_val,
            target_val,
            aliases: None,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArenaCheckpoint {
    trail_len: usize,
}

/// Central arena managing all Value nodes with transaction undo-log (trail)
#[derive(Debug, Default, Clone)]
pub struct ValueArena {
    nodes: SlotMap<ValueId, Value>,
    /// Sparse because most nodes need only their payload. Metadata has the
    /// same lifetime as its owning node, including speculative rollback.
    metadata: HashMap<ValueId, ValueMetadata>,
    trail: Vec<ValueId>,
    /// Live `speculate` scopes (disjunction branches). The trail is positional:
    /// entries below the oldest live scope can never roll back, so the last
    /// scope to close truncates them. Checkpoints never escape the call that
    /// takes them, so between top-level evaluations the trail stays empty
    /// instead of retaining every node id ever allocated.
    speculation_depth: usize,
    /// Weak IDs: rollback may remove them, and public graph mutation may
    /// replace their payload. Validate each hit before sharing it again.
    booleans: [Option<ValueId>; 2],
    /// Only live, unmodified string nodes. Mutation and rollback remove keys
    /// so failed speculative work cannot retain text through this index.
    strings: HashMap<String, ValueId>,
    /// How many times an operation chose a default over other alternatives.
    /// A decision read through one is provisional: a merge may override it.
    defaults_chosen: usize,
    /// The partial value of a definition still pending, by its placeholder: a
    /// field of the definition that selects through its own name (`#L: {a: 1,
    /// b: #L.a}`) reads it on the next pass, as a regular field reads the
    /// partial its name is bound to.
    provisional: HashMap<ValueId, ValueId>,
}

#[derive(Debug, Clone)]
pub(crate) struct ValueMetadata {
    /// Always a struct node allocated in this arena.
    pub fields: ValueId,
    /// Fields of the materialized result, when branch-local declarations make
    /// its selector view differ from the recipe's input declarations.
    pub view: Option<ValueId>,
    pub source: MetadataSource,
}

#[derive(Debug, Clone)]
pub(crate) enum MetadataSource {
    Embedded(Vec<Conjunct>),
    /// An expression and its original declarations close together, before
    /// constraints supplied by a later unification are applied.
    Closed {
        conjuncts: Vec<Conjunct>,
        closure: crate::closedness::Closure,
    },
    /// The alternatives already contain the common fields. This record only
    /// makes those fields selectable without choosing an alternative.
    ChoiceFields,
}

impl ValueMetadata {
    pub(crate) fn is_choice_view(&self) -> bool {
        matches!(self.source, MetadataSource::ChoiceFields)
    }

    pub(crate) fn conjuncts(&self) -> Option<&[Conjunct]> {
        match &self.source {
            MetadataSource::Embedded(conjuncts) | MetadataSource::Closed { conjuncts, .. } => {
                Some(conjuncts)
            }
            MetadataSource::ChoiceFields => None,
        }
    }
}

impl ValueArena {
    pub fn new() -> Self {
        Self {
            nodes: SlotMap::with_key(),
            metadata: HashMap::new(),
            trail: Vec::new(),
            speculation_depth: 0,
            booleans: [None; 2],
            strings: HashMap::new(),
            defaults_chosen: 0,
            provisional: HashMap::new(),
        }
    }

    /// What a selector reads through a definition's placeholder while the
    /// definition is still pending: its partial value, if a pass produced one.
    pub(crate) fn provisional(&self, placeholder: ValueId) -> Option<ValueId> {
        if !self.is_placeholder(placeholder) {
            return None;
        }
        self.provisional
            .get(&placeholder)
            .copied()
            .filter(|&partial| self.get(partial).is_some())
    }

    pub(crate) fn set_provisional(&mut self, placeholder: ValueId, partial: Option<ValueId>) {
        match partial {
            Some(partial) => self.provisional.insert(placeholder, partial),
            None => self.provisional.remove(&placeholder),
        };
    }

    pub(crate) fn defaults_chosen(&self) -> usize {
        self.defaults_chosen
    }

    pub(crate) fn note_default_chosen(&mut self) {
        self.defaults_chosen += 1;
    }

    /// Allocate a fresh node, even when an equal value already exists.
    pub fn alloc(&mut self, val: Value) -> ValueId {
        let id = self.nodes.insert(val);
        self.trail.push(id);
        id
    }

    /// Semantic payload. Use [`Self::fields`] to inspect retained scalar fields.
    pub fn get(&self, id: ValueId) -> Option<&Value> {
        self.nodes.get(id)
    }

    /// Mutate a graph node, affecting every reference to it. Use [`Self::alloc`]
    /// for a fresh node when independent mutation is needed.
    pub fn get_mut(&mut self, id: ValueId) -> Option<&mut Value> {
        if let Some(Value::String(value)) = self.nodes.get(id)
            && self.strings.get(value) == Some(&id)
        {
            self.strings.remove(value);
        }
        self.nodes.get_mut(id)
    }

    /// Fields retained by a value, including definitions on a scalar or list.
    pub fn fields(&self, id: ValueId) -> Option<&StructValue> {
        if let Some(Value::Struct(fields)) = self.get(id) {
            return Some(fields);
        }
        let fields = self
            .metadata
            .get(&id)
            .map_or(id, |metadata| metadata.view.unwrap_or(metadata.fields));
        match self.get(fields) {
            Some(Value::Struct(fields)) => Some(fields),
            _ => None,
        }
    }

    pub(crate) fn metadata(&self, id: ValueId) -> Option<&ValueMetadata> {
        self.metadata.get(&id)
    }

    /// Whether a value is the placeholder of a definition not evaluated yet.
    pub fn is_placeholder(&self, val_id: ValueId) -> bool {
        matches!(
            self.get(val_id),
            Some(Value::RecursiveRef { target: None, .. })
        )
    }

    /// Whether a value carries a reference nothing has resolved yet.
    ///
    /// Two callers, one meaning: the relaxation loop retries a declaration that
    /// answers yes, and a re-derived recipe that answers yes keeps the value it
    /// had rather than replacing it with a reference the merge cannot satisfy.
    ///
    /// Every composite is walked, not only the ones that hold a field. A
    /// disjunction is the one that bites: `string | *"\(pkg.name)"` deriving to
    /// a default branch of bottom exports as `_|_`, so judging it resolved
    /// overwrites a good value with a broken one.
    pub fn is_unresolved(&self, val_id: ValueId) -> bool {
        self.is_unresolved_within(val_id, MAX_UNRESOLVED_DEPTH)
    }

    fn is_unresolved_within(&self, val_id: ValueId, depth: usize) -> bool {
        // A value graph can be cyclic. Past the budget, answer as this walk did
        // before it descended into composites at all: resolved, and written.
        let Some(depth) = depth.checked_sub(1) else {
            return false;
        };
        if self
            .metadata(val_id)
            .is_some_and(|metadata| self.is_unresolved_within(metadata.fields, depth))
        {
            return true;
        }
        // A `for` loop, not `Iterator::any`: the erased `dyn Iterator`
        // is not `Sized`, so combinators are unavailable here.
        let any = |ids: &mut dyn Iterator<Item = ValueId>| -> bool {
            for id in ids {
                if self.is_unresolved_within(id, depth) {
                    return true;
                }
            }
            false
        };
        match self.get(val_id) {
            Some(Value::Bottom(reason)) => reason.kind.may_resolve_later(),
            Some(Value::Struct(s)) => any(&mut s
                .fields
                .values()
                .chain(s.definitions.values())
                .chain(s.hidden.values())
                .map(|f| f.val)
                .chain(
                    s.pattern_constraints
                        .iter()
                        .flat_map(|pc| [pc.pattern_val, pc.target_val]),
                )),
            Some(Value::List { elements, ellipsis }) => {
                any(&mut elements.iter().copied().chain(*ellipsis))
            }
            Some(Value::Disjunction { branches }) => any(&mut branches.iter().map(|b| b.val)),
            Some(Value::Bounds { constraints, .. }) => {
                any(&mut constraints.iter().map(|(_, id)| *id))
            }
            Some(Value::Validators(targets)) => any(&mut targets.iter().copied()),
            Some(Value::BuiltinValidator { target, .. }) => {
                self.is_unresolved_within(*target, depth)
            }
            _ => false,
        }
    }

    pub(crate) fn embedded_metadata(&self, id: ValueId) -> Option<&ValueMetadata> {
        self.metadata(id)
            .filter(|metadata| metadata.conjuncts().is_some())
    }

    pub(crate) fn has_embedded_recipe(&self, id: ValueId) -> bool {
        self.metadata(id).is_some_and(|metadata| {
            matches!(metadata.source, MetadataSource::Closed { .. })
                || metadata
                    .conjuncts()
                    .into_iter()
                    .flatten()
                    .any(|conjunct| match conjunct {
                        Conjunct::Thunk(_) => true,
                        Conjunct::Value(id) | Conjunct::Closed(id) => self
                            .metadata(*id)
                            .is_some_and(|m| matches!(m.source, MetadataSource::Closed { .. })),
                    })
        })
    }

    pub(crate) fn alloc_with_metadata(&mut self, value: Value, metadata: ValueMetadata) -> ValueId {
        debug_assert!(matches!(self.get(metadata.fields), Some(Value::Struct(_))));
        debug_assert!(
            metadata
                .view
                .is_none_or(|view| matches!(self.get(view), Some(Value::Struct(_))))
        );
        debug_assert!(metadata.conjuncts().is_some() || matches!(value, Value::Disjunction { .. }));
        let id = self.alloc(value);
        self.metadata.insert(id, metadata);
        id
    }

    /// Copy a payload transformation without losing the owning value's fields.
    pub(crate) fn alloc_like(&mut self, source: ValueId, value: Value) -> ValueId {
        match self.metadata(source).cloned() {
            Some(metadata) => self.alloc_with_metadata(value, metadata),
            None => self.alloc(value),
        }
    }

    pub fn checkpoint(&self) -> ArenaCheckpoint {
        ArenaCheckpoint {
            trail_len: self.trail.len(),
        }
    }

    /// Open a speculation scope (a disjunction branch attempt): the matching
    /// [`Self::commit_speculation`] or [`Self::rollback`] must run on every
    /// path through the attempt, success or failure.
    pub fn speculate(&mut self) -> ArenaCheckpoint {
        self.speculation_depth += 1;
        self.checkpoint()
    }

    /// Close a speculation scope whose nodes stay live. With no scope left,
    /// no checkpoint can roll back, so the whole trail — positional dead
    /// weight from here on — is released. Small buffers keep their capacity
    /// (disjunctions speculate constantly); only a runaway buffer shrinks.
    pub fn commit_speculation(&mut self) {
        self.speculation_depth = self.speculation_depth.saturating_sub(1);
        if self.speculation_depth == 0 {
            self.trail.clear();
            if self.trail.capacity() > 1_000_000 {
                self.trail.shrink_to_fit();
            }
        }
    }

    pub fn rollback(&mut self, checkpoint: ArenaCheckpoint) {
        while self.trail.len() > checkpoint.trail_len {
            if let Some(id) = self.trail.pop() {
                if let Some(Value::String(value)) = self.nodes.remove(id)
                    && self.strings.get(&value) == Some(&id)
                {
                    self.strings.remove(&value);
                }
                self.metadata.remove(&id);
            }
        }
        self.commit_speculation();
    }

    pub fn bottom<S: Into<String>>(&mut self, msg: S) -> ValueId {
        self.bottom_of(BottomKind::Other, msg)
    }

    pub fn bottom_of<S: Into<String>>(&mut self, kind: BottomKind, msg: S) -> ValueId {
        self.alloc(Value::Bottom(BottomReason {
            kind,
            message: msg.into(),
            path: Vec::new(),
        }))
    }

    /// A bottom that collapses the struct or list holding it, reported at the
    /// field it came from: the holder's error, with `label` in front of its path.
    /// Anything else is returned as it is.
    pub(crate) fn bottom_at(&mut self, label: &str, val: ValueId) -> ValueId {
        let Some(Value::Bottom(reason)) = self.get(val) else {
            return val;
        };
        let mut reason = reason.clone();
        reason.path.insert(0, label.to_string());
        self.alloc(Value::Bottom(reason))
    }

    pub fn top(&mut self) -> ValueId {
        self.alloc(Value::Top)
    }

    pub fn null(&mut self) -> ValueId {
        self.alloc(Value::Null)
    }

    /// Return a shared, unannotated boolean. Metadata owners always use fresh
    /// allocations so fields and lexical recipes cannot attach to this cache.
    pub fn bool(&mut self, b: bool) -> ValueId {
        let index = usize::from(b);
        if let Some(id) = self.booleans[index]
            && matches!(self.get(id), Some(Value::Bool(value)) if *value == b)
        {
            return id;
        }
        let id = self.alloc(Value::Bool(b));
        self.booleans[index] = Some(id);
        id
    }

    pub fn int<I: Into<BigInt>>(&mut self, i: I) -> ValueId {
        self.alloc(Value::Int(i.into()))
    }

    pub fn float(&mut self, f: f64) -> ValueId {
        self.alloc(Value::Float(f))
    }

    /// Return a shared, unannotated string with exactly these contents. Use
    /// [`Self::alloc`] when independent graph mutation is needed.
    pub fn string<'a>(&mut self, s: impl Into<Cow<'a, str>>) -> ValueId {
        let value = s.into();
        if let Some(&id) = self.strings.get(value.as_ref()) {
            return id;
        }
        let value = value.into_owned();
        let id = self.alloc(Value::String(value.clone()));
        self.strings.insert(value, id);
        id
    }

    pub fn type_kind(&mut self, k: TypeKind) -> ValueId {
        self.alloc(Value::Type(k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;

    #[test]
    fn placeholder_and_unresolved_queries() {
        let mut arena = ValueArena::new();
        let top = arena.top();
        assert!(!arena.is_placeholder(top));
        assert!(!arena.is_unresolved(top));

        let pending = arena.bottom_of(BottomKind::Unresolved, "x");
        assert!(arena.is_unresolved(pending));
        let conflict = arena.bottom_of(BottomKind::Conflict, "y");
        assert!(!arena.is_unresolved(conflict));

        let placeholder = arena.alloc(Value::RecursiveRef {
            name: "D".to_string(),
            target: None,
        });
        assert!(arena.is_placeholder(placeholder));
        assert!(!arena.is_unresolved(placeholder));
    }

    #[test]
    fn speculation_releases_the_trail() {
        let mut arena = ValueArena::new();
        let kept = arena.int(1);
        // A failed attempt removes its nodes; a kept one keeps them.
        let failed = arena.speculate();
        let _temp = arena.int(2);
        arena.rollback(failed);
        assert!(arena.get(kept).is_some());
        // Closing the last scope releases the whole trail: nothing below
        // can roll back any more.
        let _scope = arena.speculate();
        let _temp = arena.int(3);
        arena.commit_speculation();
        assert!(arena.get(kept).is_some());
        assert!(arena.trail.is_empty());
        // Nesting only releases at the outermost close.
        let outer = arena.speculate();
        let _temp = arena.int(4);
        let _inner = arena.speculate();
        let _temp = arena.int(5);
        arena.commit_speculation();
        assert!(!arena.trail.is_empty());
        arena.rollback(outer);
        assert!(arena.get(kept).is_some());
    }

    #[test]
    fn display_strings_preserved() {
        let cases = [
            (NumberKind::Number, "number"),
            (NumberKind::Int, "int"),
            (NumberKind::Uint, "uint"),
            (NumberKind::Uint8, "uint8"),
            (NumberKind::Uint16, "uint16"),
            (NumberKind::Uint32, "uint32"),
            (NumberKind::Uint64, "uint64"),
            (NumberKind::Int8, "int8"),
            (NumberKind::Int16, "int16"),
            (NumberKind::Int32, "int32"),
            (NumberKind::Int64, "int64"),
            (NumberKind::Float, "float"),
            (NumberKind::Float32, "float32"),
            (NumberKind::Float64, "float64"),
        ];
        for (kind, text) in cases {
            assert_eq!(kind.to_string(), text);
            assert_eq!(TypeKind::Number(kind).to_string(), text);
        }
    }

    #[test]
    fn subsumption_is_asymmetric() {
        use NumberKind::*;
        // Parents subsume children, never the reverse.
        assert!(Number.subsumes(Int));
        assert!(!Int.subsumes(Number));
        assert!(Int.subsumes(Uint8));
        assert!(!Uint8.subsumes(Int));
        assert!(Uint.subsumes(Uint64));
        assert!(!Uint64.subsumes(Uint));
        assert!(Float.subsumes(Float32));
        assert!(!Float32.subsumes(Float));
        // Families do not cross.
        assert!(!Uint.subsumes(Int8));
        assert!(!Int8.subsumes(Uint));
        assert!(!Int.subsumes(Float));
        assert!(!Float.subsumes(Int));
        assert!(!Float32.subsumes(Float64));
        // Plain number is neither family's member but subsumes all.
        assert!(!Number.is_integer() && !Number.is_float());
        assert!(Number.subsumes(Float64));
    }

    #[test]
    fn int_ranges() {
        use NumberKind::*;
        let hopefully = |n: i64| BigInt::from(n);
        assert!(Int.contains_int(&hopefully(-1)));
        assert!(Number.contains_int(&hopefully(-1)));
        assert!(!Uint.contains_int(&hopefully(-1)));
        assert!(Uint.contains_int(&hopefully(0)));
        assert!(Uint8.contains_int(&hopefully(255)));
        assert!(!Uint8.contains_int(&hopefully(256)));
        assert!(!Uint8.contains_int(&hopefully(-1)));
        assert!(Int8.contains_int(&hopefully(-128)));
        assert!(!Int8.contains_int(&hopefully(-129)));
        assert!(Int8.contains_int(&hopefully(127)));
        assert!(!Int8.contains_int(&hopefully(128)));
        // Beyond i64: unbounded kinds hold, widths do not.
        let huge = BigInt::from(1) << 100;
        assert!(Int.contains_int(&huge));
        assert!(Uint.contains_int(&huge));
        assert!(!Uint8.contains_int(&huge));
        assert!(!Int64.contains_int(&huge));
        // Float kinds accept no integer.
        assert!(!Float.contains_int(&hopefully(1)));
        assert!(!Float32.contains_int(&hopefully(1)));
    }
}
