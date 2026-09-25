//! Lexical bindings with immutable captures and copy-on-write evaluation frames.
use crate::value::ValueId;
use std::collections::HashMap;
use std::rc::Rc;

/// Cloning a frame captures its bindings without copying its map.
///
/// Only the evaluator can write a frame. A write detaches shared storage first,
/// so a recipe always sees the bindings captured when its literal was evaluated.
#[derive(Debug, Clone, Default)]
pub struct ScopeFrame {
    bindings: Rc<HashMap<String, ValueId>>,
}

impl ScopeFrame {
    #[cfg(feature = "memory-profile")]
    pub(crate) fn storage_identity(&self) -> *const () {
        Rc::as_ptr(&self.bindings).cast()
    }

    pub fn get(&self, name: &str) -> Option<&ValueId> {
        self.bindings.get(name)
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub fn values(&self) -> impl Iterator<Item = &ValueId> {
        self.bindings.values()
    }

    pub(crate) fn insert(&mut self, name: &str, value: ValueId) {
        if self.get(name) != Some(&value) {
            Rc::make_mut(&mut self.bindings).insert(name.to_owned(), value);
        }
    }

    pub(crate) fn remove(&mut self, name: &str) {
        if self.contains_key(name) {
            Rc::make_mut(&mut self.bindings).remove(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::ValueArena;

    #[test]
    fn captures_share_storage_until_a_binding_actually_changes() {
        let mut arena = ValueArena::new();
        let first = arena.int(1);
        let second = arena.int(2);
        let mut active = ScopeFrame::default();
        active.insert("name", first);
        let captured = active.clone();
        assert!(Rc::ptr_eq(&active.bindings, &captured.bindings));
        active.insert("name", first);
        active.remove("missing");
        assert!(Rc::ptr_eq(&active.bindings, &captured.bindings));
        active.insert("name", second);
        assert_eq!(active.get("name"), Some(&second));
        assert_eq!(captured.get("name"), Some(&first));
        let before_remove = active.clone();
        active.remove("name");
        assert_eq!(active.get("name"), None);
        assert_eq!(before_remove.get("name"), Some(&second));
    }
}
