use crate::value::{FieldMap, StructValue};
use std::collections::{HashMap, HashSet};

/// Which map of a struct a field lives in. Definitions and hidden fields keep
/// their sigil in the name, so the three never collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    Field,
    Definition,
    Hidden,
}

impl Section {
    pub(crate) fn map(self, s: &StructValue) -> &FieldMap {
        match self {
            Section::Field => &s.fields,
            Section::Definition => &s.definitions,
            Section::Hidden => &s.hidden,
        }
    }

    pub(crate) fn map_mut(self, s: &mut StructValue) -> &mut FieldMap {
        match self {
            Section::Field => &mut s.fields,
            Section::Definition => &mut s.definitions,
            Section::Hidden => &mut s.hidden,
        }
    }
}

pub(crate) const SECTIONS: [Section; 3] = [Section::Field, Section::Definition, Section::Hidden];

/// The order to derive a struct's fields in: a field after the fields it
/// reads, so one sweep carries a change the whole length of a chain.
pub(crate) fn derivation_order(s: &StructValue) -> Vec<(Section, String)> {
    let nodes: Vec<(Section, String)> = SECTIONS
        .iter()
        .flat_map(|section| section.map(s).keys().map(|name| (*section, name.clone())))
        .collect();
    let held: HashSet<&str> = nodes.iter().map(|(_, name)| name.as_str()).collect();

    let (readers, mut pending) = build_readers_graph(&nodes, &held, s);
    let mut order: Vec<(Section, String)> = Vec::with_capacity(nodes.len());
    let mut ready: Vec<usize> = (0..nodes.len()).filter(|i| pending[*i] == 0).collect();
    let mut placed = vec![false; nodes.len()];

    while let Some(index) = ready.pop() {
        if std::mem::replace(&mut placed[index], true) {
            continue;
        }
        order.push(nodes[index].clone());
        if let Some(dependents) = readers.get(nodes[index].1.as_str()) {
            for dependent in dependents {
                pending[*dependent] = pending[*dependent].saturating_sub(1);
                if pending[*dependent] == 0 {
                    ready.push(*dependent);
                }
            }
        }
    }

    order.extend(
        nodes
            .into_iter()
            .enumerate()
            .filter(|(index, _)| !placed[*index])
            .map(|(_, node)| node),
    );
    order
}

fn build_readers_graph<'a>(
    nodes: &[(Section, String)],
    held: &HashSet<&'a str>,
    s: &'a StructValue,
) -> (HashMap<&'a str, Vec<usize>>, Vec<usize>) {
    let mut readers: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut pending: Vec<usize> = vec![0; nodes.len()];

    for (index, (section, name)) in nodes.iter().enumerate() {
        let Some(entry) = section.map(s).get(name) else {
            continue;
        };
        let mut read: HashSet<&str> = entry.deps().filter(|dep| held.contains(dep)).collect();
        // A recipe that reads its own field is a cycle of one.
        read.remove(name.as_str());
        pending[index] = read.len();
        for dep in read {
            readers.entry(dep).or_default().push(index);
        }
    }
    (readers, pending)
}
