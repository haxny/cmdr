//! Flattening the tree into the window of rows one view shows.
//!
//! The view holds no tree: it sends its hand toggles and where it's scrolled,
//! and gets back at most [`MAX_LIMIT`] rows. Which folders are open is the
//! backend's call (`Layout::new`), so following the work needs no state on
//! either side.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::model::{LeafState, NodeId, TreeModel, TreeRowState};
use crate::file_system::write_operations::types::{ProgressAxis, WriteOperationError};

/// What a view asks for.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferTreeViewRequest {
    /// Rows the user expanded by hand.
    pub(crate) expanded: Vec<u32>,
    /// Rows the user collapsed by hand.
    pub(crate) collapsed: Vec<u32>,
    pub(crate) window: TreeWindow,
    pub(crate) limit: u32,
}

/// Where the window sits.
#[derive(Debug, Clone, Copy, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum TreeWindow {
    /// Around the work: `lead` rows above the topmost active row.
    Follow { lead: u32 },
    /// From a flat row index the user scrolled to.
    At { offset: u32 },
}

/// One window of rows.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferTreeView {
    /// `false` while the operation scans (no tree yet): every other field is
    /// empty. Also `false` for a tree a bug turned off, which the transfer
    /// outlives.
    pub(crate) ready: bool,
    /// Bumps on every mutation: equal means nothing changed.
    pub(crate) revision: u64,
    /// Rows under the effective expansion.
    pub(crate) total_rows: u32,
    /// Where `rows` starts.
    pub(crate) offset: u32,
    pub(crate) rows: Vec<TransferTreeRow>,
    /// Flat index of the topmost active row (or the folder row hiding it).
    pub(crate) focus_row: Option<u32>,
    /// The axis every folder bar fills along: the operation's bottleneck,
    /// the same answer its `write-progress` events carry.
    pub(crate) axis: ProgressAxis,
}

impl TransferTreeView {
    /// The answer while there's no tree to show.
    pub(crate) fn not_ready(axis: ProgressAxis) -> Self {
        Self {
            ready: false,
            revision: 0,
            total_rows: 0,
            offset: 0,
            rows: Vec::new(),
            focus_row: None,
            axis,
        }
    }

    pub(super) fn finished(self, revision: u64, axis: ProgressAxis) -> Self {
        Self { revision, axis, ..self }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TreeRowKind {
    File,
    Folder,
}

/// One row.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferTreeRow {
    /// The node id; for a compact chain, the deepest folder's.
    pub(crate) id: u32,
    pub(crate) depth: u16,
    /// The name, or a compact chain's names joined with `/`.
    pub(crate) label: String,
    pub(crate) kind: TreeRowKind,
    pub(crate) state: TreeRowState,
    pub(crate) expanded: bool,
    pub(crate) has_children: bool,
    /// 1-based, for `aria-posinset`.
    pub(crate) pos_in_set: u32,
    /// For `aria-setsize`.
    pub(crate) set_size: u32,
    /// A folder's files; `0/1` or `1/1` for a file.
    pub(crate) files_done: u64,
    pub(crate) files_total: u64,
    /// In-flight bytes included.
    pub(crate) bytes_done: u64,
    pub(crate) bytes_total: u64,
    /// Descendant counts (folders).
    pub(crate) skipped: u32,
    pub(crate) failed: u32,
    /// Failed files only.
    pub(crate) failure: Option<WriteOperationError>,
}

/// The most rows one response carries.
pub(crate) const MAX_LIMIT: u32 = 200;

/// The most hand toggles of each kind a request may carry; the frontend drops
/// its oldest beyond this.
pub(crate) const MAX_TOGGLES: usize = 2_000;

/// One window of rows under the effective expansion.
///
/// ❗ O(|expansion| + limit), ❌ never O(children): row counts are summed over
/// the expanded folders only, and the window's first row is found by
/// descending through those sums. A flat list of a million files answers in
/// microseconds (`budget_tests.rs`).
pub(crate) fn flatten(model: &TreeModel, request: &TransferTreeViewRequest) -> TransferTreeView {
    let limit = request.limit.min(MAX_LIMIT);
    let layout = Layout::new(model, request);
    let total_rows = layout.listing_size(NodeId::ROOT);
    let focus_row = layout.focus_row();
    let offset = match request.window {
        TreeWindow::At { offset } => offset.min(total_rows),
        TreeWindow::Follow { lead } => {
            let wanted = focus_row.map_or(0, |focus| focus.saturating_sub(lead));
            wanted.min(total_rows.saturating_sub(limit))
        }
    };
    let rows = if offset < total_rows {
        layout.rows_from(offset, limit)
    } else {
        Vec::new()
    };
    TransferTreeView {
        ready: true,
        revision: 0,
        total_rows,
        offset,
        rows,
        focus_row,
        axis: ProgressAxis::Bytes,
    }
}

/// The effective expansion for one request, and the row counts it makes.
///
/// A row is keyed by its node, or for a compact chain (`a/b/c`: folders with
/// one folder child each) by the chain's DEEPEST folder, whose children are
/// the row's children and whose id a toggle on the row carries.
struct Layout<'m> {
    model: &'m TreeModel,
    /// Every open row (expanded, under open rows only), with the rows its
    /// listing adds below it.
    sizes: HashMap<NodeId, u32>,
    /// Per parent row, the root included: its open child rows as `(position
    /// of the row's head among the parent's children, row key)`, by position.
    open_children: HashMap<NodeId, Vec<(u32, NodeId)>>,
    in_flight: HashMap<u32, u64>,
    focus: Vec<NodeId>,
}

