//! The planner: what each row of the grid does, per mode. Pure and
//! deterministic; rules and rationale in `DETAILS.md` § Planner.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::overrides::refusal;
use crate::{
    Category, CompareBy, CompareRules, ContentJob, ContentState, Entry, EntryKey, EntryKind, Rung, Side, Snapshot,
    SyncCaps, Verdict, classify,
};

/// How the two sides should end up. Left is the source in `Update` and `Mirror`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// Show the differences, do nothing by default.
    CompareOnly,
    /// Copy missing and newer files both ways; never removes.
    BothWays,
    /// Copy missing and newer files left to right; never removes.
    Update,
    /// Make the right side a copy of the left: copies, replaces, and removes on
    /// the right.
    Mirror,
}

impl Mode {
    /// Every mode, in the order the dialog lists them.
    pub const ALL: [Self; 4] = [Self::CompareOnly, Self::BothWays, Self::Update, Self::Mirror];
}

/// What one row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    /// `→` copy (or replace) left to right.
    CopyRight,
    /// `←` copy (or replace) right to left.
    CopyLeft,
    /// `×→` remove on the right (to the Trash or the versions folder).
    RemoveRight,
    /// `←×` remove on the left (overrides only).
    RemoveLeft,
    /// `=` nothing to do, the sides are equal.
    Equal,
    /// `·` leave both sides alone.
    Skip,
    /// `?` needs a person.
    Undecided,
}

impl Action {
    /// The side this action writes to, if it writes at all.
    pub fn target(self) -> Option<Side> {
        match self {
            Self::CopyRight | Self::RemoveRight => Some(Side::Right),
            Self::CopyLeft | Self::RemoveLeft => Some(Side::Left),
            Self::Equal | Self::Skip | Self::Undecided => None,
        }
    }

    /// The side whose entry must exist for this action: the copy's source, or
    /// what a removal removes.
    pub fn needs(self) -> Option<Side> {
        match self {
            Self::CopyRight | Self::RemoveLeft => Some(Side::Left),
            Self::CopyLeft | Self::RemoveRight => Some(Side::Right),
            Self::Equal | Self::Skip | Self::Undecided => None,
        }
    }

    /// Whether this action removes something.
    pub fn is_removal(self) -> bool {
        matches!(self, Self::RemoveRight | Self::RemoveLeft)
    }

    /// Whether this action copies something.
    pub fn is_copy(self) -> bool {
        matches!(self, Self::CopyRight | Self::CopyLeft)
    }
}

/// One row of the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    /// Its position in `Plan::items`.
    pub index: u32,
    /// The pairing key.
    pub key: EntryKey,
    /// The left entry, absent when missing or a name clash on the left.
    pub left: Option<Entry>,
    /// The right entry, absent when missing or a name clash on the right.
    pub right: Option<Entry>,
    /// How the sides relate.
    pub verdict: Verdict,
    /// Where a byte compare stands.
    pub content: ContentState,
    /// What the mode does with this row.
    pub default_action: Action,
    /// What this row will do (the default unless changed by you).
    pub action: Action,
    /// The action replaces a newer file with an older one (a warning).
    pub older_overwrites_newer: bool,
    /// Bytes the action copies or removes; with no action, the larger side.
    pub bytes: u64,
}

impl PlanItem {
    /// The entry on `side`.
    pub fn entry(&self, side: Side) -> Option<&Entry> {
        match side {
            Side::Left => self.left.as_ref(),
            Side::Right => self.right.as_ref(),
        }
    }

    /// The person picked another action than the mode's.
    pub fn changed_by_you(&self) -> bool {
        self.action != self.default_action
    }

    /// Files an action on `side`'s entry covers: a folder's whole subtree, one
    /// for anything else, none when the side is empty.
    pub fn files_on(&self, side: Side) -> u64 {
        self.entry(side).map_or(0, |entry| match entry.subtree {
            Some(subtree) => subtree.files,
            None => 1,
        })
    }
}

