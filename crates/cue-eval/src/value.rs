use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use slotmap::{new_key_type, SlotMap};
use std::collections::BTreeMap;
use std::fmt;

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
        matches!(self, TypeKind::Float | TypeKind::Float32 | TypeKind::Float64)
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BottomReason {
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldEntry {
    pub val: ValueId,
    pub optional: bool,
}

impl StructValue {
    pub fn new(is_closed: bool) -> Self {
        Self {
            fields: BTreeMap::new(),
            definitions: BTreeMap::new(),
            hidden: BTreeMap::new(),
            pattern_constraints: Vec::new(),
            is_closed,
        }
    }

    pub fn insert_field(&mut self, name: String, val: ValueId, optional: bool) {
        self.fields.insert(name, FieldEntry { val, optional });
    }

    pub fn insert_def(&mut self, name: String, val: ValueId, optional: bool) {
        self.definitions.insert(name, FieldEntry { val, optional });
    }

    pub fn insert_hidden(&mut self, name: String, val: ValueId, optional: bool) {
        self.hidden.insert(name, FieldEntry { val, optional });
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
        self.alloc(Value::Bottom(BottomReason {
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