impl<'m> Layout<'m> {
    fn new(model: &'m TreeModel, request: &TransferTreeViewRequest) -> Self {
        let focus = model.focus();
        let known = |id: &&u32| (**id as usize) < model.len() && **id != NodeId::ROOT.get();
        let collapsed: HashSet<NodeId> = request
            .collapsed
            .iter()
            .take(MAX_TOGGLES)
            .filter(known)
            .map(|id| row_key(model, NodeId(*id)))
            .collect();
        // The folders the work is in, plus the ones the user opened, minus the
        // ones the user closed: auto-expand, auto-collapse, and "a row the user
        // toggled stays put" are this one set expression.
        let mut expanded: HashSet<NodeId> = request
            .expanded
            .iter()
            .take(MAX_TOGGLES)
            .filter(known)
            .map(|id| row_key(model, NodeId(*id)))
            .collect();
        for file in &focus {
            let mut at = model.parent(*file);
            while let Some(folder) = at.filter(|folder| *folder != NodeId::ROOT) {
                expanded.insert(row_key(model, folder));
                at = model.parent(folder);
            }
        }
        expanded.retain(|key| !collapsed.contains(key) && model.child_count(*key) > 0);
        let open: Vec<NodeId> = expanded
            .iter()
            .copied()
            .filter(|key| row_ancestors(model, *key).all(|ancestor| expanded.contains(&ancestor)))
            .collect();

        let mut open_children: HashMap<NodeId, Vec<(u32, NodeId)>> = HashMap::new();
        for key in &open {
            let position = model.position_in_parent(chain_head(model, *key));
            open_children
                .entry(parent_row(model, *key))
                .or_default()
                .push((position, *key));
        }
        for children in open_children.values_mut() {
            children.sort_unstable();
        }
        let mut layout = Self {
            model,
            sizes: HashMap::with_capacity(open.len()),
            open_children,
            in_flight: model.in_flight_by_folder(),
            focus,
        };
        // Deepest first, so a row's open children are sized before it is.
        let mut by_depth: Vec<(usize, NodeId)> = open.iter().map(|key| (depth_of(model, *key), *key)).collect();
        by_depth.sort_unstable_by_key(|(depth, _)| std::cmp::Reverse(*depth));
        for (_, key) in by_depth {
            let size = layout.listing_size(key);
            layout.sizes.insert(key, size);
        }
        layout
    }

    /// The rows `key`'s listing takes: one per child, plus every open child's
    /// own listing. For the root, every row there is.
    fn listing_size(&self, key: NodeId) -> u32 {
        let nested: u32 = self
            .open_children
            .get(&key)
            .map_or(0, |children| children.iter().map(|(_, child)| self.sizes[child]).sum());
        self.model.child_count(key) + nested
    }

    fn is_open(&self, key: NodeId) -> bool {
        self.sizes.contains_key(&key)
    }

    /// The rows before the child at `position` in `parent`'s listing.
    fn rows_before(&self, parent: NodeId, position: u32) -> u32 {
        let nested: u32 = self.open_children.get(&parent).map_or(0, |children| {
            children
                .iter()
                .take_while(|(at, _)| *at < position)
                .map(|(_, child)| self.sizes[child])
                .sum()
        });
        position + nested
    }

    /// The flat index of the topmost focus file's row, or of the shut folder
    /// row hiding it.
    fn focus_row(&self) -> Option<u32> {
        self.focus.iter().map(|file| self.visible_index_of(*file)).min()
    }

    /// The flat index of `node`'s row, or of the outermost shut row above it.
    fn visible_index_of(&self, node: NodeId) -> u32 {
        let key = row_key(self.model, node);
        let mut path: Vec<NodeId> = row_ancestors(self.model, key).collect();
        path.reverse();
        path.push(key);
        let mut index = 0;
        let mut parent = NodeId::ROOT;
        for (level, row) in path.into_iter().enumerate() {
            if level > 0 {
                index += 1;
            }
            index += self.rows_before(parent, self.model.position_in_parent(chain_head(self.model, row)));
            if !self.is_open(row) {
                break;
            }
            parent = row;
        }
        index
    }

    /// Up to `limit` rows from flat index `offset` on.
    fn rows_from(&self, offset: u32, limit: u32) -> Vec<TransferTreeRow> {
        // Descend to the row at `offset`, keeping each level's listing position.
        let mut stack: Vec<(NodeId, u32)> = Vec::new();
        let mut parent = NodeId::ROOT;
        let mut target = offset;
        'descend: loop {
            let mut passed = 0;
            for (position, child) in self.open_children.get(&parent).into_iter().flatten() {
                let start = position + passed;
                if target < start {
                    break;
                }
                if target == start {
                    stack.push((parent, *position));
                    break 'descend;
                }
                let size = self.sizes[child];
                if target <= start + size {
                    stack.push((parent, *position));
                    parent = *child;
                    target -= start + 1;
                    continue 'descend;
                }
                passed += size;
            }
            stack.push((parent, target - passed));
            break;
        }