/// A folder that heads rows instead of being one (on both sides, or split
/// into its children): each side's own spelling of its path, so a copy lands
/// in the folder the target already has, and a missing one is made first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HeadFolder {
    /// The left `rel_path`, absent when the folder is only on the right.
    pub left: Option<String>,
    /// The right `rel_path`, absent when the folder is only on the left.
    pub right: Option<String>,
}

impl HeadFolder {
    /// The spelling on `side`, if the folder is there.
    pub fn on(&self, side: Side) -> Option<&str> {
        match side {
            Side::Left => self.left.as_deref(),
            Side::Right => self.right.as_deref(),
        }
    }
}

/// Every row, and how they were decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The mode.
    pub mode: Mode,
    /// The rules used, already widened for both sides' time precision.
    pub rules: CompareRules,
    /// What the left volume can do (defaults never write where it can't).
    pub left_caps: SyncCaps,
    /// What the right volume can do.
    pub right_caps: SyncCaps,
    /// Sorted by key; `index` is the position.
    pub items: Vec<PlanItem>,
    /// Every folder that heads rows, by key. Every ancestor of a row is one.
    pub heads: BTreeMap<EntryKey, HeadFolder>,
}

/// What the last run left behind (phase 2). Uninhabited, so phase 1 can only
/// pass `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Baseline {}

/// What `plan` needs beyond the snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanOptions {
    /// The mode.
    pub mode: Mode,
    /// How pairs compare.
    pub rules: CompareRules,
}

/// The mode table (spec 6.4): what `mode` does with a `category`, before any
/// volume's capabilities are considered.
pub fn default_action(mode: Mode, category: Category) -> Action {
    use Category as C;
    match (mode, category) {
        (_, C::Equal) => Action::Equal,
        (Mode::BothWays | Mode::Update | Mode::Mirror, C::LeftOnly | C::NewerLeft) => Action::CopyRight,
        (Mode::BothWays, C::RightOnly | C::NewerRight) => Action::CopyLeft,
        (Mode::Mirror, C::NewerRight | C::Different) => Action::CopyRight,
        (Mode::Mirror, C::RightOnly) => Action::RemoveRight,
        _ => Action::Skip,
    }
}

/// The table's action, or `Skip` when it would write to a side that can't take
/// writes.
fn default_for(mode: Mode, category: Category, left: &SyncCaps, right: &SyncCaps) -> Action {
    let action = default_action(mode, category);
    let writable = match action.target() {
        Some(Side::Left) => left.can_write,
        Some(Side::Right) => right.can_write,
        None => true,
    };
    if writable { action } else { Action::Skip }
}

fn entry_bytes(entry: Option<&Entry>) -> u64 {
    entry.map_or(0, |entry| match entry.subtree {
        Some(subtree) => subtree.bytes,
        None => entry.facts.size.unwrap_or(0),
    })
}

/// Recomputes what follows from the action: the warning flag and the bytes.
pub(crate) fn settle(item: PlanItem) -> PlanItem {
    let older_overwrites_newer = matches!(
        (item.action, item.verdict.category),
        (Action::CopyRight, Category::NewerRight) | (Action::CopyLeft, Category::NewerLeft)
    );
    let bytes = match item.action.needs() {
        Some(side) => entry_bytes(item.entry(side)),
        None => entry_bytes(item.left.as_ref()).max(entry_bytes(item.right.as_ref())),
    };
    PlanItem {
        older_overwrites_newer,
        bytes,
        ..item
    }
}

fn planner_verdict(category: Category) -> Verdict {
    Verdict {
        category,
        rung: Rung::Presence,
        same_size: false,
    }
}

/// Whether `key` is an unreadable folder of `snap` or lies beneath one.
fn within_unreadable(key: &EntryKey, snap: &Snapshot) -> bool {
    let EntryKey::Name(parts) = key;
    (0..=parts.len()).any(|depth| snap.unreadable.contains_key(&EntryKey::Name(parts[..depth].to_vec())))
}

