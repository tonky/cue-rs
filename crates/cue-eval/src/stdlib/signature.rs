//! What a failed builtin call means: a conflict, or a value not concrete yet.
//!
//! A builtin reads each argument in one way - a string, an int, a list of
//! numbers, any value, a schema - and that decides what an argument that is
//! not concrete yet means (upstream's `CallCtxt` accessors). An abstract
//! argument whose kind can still satisfy the parameter (`string` for a
//! string, `_` for a list, `>1` for an int) leaves the call *incomplete*: a
//! merge may still supply the value, as it does for
//! `_t: {n: string, u: strings.ToUpper(n)} & {n: "a"}`. A kind that can never
//! satisfy it (`int` for a string, `[string]` for a list of numbers) is an
//! error, and so is an argument that is already an error.
//!
//! The parameters come from the pinned CUE implementation
//! ([`super::signatures`]); a call is judged only when it failed, so a builtin
//! that accepts more than upstream's keeps doing so.

use super::signatures::SIGNATURES;
use crate::value::{BottomKind, TypeKind, Value, ValueArena, ValueId};
use cue_syntax::Bound;

/// How a builtin reads one argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Param {
    String,
    /// Bytes or a string.
    Bytes,
    Bool,
    Int,
    Number,
    Struct,
    /// A list whose elements the builtin reads as values.
    List,
    /// A list sorted by a comparator: an element that cannot be compared is
    /// the comparator's error, not an incomplete call.
    SortedList,
    StringList,
    NumberList,
    /// Any concrete value, data all the way down.
    Value,
    /// Read as it is: a constraint, a comparator, a schema. Never judged.
    Schema,
}

#[derive(Debug)]
pub(crate) struct Signature {
    pub package: &'static str,
    pub name: &'static str,
    pub params: &'static [Param],
    /// Parameters before the first one with a default.
    pub required: usize,
    /// Returns a bool, so a call one argument short is a validator whose
    /// arguments fill the parameters after the validated value.
    pub validator: bool,
}

/// The signature of `name` in the package imported as `path`. Packages are
/// told apart by their last path element, so `json` and `encoding/json`
/// agree.
pub(crate) fn signature(path: &str, name: &str) -> Option<&'static Signature> {
    let last = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
    let package = last(path);
    SIGNATURES
        .iter()
        .find(|sig| sig.name == name && last(sig.package) == package)
}

/// A set of value kinds, as upstream's `adt.Kind` mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Kinds(u16);

impl Kinds {
    pub const NONE: Kinds = Kinds(0);
    pub const NULL: Kinds = Kinds(1);
    pub const BOOL: Kinds = Kinds(1 << 1);
    pub const INT: Kinds = Kinds(1 << 2);
    pub const FLOAT: Kinds = Kinds(1 << 3);
    pub const STRING: Kinds = Kinds(1 << 4);
    pub const BYTES: Kinds = Kinds(1 << 5);
    pub const LIST: Kinds = Kinds(1 << 6);
    pub const STRUCT: Kinds = Kinds(1 << 7);
    pub const NUMBER: Kinds = Kinds(Self::INT.0 | Self::FLOAT.0);
    pub const ALL: Kinds = Kinds(0xff);

    pub const fn or(self, other: Kinds) -> Kinds {
        Kinds(self.0 | other.0)
    }

    pub const fn and(self, other: Kinds) -> Kinds {
        Kinds(self.0 & other.0)
    }

    pub const fn meets(self, other: Kinds) -> bool {
        self.0 & other.0 != 0
    }
}

fn type_kinds(kind: &TypeKind) -> Kinds {
    match kind {
        TypeKind::Top => Kinds::ALL,
        TypeKind::Bottom => Kinds::NONE,
        TypeKind::Null => Kinds::NULL,
        TypeKind::Bool => Kinds::BOOL,
        TypeKind::Number(_) if kind.is_integer() => Kinds::INT,
        TypeKind::Number(_) if kind.is_float() => Kinds::FLOAT,
        TypeKind::Number(_) => Kinds::NUMBER,
        TypeKind::String => Kinds::STRING,
        TypeKind::Bytes => Kinds::BYTES,
        TypeKind::List => Kinds::LIST,
        TypeKind::Struct => Kinds::STRUCT,
    }
}

