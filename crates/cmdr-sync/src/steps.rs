//! A confirmed plan as ordered steps: make folders, keep aside what copies
//! replace, copy, then remove. Pure; order and grouping in `DETAILS.md` § Run.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::{
    Action, Category, EntryKey, KeepDestination, KeepPolicy, KeepRefusal, Place, Plan, PlanItem, Side, SyncCaps,
    VersionsRoot, keep_destination,
};

/// Everything a run needs beyond the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunContext {
    /// The left folder.
    pub left_root: PathBuf,
    /// The right folder.
    pub right_root: PathBuf,
    /// What the left volume can do now.
    pub left_caps: SyncCaps,
    /// What the right volume can do now.
    pub right_caps: SyncCaps,
    /// How replaced and removed items are kept.
    pub keep: KeepPolicy,
    /// This run's versions folder for items kept from the left.
    pub versions_left: VersionsRoot,
    /// This run's versions folder for items kept from the right.
    pub versions_right: VersionsRoot,
}

impl RunContext {
    /// The root of `side`.
    pub fn root(&self, side: Side) -> &Path {
        match side {
            Side::Left => &self.left_root,
            Side::Right => &self.right_root,
        }
    }

    /// What `side`'s volume can do.
    pub fn caps(&self, side: Side) -> &SyncCaps {
        match side {
            Side::Left => &self.left_caps,
            Side::Right => &self.right_caps,
        }
    }

    /// The versions folder for items kept from `side`.
    pub fn versions(&self, side: Side) -> &VersionsRoot {
        match side {
            Side::Left => &self.versions_left,
            Side::Right => &self.versions_right,
        }
    }
}

/// One group of like operations, by plan row index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Make these folders (and any missing parents): versions folders, and
    /// target folders of split one-sided folders whose children are copied.
    MakeFolders {
        /// Which volume.
        place: Place,
        /// The side they are made for.
        side: Side,
        /// Parents before children.
        dirs: Vec<PathBuf>,
    },
    /// Move the `side` entries these copies would replace out of the way.
    KeepAside {
        /// The side whose entries go.
        side: Side,
        /// The rows.
        items: Vec<u32>,
        /// Where they go.
        to: KeepDestination,
    },
    /// Copy these rows' `from` entries into `dest_dir` on the other side.
    Copy {
        /// The side copied from.
        from: Side,
        /// The folder they land in.
        dest_dir: PathBuf,
        /// The rows.
        items: Vec<u32>,
    },
    /// Remove these rows' `side` entries by keeping them at `to`.
    Remove {
        /// The side losing them.
        side: Side,
        /// The rows.
        items: Vec<u32>,
        /// Where they go.
        to: KeepDestination,
    },
}

/// The steps of a run, and rows left alone because they can't be kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepPlan {
    /// In order.
    pub steps: Vec<Step>,
    /// Rows that replace or remove something the policy can't keep.
    pub refused: Vec<(u32, KeepRefusal)>,
}

/// The side across from `side`.
pub(crate) fn other(side: Side) -> Side {
    match side {
        Side::Left => Side::Right,
        Side::Right => Side::Left,
    }
}

/// Where a copy lands on the other side.
pub(crate) struct Landing {
    /// The folder it lands in.
    pub(crate) dest_dir: PathBuf,
    /// Its name there (the source's spelling).
    pub(crate) name: String,
    /// Folders on the way that the target doesn't have, parents first.
    pub(crate) missing: Vec<PathBuf>,
}

/// Each folder on the way is the target's own spelling where the target has
/// it, else the source's (and then it has to be made).
pub(crate) fn landing(plan: &Plan, item: &PlanItem, from: Side, ctx: &RunContext) -> Option<Landing> {
    let source = item.entry(from)?;
    let to = other(from);
    let EntryKey::Name(keys) = &item.key;
    let names: Vec<&str> = source.rel_path.split('/').collect();
    if names.len() != keys.len() {
        return None;
    }
    let root = ctx.root(to);
    let mut rel = String::new();
    let mut missing = Vec::new();
    for depth in 1..keys.len() {
        let head = EntryKey::Name(keys[..depth].to_vec());
        let there = plan.heads.get(&head).and_then(|head| head.on(to));
        rel = match there {
            Some(spelled) if missing.is_empty() => spelled.to_owned(),
            _ => {
                let next = if rel.is_empty() {
                    names[depth - 1].to_owned()
                } else {
                    format!("{rel}/{}", names[depth - 1])
                };
                missing.push(root.join(&next));
                next
            }
        };
    }
    Some(Landing {
        dest_dir: if rel.is_empty() {
            root.to_path_buf()
        } else {
            root.join(rel)
        },
        name: names.last()?.to_string(),
        missing,
    })
}