/// Whether any key of `map` lies strictly beneath `key`.
fn any_beneath<V>(map: &BTreeMap<EntryKey, V>, key: &EntryKey) -> bool {
    map.range(key.clone()..)
        .take_while(|(k, _)| k.is_within(key))
        .any(|(k, _)| k != key)
}

/// A folder the scan knows only partly (held back, unreadable, app-owned, or
/// clashing entries beneath it): never copied or removed whole. The unreadable
/// keys are checked directly too, so a miscounted subtree can't hide one.
fn is_non_atomic(entry: &Entry, snap: &Snapshot) -> bool {
    let partial = entry
        .subtree
        .is_some_and(|subtree| subtree.held_back > 0 || subtree.unreadable > 0 || subtree.app_owned > 0);
    partial || any_beneath(&snap.clashes, &entry.key) || any_beneath(&snap.unreadable, &entry.key)
}

/// What a key turns into.
enum Shape {
    /// A row; `covers` means everything beneath it belongs to this row.
    Row { verdict: Verdict, covers: bool },
    /// No row: a folder on both sides, or a partly known one-sided folder. Its
    /// children are rows of their own.
    Heads,
}

fn shape(key: &EntryKey, left: &Snapshot, right: &Snapshot, rules: &CompareRules, content: ContentState) -> Shape {
    if within_unreadable(key, left) || within_unreadable(key, right) {
        return Shape::Row {
            verdict: planner_verdict(Category::Unreadable),
            covers: true,
        };
    }
    if left.clashes.contains_key(key) || right.clashes.contains_key(key) {
        return Shape::Row {
            verdict: planner_verdict(Category::NameClash),
            covers: true,
        };
    }
    let (l, r) = (left.entries.get(key), right.entries.get(key));
    let one_sided = |entry: &Entry, snap: &Snapshot| {
        if entry.facts.kind == EntryKind::Folder && is_non_atomic(entry, snap) {
            Shape::Heads
        } else {
            let verdict = classify(l.map(|e| &e.facts), r.map(|e| &e.facts), rules, content);
            Shape::Row { verdict, covers: true }
        }
    };
    match (l, r) {
        (Some(l), Some(r)) => {
            let verdict = classify(Some(&l.facts), Some(&r.facts), rules, content);
            let both_folders = l.facts.kind == EntryKind::Folder && r.facts.kind == EntryKind::Folder;
            if both_folders {
                Shape::Heads
            } else {
                Shape::Row { verdict, covers: true }
            }
        }
        (Some(l), None) => one_sided(l, left),
        (None, Some(r)) => one_sided(r, right),
        (None, None) => Shape::Heads,
    }
}

/// `Pending` for an equal-size file pair under `Content` when both sides can be
/// read, else `NotAsked`.
fn initial_content(key: &EntryKey, left: &Snapshot, right: &Snapshot, rules: &CompareRules) -> ContentState {
    if rules.by != CompareBy::Content || !left.caps.can_read_content || !right.caps.can_read_content {
        return ContentState::NotAsked;
    }
    let (Some(l), Some(r)) = (left.entries.get(key), right.entries.get(key)) else {
        return ContentState::NotAsked;
    };
    let files = l.facts.kind == EntryKind::File && r.facts.kind == EntryKind::File;
    let same_size = matches!((l.facts.size, r.facts.size), (Some(a), Some(b)) if a == b);
    if files && same_size {
        ContentState::Pending
    } else {
        ContentState::NotAsked
    }
}