/// The kinds a value may still take: one for a concrete value, the type's
/// for an abstract one (`>1` is a number, `=~"x"` a string).
pub(crate) fn kinds(arena: &ValueArena, id: ValueId) -> Kinds {
    match arena.get(id) {
        None | Some(Value::Bottom(_)) => Kinds::NONE,
        Some(Value::Null) => Kinds::NULL,
        Some(Value::Bool(_)) => Kinds::BOOL,
        Some(Value::Int(_)) => Kinds::INT,
        Some(Value::Float(_)) => Kinds::FLOAT,
        Some(Value::String(_)) => Kinds::STRING,
        Some(Value::Bytes(_)) => Kinds::BYTES,
        Some(Value::List { .. }) => Kinds::LIST,
        Some(Value::Struct(_)) => Kinds::STRUCT,
        Some(Value::Type(kind)) => type_kinds(kind),
        Some(Value::Bounds {
            base_type: Some(kind),
            ..
        }) => type_kinds(kind),
        Some(Value::Bounds {
            base_type: None,
            constraints,
        }) => constraints
            .iter()
            .fold(Kinds::ALL, |all, (bound, limit)| match bound {
                Bound::RegexMatch | Bound::RegexNotMatch => all.and(Kinds::STRING),
                Bound::NotEqual => all,
                _ => match kinds(arena, *limit) {
                    limit if limit.meets(Kinds::NUMBER) => all.and(Kinds::NUMBER),
                    limit => all.and(limit),
                },
            }),
        Some(Value::Disjunction { branches }) => branches
            .iter()
            .fold(Kinds::NONE, |all, branch| all.or(kinds(arena, branch.val))),
        Some(
            Value::Top
            | Value::BuiltinValidator { .. }
            | Value::Validators(_)
            | Value::RecursiveRef { .. },
        ) => Kinds::ALL,
    }
}

fn concrete(value: &Value) -> bool {
    matches!(
        value,
        Value::Null
            | Value::Bool(_)
            | Value::Int(_)
            | Value::Float(_)
            | Value::String(_)
            | Value::Bytes(_)
            | Value::List { .. }
            | Value::Struct(_)
    )
}

/// What judging one argument found, worst first.
enum Finding {
    /// The argument, or a value inside it, is an error.
    Bottom(ValueId),
    /// It can never have the kind the parameter reads.
    Mismatch,
    /// Not concrete yet, but of a kind the parameter can read.
    Incomplete(String),
    Fine,
}

impl Finding {
    fn rank(&self) -> u8 {
        match self {
            Finding::Bottom(_) => 3,
            Finding::Mismatch => 2,
            Finding::Incomplete(_) => 1,
            Finding::Fine => 0,
        }
    }

    fn worse(self, other: Finding) -> Finding {
        if other.rank() > self.rank() {
            other
        } else {
            self
        }
    }
}

/// One scalar position: an error, a kind that can never fit, or a value not
/// concrete yet.
fn scalar(arena: &ValueArena, id: ValueId, want: Kinds, what: impl FnOnce() -> String) -> Finding {
    match arena.get(id) {
        None => Finding::Mismatch,
        Some(Value::Bottom(_)) => Finding::Bottom(id),
        Some(_) if !kinds(arena, id).meets(want) => Finding::Mismatch,
        Some(value) if !concrete(value) => Finding::Incomplete(what()),
        Some(_) => Finding::Fine,
    }
}

/// How upstream names what a value not concrete yet may become: `string`,
/// `number`, `_`.
pub(crate) fn kind_name(arena: &ValueArena, id: ValueId) -> String {
    let kinds = kinds(arena, id);
    let names = [
        (Kinds::NULL, "null"),
        (Kinds::BOOL, "bool"),
        (Kinds::NUMBER, "number"),
        (Kinds::INT, "int"),
        (Kinds::FLOAT, "float"),
        (Kinds::STRING, "string"),
        (Kinds::BYTES, "bytes"),
        (Kinds::LIST, "list"),
        (Kinds::STRUCT, "struct"),
    ];
    match names.iter().find(|(k, _)| *k == kinds) {
        Some((_, name)) => (*name).to_string(),
        None => "_".to_string(),
    }
}

