use cue_syntax::ast::Expr;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use slotmap::{SlotMap, new_key_type};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

new_key_type! {
    pub struct ValueId;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeKind {
    Top,
    Bottom,
    Null,
    Bool,
    Int,
    Uint,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Int8,
    Int16,
    Int32,
    Int64,
    Float,
    Float32,
    Float64,
    Number,
    String,
    Bytes,
    List,
    Struct,
}

impl TypeKind {
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            TypeKind::Int
                | TypeKind::Uint
                | TypeKind::Uint8
                | TypeKind::Uint16
                | TypeKind::Uint32
                | TypeKind::Uint64
                | TypeKind::Int8
                | TypeKind::Int16
                | TypeKind::Int32
                | TypeKind::Int64
        )
    }

    pub fn is_unsigned_integer(&self) -> bool {
        matches!(
            self,
            TypeKind::Uint
                | TypeKind::Uint8
                | TypeKind::Uint16
                | TypeKind::Uint32
                | TypeKind::Uint64
        )
    }

    pub fn is_float(&self) -> bool {
        matches!(
            self,
            TypeKind::Float | TypeKind::Float32 | TypeKind::Float64
        )
    }
}

impl fmt::Display for TypeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeKind::Top => write!(f, "_"),
            TypeKind::Bottom => write!(f, "_|_"),
            TypeKind::Null => write!(f, "null"),
            TypeKind::Bool => write!(f, "bool"),
            TypeKind::Int => write!(f, "int"),
            TypeKind::Uint => write!(f, "uint"),
            TypeKind::Uint8 => write!(f, "uint8"),
            TypeKind::Uint16 => write!(f, "uint16"),
            TypeKind::Uint32 => write!(f, "uint32"),
            TypeKind::Uint64 => write!(f, "uint64"),
            TypeKind::Int8 => write!(f, "int8"),
            TypeKind::Int16 => write!(f, "int16"),
            TypeKind::Int32 => write!(f, "int32"),
            TypeKind::Int64 => write!(f, "int64"),
            TypeKind::Float => write!(f, "float"),
            TypeKind::Float32 => write!(f, "float32"),
            TypeKind::Float64 => write!(f, "float64"),
            TypeKind::Number => write!(f, "number"),
            TypeKind::String => write!(f, "string"),
            TypeKind::Bytes => write!(f, "bytes"),
            TypeKind::List => write!(f, "list"),
            TypeKind::Struct => write!(f, "struct"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoundOp {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    NotEqual,
    RegexMatch,
    RegexNotMatch,
}

impl fmt::Display for BoundOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoundOp::Less => write!(f, "<"),
            BoundOp::LessEqual => write!(f, "<="),
            BoundOp::Greater => write!(f, ">"),
            BoundOp::GreaterEqual => write!(f, ">="),
            BoundOp::NotEqual => write!(f, "!="),
            BoundOp::RegexMatch => write!(f, "=~"),
            BoundOp::RegexNotMatch => write!(f, "!~"),
        }
    }
}

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
    /// Two values that cannot both hold.
    Conflict,
    /// A failure that is none of the above - a depth limit, a failed validator,
    /// an operation on the wrong type. These differ in wording, not in what the
    /// evaluator does with them.
    Other,
}

