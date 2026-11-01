//! A confirmed plan as ordered steps: make folders, then per landing folder
//! set aside what its copies replace and copy, then remove. Pure; order and
//! grouping in `DETAILS.md` § Run.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::overrides::refusal;
use crate::{
    Action, EntryKey, KeepDestination, KeepRefusal, Place, Plan, PlanItem, RunContext, Side, keep_destination, kept_dir,
};

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
    /// Move the `side` entries the next copy replaces into the staging folder
    /// on their own volume (`RunContext::staging`), right before that copy.
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
    let to = from.other();
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

/// Rows that may act: a copy or removal the plan's own mode, the row's
/// category, and both volumes allow (`refusal`), whose entry is there. Checked
/// again here so a plan built with an illegal action (Mirror writing to the
/// source, say) still never acts on it.
fn actable(plan: &Plan, item: &PlanItem) -> bool {
    let acts = item.action.is_copy() || item.action.is_removal();
    acts && refusal(plan.mode, item, item.action, &plan.left_caps, &plan.right_caps).is_none()
}

/// One landing folder's copies, and the files they replace by staging folder.
#[derive(Default)]
struct CopyGroup {
    keeps: BTreeMap<PathBuf, Vec<u32>>,
    items: Vec<u32>,
}

#[derive(Default)]
struct Groups {
    make: BTreeMap<(Place, Side), BTreeSet<PathBuf>>,
    copy: BTreeMap<(Side, PathBuf), CopyGroup>,
    remove: BTreeMap<(Side, KeepDestination), Vec<u32>>,
    refused: Vec<(u32, KeepRefusal)>,
}

impl Groups {
    fn make_dir(&mut self, place: Place, side: Side, dir: PathBuf) {
        self.make.entry((place, side)).or_default().insert(dir);
    }

    /// Where `side`'s entry `rel_path` goes; a folder destination is made first.
    fn keep_place(&mut self, ctx: &RunContext, side: Side, rel_path: &str) -> Result<KeepDestination, KeepRefusal> {
        match keep_destination(&ctx.keep, ctx.caps(side), ctx.versions(side), rel_path) {
            KeepDestination::Refused(refusal) => Err(refusal),
            KeepDestination::Folder { place, dir } => {
                self.make_dir(place, side, dir.clone());
                Ok(KeepDestination::Folder { place, dir })
            }
            KeepDestination::Trash => Ok(KeepDestination::Trash),
        }
    }

    /// The staging folder for a replaced entry; its final place (checked now,
    /// used after the copy lands) must exist too.
    fn stage(&mut self, ctx: &RunContext, side: Side, rel_path: &str) -> Result<PathBuf, KeepRefusal> {
        let staged = kept_dir(ctx.staging(side), rel_path)?;
        self.keep_place(ctx, side, rel_path)?;
        self.make_dir(Place::Target, side, staged.clone());
        Ok(staged)
    }

    fn add_copy(&mut self, plan: &Plan, item: &PlanItem, from: Side, ctx: &RunContext) {
        let Some(landing) = landing(plan, item, from, ctx) else {
            return;
        };
        let to = from.other();
        let staged = match item.entry(to).map(|old| self.stage(ctx, to, &old.rel_path)) {
            Some(Err(refusal)) => return self.refused.push((item.index, refusal)),
            Some(Ok(dir)) => Some(dir),
            None => None,
        };
        let made = self.make.entry((Place::Target, to)).or_default();
        made.extend(landing.missing);
        let group = self.copy.entry((from, landing.dest_dir)).or_default();
        if let Some(dir) = staged {
            group.keeps.entry(dir).or_default().push(item.index);
        }
        group.items.push(item.index);
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
        let mut steps: Vec<Step> = self
            .make
            .into_iter()
            .filter(|(_, dirs)| !dirs.is_empty())
            .map(|((place, side), dirs)| Step::MakeFolders {
                place,
                side,
                dirs: dirs.into_iter().collect(),
            })
            .collect();
        for ((from, dest_dir), group) in self.copy {
            steps.extend(group.keeps.into_iter().map(|(dir, items)| Step::KeepAside {
                side: from.other(),
                items,
                to: KeepDestination::Folder {
                    place: Place::Target,
                    dir,
                },
            }));
            steps.push(Step::Copy {
                from,
                dest_dir,
                items: group.items,
            });
        }
        steps.extend(
            self.remove
                .into_iter()
                .map(|((side, to), items)| Step::Remove { side, items, to }),
        );
        StepPlan {
            steps,
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
        if !actable(plan, item) {
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
