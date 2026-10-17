//! The rollback planner's pure decision helpers (`verify_snapshot`,
//! `is_self_collision`, `inverse_action`, …), unit-tested in isolation.
//!
//! The per-kind reversal runs end to end against the write engine's real
//! executor, so those cells live app-side, beside it
//! (`crates/cmdr-operations/src/rollback/planner_tests/`).

use std::path::Path;

use super::*;
use crate::types::{EntryType, OpKind, RollbackState};

/// A fixed mtime the snapshots and live entries agree on.
const MT: u64 = 1_700_000_000;

fn entry(name: &str, inode: Option<u64>, size: Option<u64>, mtime: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        modified_at: mtime,
        inode,
        ..FileEntry::new(name.to_string(), format!("/{name}"), false, false)
    }
}

#[test]
fn verify_snapshot_match_drift_unverifiable() {
    let live = entry("f", None, Some(10), Some(MT));
    // Both fields recorded and equal ⇒ Match.
    assert_eq!(
        verify_snapshot(Some(10), Some(MT as i64), live.size, live.modified_at),
        SnapshotVerdict::Match
    );
    // A recorded field differs ⇒ Drift.
    assert_eq!(
        verify_snapshot(Some(10), Some(MT as i64 + 1), live.size, live.modified_at),
        SnapshotVerdict::Drift
    );
    assert_eq!(
        verify_snapshot(Some(11), Some(MT as i64), live.size, live.modified_at),
        SnapshotVerdict::Drift
    );
    // Only size recorded (volume transfers carry no mtime), and it matches ⇒ Match.
    assert_eq!(
        verify_snapshot(Some(10), None, live.size, live.modified_at),
        SnapshotVerdict::Match
    );
    // A recorded field whose live counterpart is absent ⇒ Unverifiable (fail safe).
    let no_mtime = entry("f", None, Some(10), None);
    assert_eq!(
        verify_snapshot(Some(10), Some(MT as i64), no_mtime.size, no_mtime.modified_at),
        SnapshotVerdict::Unverifiable
    );
    // Nothing recorded ⇒ nothing to prove identity on ⇒ Unverifiable.
    assert_eq!(
        verify_snapshot(None, None, live.size, live.modified_at),
        SnapshotVerdict::Unverifiable
    );
}

#[test]
fn self_collision_by_inode_and_by_case_fold() {
    // Real inodes (LocalPosix): same inode ⇒ self, whatever the names.
    let a = entry("dog.JPG", Some(7), Some(1), Some(MT));
    let b = entry("dog.jpg", Some(7), Some(1), Some(MT));
    assert!(is_self_collision(
        true,
        Path::new("/d/dog.JPG"),
        Path::new("/d/dog.jpg"),
        &a,
        &b
    ));
    // Different inodes ⇒ a real collision even with the same name.
    let c = entry("dog.jpg", Some(9), Some(1), Some(MT));
    assert!(!is_self_collision(
        true,
        Path::new("/d/dog.jpg"),
        Path::new("/d/dog.jpg"),
        &a,
        &c
    ));
    // No inodes (MTP/SMB), same volume, case-only difference ⇒ self by path fold.
    let x = entry("dog.JPG", None, Some(1), Some(MT));
    let y = entry("dog.jpg", None, Some(1), Some(MT));
    assert!(is_self_collision(
        true,
        Path::new("/d/dog.JPG"),
        Path::new("/d/dog.jpg"),
        &x,
        &y
    ));
    // No inodes, DIFFERENT volume, same relative path ⇒ NOT self (the occupant is a
    // genuinely different file — a move-back must never overwrite it).
    assert!(!is_self_collision(
        false,
        Path::new("/a.txt"),
        Path::new("/a.txt"),
        &x,
        &y
    ));
    // No inodes, same volume, genuinely different sibling ⇒ not self.
    assert!(!is_self_collision(
        true,
        Path::new("/d/cat.jpg"),
        Path::new("/d/dog.jpg"),
        &x,
        &y
    ));
}

#[test]
fn pure_mapping_helpers() {
    assert_eq!(inverse_kind(OpKind::Copy), OpKind::Delete);
    assert_eq!(inverse_kind(OpKind::CreateFolder), OpKind::Delete);
    assert_eq!(inverse_kind(OpKind::ArchiveEdit), OpKind::Delete);
    assert_eq!(inverse_kind(OpKind::Move), OpKind::Move);
    assert_eq!(inverse_kind(OpKind::Trash), OpKind::Move);
    assert_eq!(inverse_kind(OpKind::Rename), OpKind::Rename);

    assert_eq!(
        inverse_action(OpKind::Copy, EntryType::File),
        Some(InverseAction::RemoveFileIfUnchanged)
    );
    assert_eq!(
        inverse_action(OpKind::Copy, EntryType::Dir),
        Some(InverseAction::RemoveDirIfEmpty)
    );
    assert_eq!(
        inverse_action(OpKind::CreateFolder, EntryType::Dir),
        Some(InverseAction::RemoveDirIfEmpty)
    );
    assert_eq!(
        inverse_action(OpKind::Move, EntryType::File),
        Some(InverseAction::RestoreMove)
    );
    assert_eq!(inverse_action(OpKind::Delete, EntryType::File), None);

    assert_eq!(resolve_final_state(3, 0, false), RollbackState::RolledBack);
    assert_eq!(resolve_final_state(2, 1, false), RollbackState::PartiallyRolledBack);
    // Non-canceled, all skipped ⇒ partial (won't clear on retry).
    assert_eq!(resolve_final_state(0, 3, false), RollbackState::PartiallyRolledBack);
    // Canceled with nothing reversed ⇒ back to rollbackable (a clean retry).
    assert_eq!(resolve_final_state(0, 0, true), RollbackState::Rollbackable);
    // Canceled after reversing some ⇒ partial.
    assert_eq!(resolve_final_state(3, 0, true), RollbackState::PartiallyRolledBack);
}