/// Plans `mode` over two snapshots. Pure; output sorted by key. Rules beyond
/// the mode table: `DETAILS.md` § Planner.
pub fn plan(left: &Snapshot, right: &Snapshot, _baseline: Option<&Baseline>, options: &PlanOptions) -> Plan {
    let rules = options.rules.widened_for(&left.caps, &right.caps);
    let keys: BTreeSet<&EntryKey> = left
        .entries
        .keys()
        .chain(right.entries.keys())
        .chain(left.clashes.keys())
        .chain(right.clashes.keys())
        .collect();
    let mut items = Vec::new();
    let mut heads = BTreeMap::new();
    let mut cover: Option<&EntryKey> = None;
    for key in keys {
        if cover.is_some_and(|covering| key.is_within(covering)) {
            continue;
        }
        cover = None;
        let content = initial_content(key, left, right, &rules);
        let Shape::Row { verdict, covers } = shape(key, left, right, &rules, content) else {
            let spelling = |snap: &Snapshot| snap.entries.get(key).map(|entry| entry.rel_path.clone());
            heads.insert(
                key.clone(),
                HeadFolder {
                    left: spelling(left),
                    right: spelling(right),
                },
            );
            continue;
        };
        if covers {
            cover = Some(key);
        }
        let default_action = default_for(options.mode, verdict.category, &left.caps, &right.caps);
        items.push(settle(PlanItem {
            index: items.len() as u32,
            key: key.clone(),
            left: left.entries.get(key).cloned(),
            right: right.entries.get(key).cloned(),
            verdict,
            content,
            default_action,
            action: default_action,
            older_overwrites_newer: false,
            bytes: 0,
        }));
    }
    Plan {
        mode: options.mode,
        rules,
        left_caps: left.caps,
        right_caps: right.caps,
        items,
        heads,
    }
}

/// A plan with byte-compare answers folded in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reclassified {
    /// The new plan.
    pub plan: Plan,
    /// Rows whose action you had changed but the answer made illegal (for
    /// example `?` on a row that turned out equal): reset to their new default.
    pub reset: Vec<u32>,
}

/// One answered row, or `None` when the row wasn't waiting for this answer.
fn answered(plan: &Plan, item: &PlanItem, content: ContentState) -> Option<(PlanItem, bool)> {
    let waiting = item.content == ContentState::Pending
        && !matches!(
            item.verdict.category,
            Category::NameClash | Category::Unreadable | Category::TypeClash
        );
    let (l, r) = (item.left.as_ref()?, item.right.as_ref()?);
    if !waiting {
        return None;
    }
    let verdict = classify(Some(&l.facts), Some(&r.facts), &plan.rules, content);
    let default_action = default_for(plan.mode, verdict.category, &plan.left_caps, &plan.right_caps);
    let answered = PlanItem {
        verdict,
        content,
        default_action,
        action: default_action,
        ..item.clone()
    };
    let kept = PlanItem {
        action: item.action,
        ..answered.clone()
    };
    let keep = item.changed_by_you();
    let legal = refusal(plan.mode, &kept, kept.action, &plan.left_caps, &plan.right_caps).is_none();
    match (keep, legal) {
        (true, true) => Some((settle(kept), false)),
        (true, false) => Some((settle(answered), true)),
        (false, _) => Some((settle(answered), false)),
    }
}

/// Folds byte-compare answers into the plan. Only rows waiting for an answer
/// (`Pending`) change. A row whose action you changed keeps it while it stays
/// legal; the rest follow their new default.
pub fn reclassify(plan: &Plan, results: &[(u32, ContentState)]) -> Reclassified {
    let mut items = plan.items.clone();
    let mut reset = Vec::new();
    for &(index, content) in results {
        let Some(item) = items.get(index as usize) else {
            continue;
        };
        let Some((next, was_reset)) = answered(plan, item, content) else {
            continue;
        };
        if was_reset {
            reset.push(index);
        }
        items[index as usize] = next;
    }
    Reclassified {
        plan: Plan { items, ..plan.clone() },
        reset,
    }
}

/// The byte compares the plan is waiting for: every row whose content is
/// `Pending`, with each side's own spelling under its root.
pub fn content_jobs(plan: &Plan, left_root: &Path, right_root: &Path) -> Vec<ContentJob> {
    plan.items
        .iter()
        .filter(|item| item.content == ContentState::Pending)
        .filter_map(|item| {
            let (l, r) = (item.left.as_ref()?, item.right.as_ref()?);
            Some(ContentJob {
                item: item.index,
                left: left_root.join(&l.rel_path),
                right: right_root.join(&r.rel_path),
                size: l.facts.size?,
            })
        })
        .collect()
}
