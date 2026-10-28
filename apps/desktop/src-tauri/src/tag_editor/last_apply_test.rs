//! The last tag write per folder, kept for the app's lifetime so a Tags window opened later over
//! the same folder can undo it: what an apply stores, how a later one replaces or continues it,
//! what an undo consumes, and which sessions find it.

use std::path::PathBuf;

use super::apply::TagApplyReport;
use super::last_apply::{FolderKey, LastApplies, LastTagApply, UndoableApply, key_of};
use super::session_test::album;
use crate::multi_rename::session::{close as close_rename, open as open_rename};

fn key(dir: &str) -> FolderKey {
    FolderKey::new("root", PathBuf::from(dir))
}

fn wrote(apply_id: &str, files: usize) -> TagApplyReport {
    TagApplyReport {
        apply_id: Some(apply_id.to_string()),
        written: (0..files).map(|n| format!("/book/{n}.mp3")).collect(),
        ..TagApplyReport::default()
    }
}

fn applies(entries: &[(&str, u32)]) -> Vec<UndoableApply> {
    entries
        .iter()
        .map(|(apply_id, written)| UndoableApply {
            apply_id: (*apply_id).to_string(),
            written: *written,
        })
        .collect()
}

#[test]
fn a_write_is_stored_for_its_folder() {
    let mut last = LastApplies::default();
    last.after_apply(key("/book"), &wrote("a1", 3), false);
    assert_eq!(
        last.get(&key("/book")),
        Some(LastTagApply {
            applies: applies(&[("a1", 3)]),
            partial: false,
        })
    );
    assert_eq!(last.get(&key("/other")), None);
}

#[test]
fn a_new_write_replaces_the_last_one_and_a_continued_one_joins_it() {
    let mut last = LastApplies::default();
    last.after_apply(key("/book"), &wrote("a1", 3), false);
    last.after_apply(key("/book"), &wrote("a2", 1), false);
    assert_eq!(last.get(&key("/book")).map(|l| l.applies), Some(applies(&[("a2", 1)])));

    // "Write anyway" continues the same press: Undo reverses both.
    last.after_apply(key("/book"), &wrote("a3", 2), true);
    assert_eq!(
        last.get(&key("/book")).map(|l| l.applies),
        Some(applies(&[("a2", 1), ("a3", 2)]))
    );
}

#[test]
fn a_write_that_wrote_nothing_keeps_the_last_one() {
    let mut last = LastApplies::default();
    last.after_apply(key("/book"), &wrote("a1", 3), false);
    last.after_apply(key("/book"), &TagApplyReport::default(), false);
    assert_eq!(last.get(&key("/book")).map(|l| l.applies), Some(applies(&[("a1", 3)])));
}

#[test]
fn a_write_without_its_old_tags_is_kept_as_partial_and_undoes_nothing() {
    let mut last = LastApplies::default();
    let unkept = TagApplyReport {
        apply_id: None,
        written: vec!["/book/a.mp3".to_string()],
        undo_unavailable: true,
        ..TagApplyReport::default()
    };
    last.after_apply(key("/book"), &unkept, false);
    assert_eq!(
        last.get(&key("/book")),
        Some(LastTagApply {
            applies: Vec::new(),
            partial: true,
        })
    );
    last.after_apply(key("/book"), &wrote("a2", 1), true);
    assert_eq!(
        last.get(&key("/book")),
        Some(LastTagApply {
            applies: applies(&[("a2", 1)]),
            partial: true,
        })
    );
}

#[test]
fn an_undo_consumes_its_apply_and_the_last_one_goes_with_the_last_apply() {
    let mut last = LastApplies::default();
    last.after_apply(key("/book"), &wrote("a1", 3), false);
    last.after_apply(key("/book"), &wrote("a2", 2), true);
    last.after_undo("a2", 0);
    assert_eq!(last.get(&key("/book")).map(|l| l.applies), Some(applies(&[("a1", 3)])));
    last.after_undo("a1", 0);
    assert_eq!(last.get(&key("/book")), None);
}

#[test]
fn an_undo_that_left_files_keeps_its_apply_with_how_many_are_left() {
    let mut last = LastApplies::default();
    last.after_apply(key("/book"), &wrote("a1", 3), false);
    last.after_undo("a1", 1);
    assert_eq!(last.get(&key("/book")).map(|l| l.applies), Some(applies(&[("a1", 1)])));
}

#[test]
fn an_undo_of_an_apply_no_folder_keeps_changes_nothing() {
    let mut last = LastApplies::default();
    last.after_apply(key("/book"), &wrote("a1", 3), false);
    last.after_undo("elsewhere", 0);
    assert_eq!(last.get(&key("/book")).map(|l| l.applies), Some(applies(&[("a1", 3)])));
}

/// A Tags window opened later over the same folder (a new Multi-Rename session, other files
/// selected even) finds the last write there; a gone session finds nothing.
#[test]
fn every_session_over_the_same_folder_shares_one_key() {
    let (_dir, listing, first) = album("tag-last-apply-key");
    let second = open_rename(listing.id(), true, Some(&[0]), 0)
        .expect("a second session opens")
        .session_id;
    assert_eq!(key_of(&first).ok(), key_of(&second).ok());
    assert!(key_of(&first).is_ok());
    close_rename(&first);
    close_rename(&second);
    assert!(key_of(&first).is_err());
}

/// The undo store pruned the record (past its 50 applies): the undo that found it gone takes the
/// apply out, so ⌘Z isn't offered for it again.
#[test]
fn an_apply_whose_record_is_gone_is_forgotten() {
    let mut last = LastApplies::default();
    last.after_apply(key("/book"), &wrote("a1", 3), false);
    last.forget("a1");
    assert_eq!(last.get(&key("/book")), None);
}

/// The map never holds more folders than the store keeps applies: past that, the folder written
/// longest ago goes first (its record is the store's next to prune anyway).
#[test]
fn no_more_folders_are_kept_than_the_store_keeps_applies() {
    let mut last = LastApplies::with_cap(2);
    last.after_apply(key("/one"), &wrote("a1", 1), false);
    last.after_apply(key("/two"), &wrote("a2", 1), false);
    // Written again: now the newest.
    last.after_apply(key("/one"), &wrote("a3", 1), false);
    last.after_apply(key("/three"), &wrote("a4", 1), false);
    assert_eq!(last.get(&key("/two")), None);
    assert!(last.get(&key("/one")).is_some());
    assert!(last.get(&key("/three")).is_some());
}

/// What an undo of a pruned record answers is what `undo` takes as "gone"; any other refusal isn't.
#[test]
fn an_undo_of_a_pruned_record_reads_as_gone() {
    let (_dir, store) = super::apply_test::store("tag-last-apply-gone");
    let refused = super::undo::undo_with(&store, "tags-1-gone", None).expect_err("no such record");
    assert!(super::undo::record_gone(&refused));
    assert!(!super::undo::record_gone(&super::session::TagEditorError::TaskFailed));
}
