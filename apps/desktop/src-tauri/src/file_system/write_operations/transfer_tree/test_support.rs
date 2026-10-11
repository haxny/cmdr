//! Helpers the tree's cells share: a pinned order, a readable outline of an
//! arena, and lookups by the labels a person would read.

use super::model::{NodeId, NodeKind, TreeModel};
use super::order::TreeOrder;
use cmdr_listing::testing::collator_for;

pub(crate) fn english() -> TreeOrder {
    TreeOrder::with_collator(collator_for("en"))
}

/// The arena as indented lines in tree order: `name/ 3 files 60 B` for a
/// folder (`name/? …` when opaque), `name 20 B` for a file.
pub(crate) fn outline(model: &TreeModel) -> Vec<String> {
    let mut lines = Vec::new();
    walk(model, NodeId::ROOT, 0, &mut lines);
    lines
}

fn walk(model: &TreeModel, parent: NodeId, depth: usize, lines: &mut Vec<String>) {
    for index in 0..model.child_count(parent) {
        let child = model.child_at(parent, index);
        let indent = "  ".repeat(depth);
        let name = model.name(child);
        match model.kind(child) {
            NodeKind::File => {
                let size = model.file_size(child).expect("a file has a size");
                lines.push(format!("{indent}{name} {size} B"));
            }
            kind => {
                let agg = model.folder(child).expect("a folder has an aggregate");
                let mark = if kind == NodeKind::Opaque { "/?" } else { "/" };
                lines.push(format!(
                    "{indent}{name}{mark} {} files {} B",
                    agg.files_total, agg.bytes_total
                ));
                walk(model, child, depth + 1, lines);
            }
        }
    }
}

/// The node at `labels`, one per level from the top (`["photos", "a.jpg"]`).
pub(crate) fn node_at(model: &TreeModel, labels: &[&str]) -> NodeId {
    let mut at = NodeId::ROOT;
    for label in labels {
        at = (0..model.child_count(at))
            .map(|index| model.child_at(at, index))
            .find(|child| model.name(*child) == *label)
            .unwrap_or_else(|| panic!("no {label:?} under {:?}", model.name(at)));
    }
    at
}