impl BottomKind {
    /// Whether another pass of the relaxation loop might still resolve this.
    ///
    /// All three reference failures qualify, including the two upstream reports
    /// as final. cue-rs decides the wording where the bottom is built, and at
    /// that moment a name the enclosing literal has not reached yet looks
    /// exactly like a name that does not exist; only the pass that finds no new
    /// information settles which it was. Retrying costs a pass, and not
    /// retrying would break every forward reference.
    pub fn may_resolve_later(self) -> bool {
        matches!(
            self,
            BottomKind::Unresolved | BottomKind::ReferenceNotFound | BottomKind::UndefinedField
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BottomReason {
    pub kind: BottomKind,
    pub message: String,
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
        constraints: Vec<(BoundOp, ValueId)>,
    },
    List {
        elements: Vec<ValueId>,
        ellipsis: Option<ValueId>,
    },
    Struct(StructValue),
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
    pub target_val: ValueId,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StructValue {
    pub fields: BTreeMap<String, FieldEntry>,
    pub definitions: BTreeMap<String, FieldEntry>,
    pub hidden: BTreeMap<String, FieldEntry>,
    pub pattern_constraints: Vec<PatternConstraint>,
    pub is_closed: bool,
    /// Written with `...`: closing the definition it belongs to leaves this
    /// struct open, though the structs inside it are closed as usual.
    pub is_open: bool,
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
    pub scopes: Vec<HashMap<String, ValueId>>,
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
}

impl ThunkEnv {
    pub fn new(
        scopes: Vec<HashMap<String, ValueId>>,
        lets: Vec<(String, Rc<Expr>)>,
        own_fields: HashSet<String>,
        imports: Rc<Imports>,
    ) -> Self {
        Self {
            scopes,
            lets,
            own_fields: RefCell::new(own_fields),
            imports,
        }
    }

    pub fn owns_field(&self, name: &str) -> bool {
        self.own_fields.borrow().contains(name)
    }

    /// A field this literal declares only once it runs - a dynamic label, or one
    /// a comprehension or an embedding generates - is reachable the same way.
    pub fn note_field(&self, name: &str) {
        self.own_fields.borrow_mut().insert(name.to_string());
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
}

#[derive(Debug, Clone)]
pub struct FieldEntry {
    /// Unification of the conjuncts, cached so readers and export are unchanged.
    pub val: ValueId,
    pub optional: bool,
    pub conjuncts: Vec<Conjunct>,
}

impl FieldEntry {
    /// An entry whose recipe is the value itself: a field from a builtin, an
    /// imported package or the unifier, which nothing re-derives but which must
    /// still survive a merge with a field that is re-derived.
    pub fn value(val: ValueId, optional: bool) -> Self {
        Self {
            val,
            optional,
            conjuncts: vec![Conjunct::Value(val)],
        }
    }

    pub fn with_conjuncts(val: ValueId, optional: bool, conjuncts: Vec<Conjunct>) -> Self {
        Self {
            val,
            optional,
            conjuncts,
        }
    }

    pub fn has_thunk(&self) -> bool {
        self.conjuncts
            .iter()
            .any(|c| matches!(c, Conjunct::Thunk(_)))
    }

    /// Whether any recipe of this field could read one of the given names.
    pub fn reads_any(&self, names: &HashSet<String>) -> bool {
        self.conjuncts.iter().any(|conjunct| match conjunct {
            Conjunct::Value(_) => false,
            Conjunct::Thunk(thunk) => thunk.reads_any(names),
        })
    }

    /// The names this field's recipes could read.
    pub fn deps(&self) -> impl Iterator<Item = &str> {
        self.conjuncts
            .iter()
            .filter_map(|conjunct| match conjunct {
                Conjunct::Value(_) => None,
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
            fields: BTreeMap::new(),
            definitions: BTreeMap::new(),
            hidden: BTreeMap::new(),
            pattern_constraints: Vec::new(),
            is_closed,
            is_open: false,
        }
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
    trail: Vec<ValueId>,
}

impl ValueArena {
    pub fn new() -> Self {
        Self {
            nodes: SlotMap::with_key(),
            trail: Vec::new(),
        }
    }

    pub fn alloc(&mut self, val: Value) -> ValueId {
        let id = self.nodes.insert(val);
        self.trail.push(id);
        id
    }

    pub fn get(&self, id: ValueId) -> Option<&Value> {
        self.nodes.get(id)
    }

    pub fn get_mut(&mut self, id: ValueId) -> Option<&mut Value> {
        self.nodes.get_mut(id)
    }

    pub fn checkpoint(&self) -> ArenaCheckpoint {
        ArenaCheckpoint {
            trail_len: self.trail.len(),
        }
    }

    pub fn rollback(&mut self, checkpoint: ArenaCheckpoint) {
        while self.trail.len() > checkpoint.trail_len {
            if let Some(id) = self.trail.pop() {
                self.nodes.remove(id);
            }
        }
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

    pub fn top(&mut self) -> ValueId {
        self.alloc(Value::Top)
    }

    pub fn null(&mut self) -> ValueId {
        self.alloc(Value::Null)
    }

    pub fn bool(&mut self, b: bool) -> ValueId {
        self.alloc(Value::Bool(b))
    }

    pub fn int<I: Into<BigInt>>(&mut self, i: I) -> ValueId {
        self.alloc(Value::Int(i.into()))
    }

    pub fn float(&mut self, f: f64) -> ValueId {
        self.alloc(Value::Float(f))
    }

    pub fn string<S: Into<String>>(&mut self, s: S) -> ValueId {
        self.alloc(Value::String(s.into()))
    }

    pub fn type_kind(&mut self, k: TypeKind) -> ValueId {
        self.alloc(Value::Type(k))
    }
}
