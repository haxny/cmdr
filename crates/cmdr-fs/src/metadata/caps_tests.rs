//! The two questions the loss note asks: which classes a pair puts at risk, and
//! which of those a source item actually had something worth keeping in.

use std::ffi::OsString;
use std::time::{Duration, UNIX_EPOCH};

use super::*;
use crate::metadata::{CarriedMetadata, UserFlags, Xattr};

const SMB_LIKE: MetadataCaps = MetadataCaps {
    stores: MetadataClasses::CREATED,
    reports: MetadataClasses::CREATED,
    folder_stores: MetadataClasses::CREATED,
    folder_dates: true,
};

/// What a source reports and the destination can't store is at risk, and only
/// that: a class the destination keeps isn't, and a class the source can't
/// report is unknown, never at risk.
#[test]
fn at_risk_is_what_the_source_reports_and_the_destination_cant_store() {
    assert_eq!(
        MetadataCaps::NONE.file_classes_at_risk_from(MetadataCaps::ALL),
        MetadataClasses::ALL
    );
    assert_eq!(
        SMB_LIKE.file_classes_at_risk_from(MetadataCaps::ALL),
        MetadataClasses::MODE | MetadataClasses::FLAGS | MetadataClasses::XATTRS
    );
    assert_eq!(
        MetadataCaps::NONE.file_classes_at_risk_from(SMB_LIKE),
        MetadataClasses::CREATED
    );
}

/// Invariant 10's shape: a pair that loses nothing puts nothing at risk, so a
/// copy between them never asks the source anything.
#[test]
fn a_pair_that_keeps_everything_puts_nothing_at_risk() {
    assert!(
        MetadataCaps::ALL
            .file_classes_at_risk_from(MetadataCaps::ALL)
            .is_empty()
    );
    assert!(SMB_LIKE.file_classes_at_risk_from(SMB_LIKE).is_empty());
    assert!(
        MetadataCaps::ALL
            .file_classes_at_risk_from(MetadataCaps::NONE)
            .is_empty()
    );
}

fn at(secs: u64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn xattr(name: &str) -> Xattr {
    Xattr {
        name: OsString::from(name),
        value: Vec::new(),
    }
}

/// Each class counts only when it holds something a person would miss: a birth
/// before the last edit, a mode a fresh file wouldn't get, any flag, and an
/// xattr other than the quarantine.
#[test]
fn worth_keeping_counts_only_what_a_person_would_miss() {
    let carried = CarriedMetadata {
        created: Some(at(1_000)),
        mode: Some(0o755),
        flags: UserFlags::HIDDEN,
        xattrs: vec![xattr("com.apple.metadata:_kMDItemUserTags")],
    };
    assert_eq!(carried.worth_keeping(Some(at(2_000)), 0o644), MetadataClasses::ALL);
}

#[test]
fn a_birth_equal_to_the_last_edit_is_not_worth_reporting() {
    let carried = CarriedMetadata {
        created: Some(at(2_000)),
        ..CarriedMetadata::default()
    };
    assert!(carried.worth_keeping(Some(at(2_000)), 0o644).is_empty());
    assert_eq!(carried.worth_keeping(None, 0o644), MetadataClasses::CREATED);
}

#[test]
fn the_default_mode_is_not_worth_reporting() {
    let carried = CarriedMetadata {
        mode: Some(0o644),
        ..CarriedMetadata::default()
    };
    assert!(carried.worth_keeping(None, 0o644).is_empty());
    assert_eq!(carried.worth_keeping(None, 0o600), MetadataClasses::MODE);
}

#[test]
fn a_quarantine_alone_is_not_worth_reporting() {
    let carried = CarriedMetadata {
        xattrs: vec![xattr("com.apple.quarantine")],
        ..CarriedMetadata::default()
    };
    assert!(carried.worth_keeping(None, 0o644).is_empty());
}

#[test]
fn nothing_carried_is_nothing_worth_keeping() {
    assert!(CarriedMetadata::default().worth_keeping(Some(at(1)), 0o644).is_empty());
}
