use cmdr_fs::volume::{BackendKind, InMemoryVolume};

use crate::*;

#[test]
fn s3_is_case_sensitive_and_smb_is_not() {
    assert!(backend_facts(BackendKind::S3).case_sensitive);
    assert!(backend_facts(BackendKind::Sftp).case_sensitive);
    assert!(!backend_facts(BackendKind::Smb).case_sensitive);
    assert!(!backend_facts(BackendKind::Mtp).case_sensitive);
    assert!(!backend_facts(BackendKind::Adb).case_sensitive);
    assert_eq!(
        backend_facts(BackendKind::Local).case_sensitive,
        !cfg!(target_os = "macos")
    );
}

#[test]
fn archive_times_move_in_two_second_steps() {
    assert_eq!(backend_facts(BackendKind::Archive).mtime_precision_secs, 2);
    assert_eq!(backend_facts(BackendKind::Smb).mtime_precision_secs, 1);
}

#[test]
fn in_memory_volume_caps_follow_its_backend_kind() {
    let smb = InMemoryVolume::new("t").with_backend_kind(BackendKind::Smb);
    assert!(!SyncCaps::of(&smb).case_sensitive);
    let s3 = InMemoryVolume::new("t").with_backend_kind(BackendKind::S3);
    let caps = SyncCaps::of(&s3);
    assert!(caps.case_sensitive);
    assert_eq!(caps.can_read_content, cmdr_fs::volume::Volume::supports_export(&s3));
}
