//! Safety guards: what a plan may not do without a second yes, or at all.
//! Semantics in `DETAILS.md` § Guards.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Action, Category, EntryKey, EntryKind, HOUR_SHIFT_SECS, NameRule, Plan, PlanItem, Side, Snapshot};

/// Removals above this share of a side's files block (rclone bisync's default).
pub const DEFAULT_MAX_REMOVE_PERCENT: u8 = 50;
/// A clock skew is reported when at least this share of pairs agree on it.
const SKEW_SHARE_PERCENT: u64 = 90;
/// Clock offsets come in steps of a quarter hour (time zones, DST)…
const SKEW_STEP_SECS: i64 = 900;
/// …up to 26 hours (the widest time-zone gap plus DST).
const SKEW_MAX_SECS: i64 = 26 * 3600;
/// Fewer comparable pairs than this say nothing about a clock.
const SKEW_MIN_PAIRS: u64 = 10;

/// The guard settings of a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GuardSettings {
    /// Removals on a side above this percentage of its files block.
    pub max_remove_percent: u8,
    /// Removals on a side above this count block; 0 turns it off.
    pub max_remove_count: u32,
    /// A file name that must exist in both roots, against unmounted paths.
    pub sentinel: Option<String>,
}

impl Default for GuardSettings {
    fn default() -> Self {
        Self {
            max_remove_percent: DEFAULT_MAX_REMOVE_PERCENT,
            max_remove_count: 0,
            sentinel: None,
        }
    }
}

/// A guard that fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardBlock {
    /// Too many removals on `side`; "Remove anyway" releases them.
    MaxRemove {
        /// The side losing files.
        side: Side,
        /// Files the plan removes there.
        removals: u64,
        /// Files the side has.
        target_files: u64,
    },
    /// `side`'s root listed empty: every removal is held, and copies into it
    /// wait for "Copy anyway".
    EmptyRoot {
        /// The empty side.
        side: Side,
    },
    /// The sentinel file is missing on `side`: nothing runs.
    SentinelMissing {
        /// The side without it.
        side: Side,
    },
}

/// A second yes from the confirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Acknowledgement {
    /// Releases `MaxRemove`.
    RemoveAnyway,
    /// Releases copies into an empty root.
    CopyIntoEmpty,
}

/// The right side's clock seems to run off by a fixed step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClockSkew {
    /// How far the right side's times run ahead of the left's, in seconds
    /// (negative: behind). Its negation is the time offset that cancels it.
    pub offset_secs: i64,
    /// The share of paired files that show it.
    pub share_percent: u8,
}

/// Everything the confirm must show about a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardReport {
    /// Guards that hold actions back.
    pub blocks: Vec<GuardBlock>,
    /// Rows that replace a newer file with an older one (never a block).
    pub older_overwrites_newer: u64,
    /// A uniform clock offset, if one shows.
    pub skew: Option<ClockSkew>,
}

fn snapshot_of<'a>(side: Side, left: &'a Snapshot, right: &'a Snapshot) -> &'a Snapshot {
    match side {
        Side::Left => left,
        Side::Right => right,
    }
}

fn sentinel_missing(name: &str, side: &Snapshot, rule: NameRule) -> bool {
    let key = EntryKey::root().child(rule.key(name));
    side.entries
        .get(&key)
        .is_none_or(|entry| entry.facts.kind == EntryKind::Folder)
}

fn removed_files(plan: &Plan, side: Side) -> u64 {
    let removal = match side {
        Side::Left => Action::RemoveLeft,
        Side::Right => Action::RemoveRight,
    };
    plan.items
        .iter()
        .filter(|item| item.action == removal)
        .map(|item| item.files_on(side))
        .sum()
}

fn max_remove(plan: &Plan, side: Side, snap: &Snapshot, settings: &GuardSettings) -> Option<GuardBlock> {
    let removals = removed_files(plan, side);
    let target_files = snap.file_count();
    let over_percent = removals * 100 > u64::from(settings.max_remove_percent) * target_files;
    let over_count = settings.max_remove_count > 0 && removals > u64::from(settings.max_remove_count);
    (removals > 0 && (over_percent || over_count)).then_some(GuardBlock::MaxRemove {
        side,
        removals,
        target_files,
    })
}

/// Checks a plan against its snapshots. Order: sentinel, empty root, max remove.
pub fn check(plan: &Plan, left: &Snapshot, right: &Snapshot, settings: &GuardSettings) -> GuardReport {
    let sides = [Side::Left, Side::Right];
    let rule = NameRule::for_pair(&left.caps, &right.caps);
    let sentinel = settings.sentinel.as_deref().into_iter().flat_map(|name| {
        sides
            .into_iter()
            .filter(move |&side| sentinel_missing(name, snapshot_of(side, left, right), rule))
            .map(|side| GuardBlock::SentinelMissing { side })
    });
    let empty = sides
        .into_iter()
        .filter(|&side| snapshot_of(side, left, right).is_empty())
        .map(|side| GuardBlock::EmptyRoot { side });
    let removals = sides
        .into_iter()
        .filter_map(|side| max_remove(plan, side, snapshot_of(side, left, right), settings));
    GuardReport {
        blocks: sentinel.chain(empty).chain(removals).collect(),
        older_overwrites_newer: plan.items.iter().filter(|item| item.older_overwrites_newer).count() as u64,
        skew: skew(plan),
    }
}

