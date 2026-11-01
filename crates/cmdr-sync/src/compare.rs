//! The one answer to "how do these two copies relate?", shared with ⇧F2.

use cmdr_fs::entry::FileEntry;
use serde::{Deserialize, Serialize};

use crate::SyncCaps;

/// How far apart two modification times can be and still count as the same, in
/// seconds. FAT and many network shares store time in 2 s steps, so a faithful
/// copy can read a second or two off its original.
pub const SAME_TIME_TOLERANCE_SECS: u64 = 2;
/// The exact shift FAT and exFAT show across a daylight-saving change.
pub const HOUR_SHIFT_SECS: u64 = 3600;

/// What an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A folder.
    Folder,
    /// A symlink (compared by its own listing facts, never followed).
    Symlink,
}

/// The listing facts a comparison looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EntryFacts {
    /// What the entry is.
    pub kind: EntryKind,
    /// Size in bytes, when the listing knows it.
    pub size: Option<u64>,
    /// Modification time in whole seconds, when the listing knows it.
    pub mtime: Option<u64>,
}

impl EntryFacts {
    /// The facts of a listed entry.
    pub fn of(entry: &FileEntry) -> Self {
        let kind = if entry.is_symlink {
            EntryKind::Symlink
        } else if entry.is_directory {
            EntryKind::Folder
        } else {
            EntryKind::File
        };
        Self {
            kind,
            size: entry.size,
            mtime: entry.modified_at,
        }
    }
}

/// What decides that two files differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CompareBy {
    /// Size and modification time.
    DateAndSize,
    /// Size only.
    SizeOnly,
    /// Size, then the bytes of equal-size pairs.
    Content,
}

/// How to compare a pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompareRules {
    /// What decides.
    pub by: CompareBy,
    /// Leave modification times out of the decision.
    pub ignore_date: bool,
    /// Times this close count as equal, in seconds.
    pub tolerance_secs: u64,
    /// Treat an exact one-hour difference as equal.
    pub ignore_hour_shift: bool,
    /// Seconds added to the right side's time before comparing (clock skew).
    pub right_offset_secs: i64,
}

impl CompareRules {
    /// What ⇧F2 and a fresh sync use: date and size, 2 s tolerance.
    pub const QUICK: Self = Self {
        by: CompareBy::DateAndSize,
        ignore_date: false,
        tolerance_secs: SAME_TIME_TOLERANCE_SECS,
        ignore_hour_shift: false,
        right_offset_secs: 0,
    };

    /// These rules with the tolerance raised to the coarser of the two sides'
    /// time precision.
    pub fn widened_for(self, left: &SyncCaps, right: &SyncCaps) -> Self {
        let coarsest = left.mtime_precision_secs.max(right.mtime_precision_secs);
        Self {
            tolerance_secs: self.tolerance_secs.max(coarsest),
            ..self
        }
    }
}

/// What a byte compare said, when one was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ContentState {
    /// Nobody asked.
    NotAsked,
    /// Asked, no answer yet.
    Pending,
    /// The bytes are equal.
    Same,
    /// The bytes differ.
    Differs,
    /// A side couldn't be read.
    Unreadable,
}

/// How two sides relate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    /// Only on the left.
    LeftOnly,
    /// Only on the right.
    RightOnly,
    /// Both, the left is newer.
    NewerLeft,
    /// Both, the right is newer.
    NewerRight,
    /// Both, different, no newer side.
    Different,
    /// Both, the same.
    Equal,
    /// Not enough facts to say.
    Unknown,
    /// A file against a folder (or a symlink).
    TypeClash,
    /// Two names on one side collapse to one key (set by the planner, never `classify`).
    NameClash,
    /// A side couldn't be read (set by the planner, never `classify`).
    Unreadable,
}

/// Which kind of evidence settled the verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Rung {
    /// Present on one side only, or the kinds decided.
    Presence,
    /// Size and time.
    SizeAndTime,
    /// Size alone.
    SizeOnly,
    /// The bytes.
    Content,
}

/// The answer for one pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    /// How the sides relate.
    pub category: Category,
    /// What settled it.
    pub rung: Rung,
    /// Whether both sizes are known and equal.
    pub same_size: bool,
}

/// How the two modification times relate under `rules`.
enum Dates {
    Equal,
    LeftNewer,
    RightNewer,
}

