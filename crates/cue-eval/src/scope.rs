//! Lexical bindings with immutable captures and copy-on-write evaluation frames.
use crate::value::ValueId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

type Bindings = HashMap<String, ValueId>;

/// Cloning a frame captures its bindings without copying its map.
///
/// Only the evaluator can write a frame. A write detaches shared storage first,
/// so a capture keeps the bindings it was taken with. A recipe derived again
/// later reads the frame as it stands then ([`ScopeFrame::current`]): a name
/// the frame bound after the capture - a field declared further down, a
/// self-reference that settled on a later pass - is the same lexical name, and
/// upstream resolves it to that field's final value whatever the order.
#[derive(Debug, Clone, Default)]
pub struct ScopeFrame {
    bindings: Rc<Bindings>,
    /// Shared by the frame and its captures; `None` until the first capture.
    later: Option<Rc<RefCell<Later>>>,
}

/// What a frame bound after it was first captured.
#[derive(Debug, Default)]
struct Later {
    /// The bindings when the frame was first captured.
    base: Rc<Bindings>,
    /// Every write since, by name; `None` for a removal.
    writes: HashMap<String, Option<ValueId>>,
    /// `base` with `writes` applied, built on demand.
    view: Option<Rc<Bindings>>,
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

    /// A capture: the bindings as they stand, sharing storage, and linked to
    /// this frame so that [`ScopeFrame::current`] can read what it binds later.
    pub(crate) fn capture(&mut self) -> Self {
        if self.later.is_none() {
            self.later = Some(Rc::new(RefCell::new(Later {
                base: self.bindings.clone(),
                writes: HashMap::new(),
                view: None,
            })));
        }
        self.clone()
    }

    /// A frame of its own holding what the captured frame binds now: the
    /// captured bindings and every write the frame made after the capture.
    /// Writes to the result reach neither the frame nor its captures.
    pub(crate) fn current(&self) -> Self {
        let Some(later) = &self.later else {
            return Self {
                bindings: self.bindings.clone(),
                later: None,
            };
        };
        let mut later = later.borrow_mut();
        let bindings = if later.writes.is_empty() {
            self.bindings.clone()
        } else if let Some(view) = &later.view {
            view.clone()
        } else {
            let mut view = (*later.base).clone();
            for (name, value) in &later.writes {
                match value {
                    Some(value) => view.insert(name.clone(), *value),
                    None => view.remove(name),
                };
            }
            let view = Rc::new(view);
            later.view = Some(view.clone());
            view
        };
        Self {
            bindings,
            later: None,
        }
    }

    fn record(&mut self, name: &str, value: Option<ValueId>) {
        if let Some(later) = &self.later {
            let mut later = later.borrow_mut();
            later.writes.insert(name.to_owned(), value);
            later.view = None;
        }
    }

    pub(crate) fn insert(&mut self, name: &str, value: ValueId) {
        if self.get(name) != Some(&value) {
            Rc::make_mut(&mut self.bindings).insert(name.to_owned(), value);
            self.record(name, Some(value));
        }
    }

    pub(crate) fn remove(&mut self, name: &str) {
        if self.contains_key(name) {
            Rc::make_mut(&mut self.bindings).remove(name);
            self.record(name, None);
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
        let captured = active.capture();
        assert!(Rc::ptr_eq(&active.bindings, &captured.bindings));
        active.insert("name", first);
        active.remove("missing");
        assert!(Rc::ptr_eq(&active.bindings, &captured.bindings));
        active.insert("name", second);
        assert_eq!(active.get("name"), Some(&second));
        assert_eq!(captured.get("name"), Some(&first));
        let before_remove = active.capture();
        active.remove("name");
        assert_eq!(active.get("name"), None);
        assert_eq!(before_remove.get("name"), Some(&second));
    }

    #[test]
    fn current_reads_what_the_frame_bound_after_the_capture() {
        let mut arena = ValueArena::new();
        let first = arena.int(1);
        let second = arena.int(2);
        let mut active = ScopeFrame::default();
        active.insert("a", first);
        let captured = active.capture();
        // Nothing written since: the capture's own storage.
        assert!(Rc::ptr_eq(&captured.current().bindings, &captured.bindings));
        active.insert("a", second);
        active.insert("b", first);
        let mut now = captured.current();
        assert_eq!(now.get("a"), Some(&second));
        assert_eq!(now.get("b"), Some(&first));
        assert_eq!(captured.get("b"), None);
        // Writing the current view touches neither the frame nor the capture.
        now.insert("c", first);
        assert_eq!(active.get("c"), None);
        assert_eq!(captured.current().get("c"), None);
        active.remove("b");
        assert_eq!(captured.current().get("b"), None);
    }
}