/// The nearest quarter-hour step to `gap`, when `gap` sits within `tolerance`
/// of one (FAT's 2 s steps blur an exact hour).
fn clock_step(gap: i64, tolerance: u64) -> Option<i64> {
    let step = (gap + SKEW_STEP_SECS / 2).div_euclid(SKEW_STEP_SECS) * SKEW_STEP_SECS;
    let near = gap.abs_diff(step) <= tolerance;
    (near && step != 0 && step.abs() <= SKEW_MAX_SECS).then_some(step)
}

fn file_gap(item: &PlanItem, right_offset: i64) -> Option<i64> {
    if matches!(item.verdict.category, Category::NameClash | Category::Unreadable) {
        return None;
    }
    let (l, r) = (item.left.as_ref()?, item.right.as_ref()?);
    let files = l.facts.kind != EntryKind::Folder && r.facts.kind != EntryKind::Folder;
    let (lt, rt) = (i64::try_from(l.facts.mtime?).ok()?, i64::try_from(r.facts.mtime?).ok()?);
    files.then_some(rt + right_offset - lt)
}

/// Spec 5.4: with at least ten comparable pairs, ≥ 90% of them differ by one
/// non-zero quarter-hour step up to 26 h. With the hour-shift option on, pairs
/// an exact hour apart are already equal, so they don't take part.
fn skew(plan: &Plan) -> Option<ClockSkew> {
    let rules = &plan.rules;
    let is_hour = |gap: i64| gap.unsigned_abs() == HOUR_SHIFT_SECS;
    let gaps: Vec<i64> = plan
        .items
        .iter()
        .filter_map(|item| file_gap(item, rules.right_offset_secs))
        .filter(|&gap| !(rules.ignore_hour_shift && is_hour(gap)))
        .collect();
    let pairs = gaps.len() as u64;
    if pairs < SKEW_MIN_PAIRS {
        return None;
    }
    let mut tally: BTreeMap<i64, u64> = BTreeMap::new();
    for step in gaps.iter().filter_map(|&gap| clock_step(gap, rules.tolerance_secs)) {
        *tally.entry(step).or_insert(0) += 1;
    }
    let (&offset_secs, &count) = tally.iter().max_by_key(|&(_, count)| *count)?;
    let share = count * 100 / pairs;
    (count * 100 >= SKEW_SHARE_PERCENT * pairs).then_some(ClockSkew {
        offset_secs,
        share_percent: u8::try_from(share).unwrap_or(100),
    })
}

/// Which rows run and which a guard holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Gated {
    /// Rows that run, by index.
    pub run: Vec<u32>,
    /// Rows held back, with the guard that holds each.
    pub held: Vec<(u32, GuardBlock)>,
}

fn hold(item: &PlanItem, blocks: &[GuardBlock], acknowledged: &[Acknowledgement]) -> Option<GuardBlock> {
    let side = item.action.target()?;
    let acked = |ack: Acknowledgement| acknowledged.contains(&ack);
    let found = |wanted: &dyn Fn(&GuardBlock) -> bool| blocks.iter().copied().find(|block| wanted(block));
    if let Some(block) = found(&|b| matches!(b, GuardBlock::SentinelMissing { .. })) {
        return Some(block);
    }
    if item.action.is_removal() {
        if let Some(block) = found(&|b| matches!(b, GuardBlock::EmptyRoot { .. })) {
            return Some(block);
        }
        let max = found(&|b| matches!(b, GuardBlock::MaxRemove { side: s, .. } if *s == side));
        return max.filter(|_| !acked(Acknowledgement::RemoveAnyway));
    }
    found(&|b| *b == GuardBlock::EmptyRoot { side }).filter(|_| !acked(Acknowledgement::CopyIntoEmpty))
}

/// Splits the rows that act into those that run and those a guard holds.
/// Rows that do nothing (`=`, `·`, `?`) are in neither list.
///
/// Runs `check` itself on exactly this plan, so a report made before the
/// person changed actions can never wave new removals through.
pub fn gate(
    plan: &Plan,
    left: &Snapshot,
    right: &Snapshot,
    settings: &GuardSettings,
    acknowledged: &[Acknowledgement],
) -> Gated {
    let report = check(plan, left, right, settings);
    let acting = plan.items.iter().filter(|item| item.action.target().is_some());
    let (run, held) = acting.fold((Vec::new(), Vec::new()), |(mut run, mut held), item| {
        match hold(item, &report.blocks, acknowledged) {
            Some(block) => held.push((item.index, block)),
            None => run.push(item.index),
        }
        (run, held)
    });
    Gated { run, held }
}