fn elements(arena: &ValueArena, id: ValueId) -> Vec<ValueId> {
    match arena.get(id) {
        Some(Value::List { elements, .. }) => elements.clone(),
        _ => Vec::new(),
    }
}

/// A value read as data: concrete itself and, `depth` levels down, in its
/// list elements and regular fields - the parts that export.
fn data(arena: &mut ValueArena, id: ValueId, depth: usize) -> Finding {
    let id = crate::operators::operand(arena, id);
    let name = kind_name(arena, id);
    let finding = scalar(arena, id, Kinds::ALL, || {
        format!("non-concrete value {name}")
    });
    if !matches!(finding, Finding::Fine) || depth == 0 {
        return finding;
    }
    let children: Vec<ValueId> = match arena.get(id) {
        Some(Value::List { elements, .. }) => elements.clone(),
        Some(Value::Struct(s)) => {
            if let Some(reason) = s.incomplete() {
                return Finding::Incomplete(reason.to_string());
            }
            s.fields
                .values()
                .filter(|entry| !entry.optional)
                .map(|entry| entry.val)
                .collect()
        }
        _ => return Finding::Fine,
    };
    children.into_iter().fold(Finding::Fine, |found, child| {
        found.worse(data(arena, child, depth - 1))
    })
}

/// How far into a value a failed call looks for what is not concrete yet.
const DATA_DEPTH: usize = 64;

/// One argument as `param` reads it. Before the call that is what upstream's
/// accessor checks: the value itself, and the elements of a typed list.
/// After a failed call (`after_failure`) it is also what the builtin read
/// inside it: the elements of any list, data all the way down.
fn judge_one(
    arena: &mut ValueArena,
    param: Param,
    id: ValueId,
    index: usize,
    after_failure: bool,
) -> Finding {
    let name = kind_name(arena, id);
    let incomplete = || format!("non-concrete value {name}");
    let list = |arena: &mut ValueArena, element: Option<Kinds>| -> Finding {
        let top = scalar(arena, id, Kinds::LIST, || {
            format!("non-concrete list for argument {index}")
        });
        let Some(element) = element.filter(|_| matches!(top, Finding::Fine)) else {
            return top;
        };
        elements(arena, id)
            .into_iter()
            .enumerate()
            .fold(Finding::Fine, |found, (j, element_id)| {
                let element_id = crate::operators::operand(arena, element_id);
                let name = kind_name(arena, element_id);
                found.worse(scalar(arena, element_id, element, || {
                    format!("non-concrete value {name} for element {j} of argument {index}")
                }))
            })
    };
    match param {
        Param::Schema => Finding::Fine,
        Param::String => scalar(arena, id, Kinds::STRING, incomplete),
        Param::Bytes => scalar(arena, id, Kinds::STRING.or(Kinds::BYTES), incomplete),
        Param::Bool => scalar(arena, id, Kinds::BOOL, incomplete),
        Param::Int => scalar(arena, id, Kinds::INT, incomplete),
        Param::Number => scalar(arena, id, Kinds::NUMBER, incomplete),
        Param::Struct => scalar(arena, id, Kinds::STRUCT, || {
            format!("non-concrete struct for argument {index}")
        }),
        Param::List => list(arena, after_failure.then_some(Kinds::ALL)),
        Param::SortedList => list(arena, None),
        Param::StringList => list(arena, Some(Kinds::STRING)),
        Param::NumberList => list(arena, Some(Kinds::NUMBER)),
        Param::Value => data(arena, id, if after_failure { DATA_DEPTH } else { 0 }),
    }
}

/// The parameter each argument fills: a validator's arguments start at the
/// second parameter.
fn params_for(sig: &Signature, args: usize) -> &'static [Param] {
    if sig.validator && sig.required == sig.params.len() && args + 1 == sig.params.len() {
        &sig.params[1..]
    } else {
        sig.params
    }
}