fn compare_dates(left: u64, right: u64, rules: &CompareRules) -> Dates {
    let delta = i128::from(left) - (i128::from(right) + i128::from(rules.right_offset_secs));
    let gap = delta.unsigned_abs();
    let shifted = rules.ignore_hour_shift && gap == u128::from(HOUR_SHIFT_SECS);
    if gap <= u128::from(rules.tolerance_secs) || shifted {
        Dates::Equal
    } else if delta > 0 {
        Dates::LeftNewer
    } else {
        Dates::RightNewer
    }
}

fn verdict(category: Category, rung: Rung, same_size: bool) -> Verdict {
    Verdict {
        category,
        rung,
        same_size,
    }
}

/// Sizes decide: equal, different, or unknown.
fn by_size(l: &EntryFacts, r: &EntryFacts) -> Verdict {
    match (l.size, r.size) {
        (Some(a), Some(b)) if a == b => verdict(Category::Equal, Rung::SizeOnly, true),
        (Some(_), Some(_)) => verdict(Category::Different, Rung::SizeOnly, false),
        _ => verdict(Category::Unknown, Rung::SizeOnly, false),
    }
}

/// Size and time decide (a missing time hands the call to the sizes).
fn by_date_and_size(l: &EntryFacts, r: &EntryFacts, rules: &CompareRules) -> Verdict {
    let (Some(lt), Some(rt)) = (l.mtime, r.mtime) else {
        return by_size(l, r);
    };
    let same_size = matches!((l.size, r.size), (Some(a), Some(b)) if a == b);
    let category = match compare_dates(lt, rt, rules) {
        Dates::LeftNewer => Category::NewerLeft,
        Dates::RightNewer => Category::NewerRight,
        Dates::Equal => match (l.size, r.size) {
            (Some(a), Some(b)) if a != b => Category::Different,
            _ => Category::Equal,
        },
    };
    verdict(category, Rung::SizeAndTime, same_size)
}

fn by_content(l: &EntryFacts, r: &EntryFacts, rules: &CompareRules, content: ContentState) -> Verdict {
    let sizes_known_equal = matches!((l.size, r.size), (Some(a), Some(b)) if a == b);
    if !sizes_known_equal {
        return by_date_or_size(l, r, rules);
    }
    let category = match content {
        ContentState::Same => Category::Equal,
        ContentState::Differs => match newer_by_date(l, r, rules) {
            Some(newer) => newer,
            None => Category::Different,
        },
        ContentState::NotAsked | ContentState::Pending | ContentState::Unreadable => Category::Unknown,
    };
    verdict(category, Rung::Content, true)
}

/// The newer side by date, when dates are in play and differ.
fn newer_by_date(l: &EntryFacts, r: &EntryFacts, rules: &CompareRules) -> Option<Category> {
    if rules.ignore_date {
        return None;
    }
    let (lt, rt) = (l.mtime?, r.mtime?);
    match compare_dates(lt, rt, rules) {
        Dates::LeftNewer => Some(Category::NewerLeft),
        Dates::RightNewer => Some(Category::NewerRight),
        Dates::Equal => None,
    }
}

fn by_date_or_size(l: &EntryFacts, r: &EntryFacts, rules: &CompareRules) -> Verdict {
    if rules.ignore_date {
        by_size(l, r)
    } else {
        by_date_and_size(l, r, rules)
    }
}

/// How the two copies of one name relate. Never yields `NameClash` or
/// `Unreadable`: the planner sets those. Table: `DETAILS.md`.
pub fn classify(
    left: Option<&EntryFacts>,
    right: Option<&EntryFacts>,
    rules: &CompareRules,
    content: ContentState,
) -> Verdict {
    let (l, r) = match (left, right) {
        (Some(l), Some(r)) => (l, r),
        (Some(_), None) => return verdict(Category::LeftOnly, Rung::Presence, false),
        (None, Some(_)) => return verdict(Category::RightOnly, Rung::Presence, false),
        (None, None) => return verdict(Category::Unknown, Rung::Presence, false),
    };
    if l.kind != r.kind {
        return verdict(Category::TypeClash, Rung::Presence, false);
    }
    if l.kind == EntryKind::Folder {
        return verdict(Category::Equal, Rung::Presence, false);
    }
    match rules.by {
        CompareBy::SizeOnly => by_size(l, r),
        CompareBy::DateAndSize => by_date_or_size(l, r, rules),
        CompareBy::Content => by_content(l, r, rules, content),
    }
}