        // Then walk forward in tree order: into an open row, else to the next
        // sibling, else back up.
        let mut rows = Vec::with_capacity(limit as usize);
        while rows.len() < limit as usize {
            let Some(&(parent, position)) = stack.last() else {
                break;
            };
            if position >= self.model.child_count(parent) {
                stack.pop();
                if let Some(above) = stack.last_mut() {
                    above.1 += 1;
                }
                continue;
            }
            let key = row_key(self.model, self.model.child_at(parent, position));
            rows.push(self.row(key, (stack.len() - 1) as u16, position, parent));
            if self.is_open(key) {
                stack.push((key, 0));
            } else if let Some(here) = stack.last_mut() {
                here.1 += 1;
            }
        }
        rows
    }

    fn row(&self, key: NodeId, depth: u16, position: u32, parent: NodeId) -> TransferTreeRow {
        let model = self.model;
        let (kind, state, files, bytes, skipped, failed) = match model.folder(key) {
            Some(agg) => {
                let in_flight = self.in_flight.get(&key.get()).copied();
                let bytes_done = (agg.bytes_done + in_flight.unwrap_or(0)).min(agg.bytes_total);
                (
                    TreeRowKind::Folder,
                    model.folder_state(key, in_flight.is_some()),
                    (u64::from(agg.files_done), u64::from(agg.files_total)),
                    (bytes_done, agg.bytes_total),
                    agg.skipped,
                    agg.failed,
                )
            }
            None => {
                let size = model.file_size(key).unwrap_or(0);
                let (state, files_done, bytes_done) = match model.leaf_state(key) {
                    LeafState::Waiting => (TreeRowState::Waiting, 0, 0),
                    LeafState::Active => (
                        TreeRowState::Active,
                        0,
                        model.in_flight_bytes(key).unwrap_or(0).min(size),
                    ),
                    LeafState::Done => (TreeRowState::Done, 1, size),
                    LeafState::Skipped => (TreeRowState::Skipped, 1, size),
                    LeafState::Failed => (TreeRowState::Failed, 0, 0),
                };
                (TreeRowKind::File, state, (files_done, 1), (bytes_done, size), 0, 0)
            }
        };
        TransferTreeRow {
            id: key.get(),
            depth,
            label: label(model, key),
            kind,
            state,
            expanded: self.is_open(key),
            has_children: model.child_count(key) > 0,
            pos_in_set: position + 1,
            set_size: model.child_count(parent),
            files_done: files.0,
            files_total: files.1,
            bytes_done: bytes.0,
            bytes_total: bytes.1,
            skipped,
            failed,
            failure: model.failure(key).cloned(),
        }
    }
}

/// The row a node shows as: itself, or for a folder heading a compact chain,
/// the chain's deepest folder.
fn row_key(model: &TreeModel, node: NodeId) -> NodeId {
    let mut at = node;
    while let Some(only) = only_folder_child(model, at) {
        at = only;
    }
    at
}

/// The first folder of `key`'s compact chain (`key` itself outside one).
fn chain_head(model: &TreeModel, key: NodeId) -> NodeId {
    let mut head = key;
    while let Some(parent) = model.parent(head) {
        if only_folder_child(model, parent) != Some(head) {
            break;
        }
        head = parent;
    }
    head
}

/// The row whose listing holds `key`'s row: a row key, or the root.
fn parent_row(model: &TreeModel, key: NodeId) -> NodeId {
    model.parent(chain_head(model, key)).unwrap_or(NodeId::ROOT)
}

/// `key`'s ancestor rows, nearest first, the root left out.
fn row_ancestors(model: &TreeModel, key: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    std::iter::successors(Some(parent_row(model, key)), move |row| Some(parent_row(model, *row)))
        .take_while(|row| *row != NodeId::ROOT)
}

/// A folder's one child, when it has exactly one and that child is a folder:
/// the link that compacts two folders into one row. ❌ A file never compacts,
/// and nothing compacts into the root.
fn only_folder_child(model: &TreeModel, folder: NodeId) -> Option<NodeId> {
    if folder == NodeId::ROOT || !model.kind(folder).is_folder() || model.child_count(folder) != 1 {
        return None;
    }
    let only = model.child_at(folder, 0);
    model.kind(only).is_folder().then_some(only)
}

fn depth_of(model: &TreeModel, node: NodeId) -> usize {
    std::iter::successors(model.parent(node), |at| model.parent(*at)).count()
}

/// A row's label: its name, or a compact chain's names joined with `/`.
fn label(model: &TreeModel, key: NodeId) -> String {
    let head = chain_head(model, key);
    let mut names = vec![model.name(key)];
    let mut at = key;
    while at != head {
        at = model.parent(at).expect("a chain's head is above its key");
        names.push(model.name(at));
    }
    names.reverse();
    names.join("/")
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