/// The worst finding over every argument; the first of equally bad ones.
fn judge(
    arena: &mut ValueArena,
    sig: &Signature,
    args: &[ValueId],
    after_failure: bool,
) -> Finding {
    let params = params_for(sig, args.len());
    args.iter()
        .zip(params)
        .enumerate()
        .fold(Finding::Fine, |found, (index, (&id, &param))| {
            found.worse(judge_one(arena, param, id, index, after_failure))
        })
}

fn incomplete_call(arena: &mut ValueArena, qualified_name: &str, reason: &str) -> ValueId {
    arena.bottom_of(
        BottomKind::Incomplete,
        format!("error in call to {qualified_name}: {reason}"),
    )
}

/// What a call stands for before the builtin runs, as upstream's
/// `Builtin.call` and argument accessors decide it: an argument that is an
/// error is the call's error, and one not concrete yet but of a kind the
/// parameter reads makes the call incomplete. `None` runs the builtin - also
/// when an argument can never fit, so the builtin's own message reports it.
pub(crate) fn before_call(
    arena: &mut ValueArena,
    qualified_name: &str,
    sig: &Signature,
    args: &[ValueId],
) -> Option<ValueId> {
    match judge(arena, sig, args, false) {
        Finding::Bottom(id) => Some(id),
        Finding::Incomplete(reason) => Some(incomplete_call(arena, qualified_name, &reason)),
        Finding::Mismatch | Finding::Fine => None,
    }
}

/// The value a failed call stands for, `failure` being the builtin's own
/// message: an error inside an argument passed on, the failure as a conflict
/// when an argument can never fit, an incomplete value when what the builtin
/// read is not concrete yet but could be, or the failure as it was.
pub(crate) fn failed_call(
    arena: &mut ValueArena,
    qualified_name: &str,
    sig: &Signature,
    args: &[ValueId],
    failure: String,
) -> ValueId {
    match judge(arena, sig, args, true) {
        Finding::Bottom(id) => id,
        Finding::Mismatch => arena.bottom_of(BottomKind::Conflict, failure),
        Finding::Incomplete(reason) => incomplete_call(arena, qualified_name, &reason),
        Finding::Fine => arena.bottom(failure),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_package_resolves_by_its_last_path_element() {
        assert_eq!(
            signature("strings", "ToUpper").unwrap().params,
            &[Param::String]
        );
        assert_eq!(
            signature("json", "Marshal").unwrap().package,
            "encoding/json"
        );
        assert_eq!(
            signature("encoding/json", "Marshal").unwrap().params,
            &[Param::Value]
        );
        assert!(signature("strings", "NoSuchBuiltin").is_none());
    }

    #[test]
    fn the_table_is_the_pinned_implementation() {
        // `tools/builtin-signatures.py` over cue 635e4bb parses every builtin.
        assert_eq!(SIGNATURES.len(), 259);
        let join = signature("strings", "Join").unwrap();
        assert_eq!(join.params, &[Param::StringList, Param::String]);
        let sort = signature("list", "Sort").unwrap();
        assert_eq!(sort.params, &[Param::SortedList, Param::Schema]);
        let min_runes = signature("strings", "MinRunes").unwrap();
        assert!(min_runes.validator);
        assert_eq!(params_for(min_runes, 1), &[Param::Int]);
        assert_eq!(params_for(min_runes, 2), &[Param::String, Param::Int]);
    }

    #[test]
    fn abstract_kinds_follow_the_type() {
        let mut arena = ValueArena::new();
        let string = arena.alloc(Value::Type(TypeKind::String));
        assert_eq!(kinds(&arena, string), Kinds::STRING);
        let top = arena.alloc(Value::Top);
        assert_eq!(kinds(&arena, top), Kinds::ALL);
        let one = arena.int(1);
        let above_one = arena.alloc(Value::Bounds {
            base_type: None,
            constraints: vec![(Bound::Greater, one)],
        });
        assert_eq!(kinds(&arena, above_one), Kinds::NUMBER);
        let pattern = arena.string("x");
        let matching = arena.alloc(Value::Bounds {
            base_type: None,
            constraints: vec![(Bound::RegexMatch, pattern)],
        });
        assert!(kinds(&arena, matching).meets(Kinds::STRING));
        assert!(!kinds(&arena, matching).meets(Kinds::NUMBER));
    }
}
