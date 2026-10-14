//! The whole-listing count's one decision: count every uncounted row, say the
//! listing is too long for it, or say there's nothing left to count.

use cmdr_fs::entry::FileEntry;
use cmdr_fs::git_meta::{GitCountKind, GitEntryMeta};
use cmdr_git::GitPortal;

use super::commit_counts::{CountAllPlan, plan_count_all};

fn commit(n: usize, counted: bool) -> FileEntry {
    let short = format!("{n:07x}");
    let mut row = FileEntry::new(short.clone(), format!("/repo/.git/commits/{short}"), true, false);
    row.git_meta = Some(if counted {
        GitEntryMeta::Count {
            counted: GitCountKind::FilesChanged,
            n: 3,
        }
    } else {
        GitEntryMeta::CountPending {
            counted: GitCountKind::FilesChanged,
        }
    });
    row
}

#[test]
fn a_short_listing_counts_every_uncounted_row() {
    let rows = vec![commit(1, false), commit(2, true), commit(3, false)];

    let CountAllPlan::Count(paths) = plan_count_all(&rows) else {
        panic!("a three-commit listing is short enough to count in full");
    };
    let paths: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
    assert_eq!(paths, ["/repo/.git/commits/0000001", "/repo/.git/commits/0000003"]);
}

#[test]
fn a_listing_at_the_limit_still_counts_in_full() {
    let rows: Vec<FileEntry> = (0..GitPortal::COUNT_ALL_LIMIT).map(|n| commit(n, false)).collect();
    assert!(matches!(plan_count_all(&rows), CountAllPlan::Count(p) if p.len() == GitPortal::COUNT_ALL_LIMIT));
}

#[test]
fn one_commit_past_the_limit_is_too_many() {
    let rows: Vec<FileEntry> = (0..=GitPortal::COUNT_ALL_LIMIT).map(|n| commit(n, false)).collect();
    assert_eq!(plan_count_all(&rows), CountAllPlan::TooMany);
}

#[test]
fn a_fully_counted_listing_has_nothing_to_count() {
    assert_eq!(plan_count_all(&[commit(1, true)]), CountAllPlan::NothingToCount);
}