/// Rows that may act: a copy or removal whose entry is there, never on a row
/// the planner only lets skip.
fn actable(item: &PlanItem) -> bool {
    let partial = matches!(
        item.verdict.category,
        Category::NameClash | Category::TypeClash | Category::Unreadable
    );
    let acts = item.action.is_copy() || item.action.is_removal();
    acts && !partial && item.action.needs().is_some_and(|side| item.entry(side).is_some())
}

#[derive(Default)]
struct Groups {
    make: BTreeMap<(Place, Side), BTreeSet<PathBuf>>,
    keep: BTreeMap<(Side, KeepDestination), Vec<u32>>,
    copy: BTreeMap<(Side, PathBuf), Vec<u32>>,
    remove: BTreeMap<(Side, KeepDestination), Vec<u32>>,
    refused: Vec<(u32, KeepRefusal)>,
}

impl Groups {
    /// Where `side`'s entry `rel_path` goes; a folder destination is made first.
    fn keep_place(&mut self, ctx: &RunContext, side: Side, rel_path: &str) -> Result<KeepDestination, KeepRefusal> {
        match keep_destination(&ctx.keep, ctx.caps(side), ctx.versions(side), rel_path) {
            KeepDestination::Refused(refusal) => Err(refusal),
            KeepDestination::Folder { place, dir } => {
                self.make.entry((place, side)).or_default().insert(dir.clone());
                Ok(KeepDestination::Folder { place, dir })
            }
            KeepDestination::Trash => Ok(KeepDestination::Trash),
        }
    }

    fn add_copy(&mut self, plan: &Plan, item: &PlanItem, from: Side, ctx: &RunContext) {
        let Some(landing) = landing(plan, item, from, ctx) else {
            return;
        };
        let to = other(from);
        if let Some(old) = item.entry(to) {
            match self.keep_place(ctx, to, &old.rel_path) {
                Ok(to_place) => self.keep.entry((to, to_place)).or_default().push(item.index),
                Err(refusal) => return self.refused.push((item.index, refusal)),
            }
        }
        let made = self.make.entry((Place::Target, to)).or_default();
        made.extend(landing.missing);
        self.copy.entry((from, landing.dest_dir)).or_default().push(item.index);
    }

    fn add_removal(&mut self, item: &PlanItem, side: Side, ctx: &RunContext) {
        let Some(entry) = item.entry(side) else {
            return;
        };
        match self.keep_place(ctx, side, &entry.rel_path) {
            Ok(to) => self.remove.entry((side, to)).or_default().push(item.index),
            Err(refusal) => self.refused.push((item.index, refusal)),
        }
    }

    fn into_plan(self) -> StepPlan {
        let make = self
            .make
            .into_iter()
            .filter(|(_, dirs)| !dirs.is_empty())
            .map(|((place, side), dirs)| Step::MakeFolders {
                place,
                side,
                dirs: dirs.into_iter().collect(),
            });
        let keep = self
            .keep
            .into_iter()
            .map(|((side, to), items)| Step::KeepAside { side, items, to });
        let copy = self
            .copy
            .into_iter()
            .map(|((from, dest_dir), items)| Step::Copy { from, dest_dir, items });
        let remove = self
            .remove
            .into_iter()
            .map(|((side, to), items)| Step::Remove { side, items, to });
        StepPlan {
            steps: make.chain(keep).chain(copy).chain(remove).collect(),
            refused: self.refused,
        }
    }
}

/// The steps that carry out the rows of `plan` listed in `run` (from `gate`).
/// Rows that don't act, or aren't in the plan, are left out.
pub fn steps(plan: &Plan, run: &[u32], ctx: &RunContext) -> StepPlan {
    let rows: BTreeSet<u32> = run.iter().copied().collect();
    let mut groups = Groups::default();
    for item in rows.iter().filter_map(|&index| plan.items.get(index as usize)) {
        if !actable(item) {
            continue;
        }
        match item.action {
            Action::CopyRight => groups.add_copy(plan, item, Side::Left, ctx),
            Action::CopyLeft => groups.add_copy(plan, item, Side::Right, ctx),
            Action::RemoveRight => groups.add_removal(item, Side::Right, ctx),
            Action::RemoveLeft => groups.add_removal(item, Side::Left, ctx),
            Action::Equal | Action::Skip | Action::Undecided => {}
        }
    }
    groups.into_plan()
}
