//! Opt-in allocation attribution, kept out of the evaluation hot path.

use super::*;

impl ValueArena {
    /// Read-only allocation attribution; excluded from default builds.
    /// Counts all allocated nodes, including unreachable intermediates. Byte
    /// counts describe selected buffers, not total heap or allocator overhead.
    /// This walk allocates scratch sets: measure production RSS separately.
    pub fn memory_profile(&self) -> serde_json::Value {
        use std::collections::{BTreeMap, HashSet};
        let mut kinds = BTreeMap::<&str, usize>::new();
        let mut fields = 0usize;
        let mut conjunct_count = 0usize;
        let mut recipe_sequences = HashSet::new();
        let mut field_name_bytes = 0usize;
        let mut text_bytes = 0usize;
        let mut unique_strings = HashSet::new();
        let mut empty_lists = 0usize;
        let mut unique_lists = HashSet::new();
        let mut environments = HashSet::new();
        let mut frames = HashSet::new();
        let mut captured_bindings = 0usize;
        let mut visit_conjunct = |conjunct: &Conjunct| {
            if let Conjunct::Thunk(thunk) = conjunct
                && environments.insert(Rc::as_ptr(&thunk.env))
            {
                for frame in &thunk.env.scopes {
                    if frames.insert(frame.storage_identity()) {
                        captured_bindings += frame.len();
                    }
                }
            }
        };
        for value in self.nodes.values() {
            let kind = match value {
                Value::Struct(body) => {
                    for (name, field) in body
                        .fields
                        .iter()
                        .chain(&body.definitions)
                        .chain(&body.hidden)
                    {
                        fields += 1;
                        field_name_bytes += name.capacity();
                        if recipe_sequences.insert(Rc::as_ptr(&field.conjuncts)) {
                            conjunct_count += field.conjuncts.len();
                            for conjunct in field.conjuncts.iter() {
                                visit_conjunct(conjunct);
                            }
                        }
                    }
                    "struct"
                }
                Value::Bottom(reason) => {
                    text_bytes += reason.message.capacity();
                    "bottom"
                }
                Value::String(s) => {
                    text_bytes += s.capacity();
                    unique_strings.insert(s.as_str());
                    "string"
                }
                Value::Bytes(s) => {
                    text_bytes += s.capacity();
                    "bytes"
                }
                Value::Int(_) => "int",
                Value::Float(_) => "float",
                Value::Bool(_) => "bool",
                Value::Top => "top",
                Value::Null => "null",
                Value::Type(_) => "type",
                Value::Bounds { .. } => "bounds",
                Value::List { elements, ellipsis } => {
                    empty_lists += usize::from(elements.is_empty() && ellipsis.is_none());
                    unique_lists.insert((elements.as_slice(), *ellipsis));
                    "list"
                }
                Value::Disjunction { .. } => "choice",
                Value::BuiltinValidator { .. } | Value::Validators(_) => "validator",
                Value::RecursiveRef { .. } => "reference",
            };
            *kinds.entry(kind).or_default() += 1;
        }
        for metadata in self.metadata.values() {
            if let Some(conjuncts) = metadata.conjuncts() {
                for conjunct in conjuncts {
                    visit_conjunct(conjunct);
                }
            }
        }
        serde_json::json!({
            "value_size": std::mem::size_of::<Value>(),
            "field_entry_size": std::mem::size_of::<FieldEntry>(),
            "conjunct_size": std::mem::size_of::<Conjunct>(),
            "slots": self.nodes.len(), "slot_capacity": self.nodes.capacity(),
            "string_cache_entries": self.strings.len(),
            "string_cache_text_bytes": self.strings.keys().map(String::capacity).sum::<usize>(),
            "trail_capacity": self.trail.capacity(), "metadata": self.metadata.len(),
            "kinds": kinds, "fields": fields, "field_name_bytes": field_name_bytes,
            "conjunct_count": conjunct_count, "recipe_sequences": recipe_sequences.len(), "text_bytes": text_bytes,
            "unique_strings": unique_strings.len(), "empty_lists": empty_lists, "unique_lists": unique_lists.len(),
            "environments": environments.len(), "frames": frames.len(),
            "captured_bindings": captured_bindings,
        })
    }
}
