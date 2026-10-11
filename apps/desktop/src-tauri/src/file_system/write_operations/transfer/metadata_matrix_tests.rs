//! What a local copy or move keeps of a file's and a folder's metadata TODAY, one
//! cell per (path, class): the characterization the metadata plan converts
//! (`docs/specs/transfer-metadata-preservation.md` § "M0").
//!
//! Each cell pins today's outcome, and a `// FLIPS IN M<n>` comment names the
//! milestone that turns it into the target behavior. A bug cell asserts the
//! failure (a Locked file can't land), a loss cell asserts the loss (birth reads
//! the mtime), and a keep cell guards what already survives. Every flip cell was
//! first written against the target and seen red for the documented reason.
//!
//! The fixture is `cmdr_fs::testing::metadata_fixture` (macOS). Elsewhere: the
//! extract cell in `volume/copy_extract_out_tests.rs`, the APFS → HFS+ cell in
//! `real_image.rs`, and the SMB cells in `backend_suites/network_dates_test_support.rs`.

#[cfg(target_os = "macos")]
mod macos {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use cmdr_fs::staging::STAGING_TEMP_MARKER;
    use cmdr_fs::testing::metadata_fixture::{
        ACCESSED_SECS, BIRTH_SECS, HIDDEN_FLAG, LOCKED_FLAG, MODIFIED_SECS, MetadataFixture, accessed_secs,
        add_acl_entry, birth_secs, flags, has_acl, mode, modified_secs, xattrs,
    };

    use crate::file_system::write_operations::event_sinks::CollectorEventSink;
    use crate::file_system::write_operations::overwrite::PostLanding;
    use crate::file_system::write_operations::state::WriteOperationState;
    use crate::file_system::write_operations::transfer::copy::copy_files_with_progress_inner;
    use crate::file_system::write_operations::transfer::copy_strategy::{
        LocalCopyStrategy, StrategyCopyOutcome, copy_file_using, select_local_copy_strategy,
    };
    use crate::file_system::write_operations::transfer::move_op::test_support::{run_cross_fs_move, run_same_fs_move};
    use crate::file_system::write_operations::types::{ConflictResolution, WriteOperationConfig, WriteOperationError};

    fn state() -> Arc<WriteOperationState> {
        Arc::new(WriteOperationState::new(Duration::from_millis(50)))
    }

    /// `dst/` inside the fixture, so source and destination share one APFS volume.
    fn dst_dir(fixture: &MetadataFixture) -> PathBuf {
        let dst = fixture.join("dst");
        std::fs::create_dir_all(&dst).expect("create dst");
        dst
    }

    fn copy_one(
        strategy: LocalCopyStrategy,
        source: &Path,
        dest: &Path,
    ) -> Result<StrategyCopyOutcome, WriteOperationError> {
        copy_file_using(strategy, &state(), source, dest, false, PostLanding::Apply, None)
    }

    /// Asserts the pair takes the clone arm on this machine, so a clone cell can't
    /// pass for the wrong reason.
    fn assert_clone_arm(source: &Path, dest: &Path) {
        assert_eq!(
            select_local_copy_strategy(source, dest),
            LocalCopyStrategy::AppleClone,
            "the fixture's temp dir must be one APFS volume for a clone cell to mean anything"
        );
    }

    /// The `.cmdr-tmp-*` items left in `dir`.
    fn staging_temps(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.to_string_lossy().contains(STAGING_TEMP_MARKER))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn copy_tree(sources: &[PathBuf], dst: &Path) {
        let events = Arc::new(CollectorEventSink::new());
        let result = copy_files_with_progress_inner(
            &*events,
            "op-metadata-matrix",
            &state(),
            sources,
            dst,
            &WriteOperationConfig::default(),
        );
        assert!(result.is_ok(), "the copy itself succeeds: {result:?}");
    }

    // ── Clone arm (same APFS volume) ────────────────────────────────────────

    /// The clone carries `uchg` onto the temp and the landing rename refuses it
    /// (`EPERM`), so the landing clears it, lands, and puts it back on the landed
    /// file. The clone's birth time survives all three steps.
    #[test]
    fn a_locked_file_cloned_within_one_volume_lands_locked_with_its_birth_time() {
        let fixture = MetadataFixture::new("matrix_clone_locked");
        let source = fixture.locked_file("locked.txt");
        let dst = dst_dir(&fixture);
        let dest = dst.join("locked.txt");
        assert_clone_arm(&source, &dest);

        copy_one(LocalCopyStrategy::AppleClone, &source, &dest).expect("the clone lands");

        assert_eq!(flags(&dest) & LOCKED_FLAG, LOCKED_FLAG, "the landed file is Locked");
        assert_eq!(birth_secs(&dest), BIRTH_SECS);
        assert_eq!(modified_secs(&dest), MODIFIED_SECS);
        assert_eq!(staging_temps(&dst), Vec::<PathBuf>::new(), "no temp stays behind");
    }

    /// `COPYFILE_ACL` puts the `deny delete` ACL on the temp and the landing
    /// rename refuses it (`EACCES`), so the landing strips it, lands, and puts it
    /// back on the landed file.
    #[test]
    fn a_deny_delete_file_cloned_within_one_volume_lands_with_its_acl() {
        let fixture = MetadataFixture::new("matrix_clone_acl");
        let source = fixture.deny_delete_file("acl.txt");
        let dst = dst_dir(&fixture);
        let dest = dst.join("acl.txt");
        assert_clone_arm(&source, &dest);

        copy_one(LocalCopyStrategy::AppleClone, &source, &dest).expect("the clone lands");

        assert!(has_acl(&dest), "the landed file wears the ACL");
        assert_eq!(birth_secs(&dest), BIRTH_SECS);
        assert_eq!(staging_temps(&dst), Vec::<PathBuf>::new(), "no temp stays behind");
    }

    /// The landing rename is the probe: a file with nothing to clear lands with
    /// one `copyfile` and one `rename`, and never reaches the metadata helper.
    #[test]
    fn a_plain_file_through_the_clone_arm_never_calls_the_metadata_helper() {
        let fixture = MetadataFixture::new("matrix_clone_plain");
        let source = fixture.rich_file("f.txt");
        let dst = dst_dir(&fixture);
        let dest = dst.join("f.txt");
        assert_clone_arm(&source, &dest);
        let before = cmdr_fs::metadata::helper_calls_on_this_thread();

        copy_one(LocalCopyStrategy::AppleClone, &source, &dest).expect("the clone lands");

        assert_eq!(cmdr_fs::metadata::helper_calls_on_this_thread(), before);
    }

    /// A keep cell: the clone carries every class of a file with no rename-blocker.
    #[test]
    fn a_clone_keeps_every_class_of_a_file_without_a_rename_blocker() {
        let fixture = MetadataFixture::new("matrix_clone_rich");
        let source = fixture.rich_file("f.txt");
        let dst = dst_dir(&fixture);
        let dest = dst.join("f.txt");
        assert_clone_arm(&source, &dest);

        copy_one(LocalCopyStrategy::AppleClone, &source, &dest).expect("the clone lands");

        assert_eq!(birth_secs(&dest), BIRTH_SECS);
        assert_eq!(modified_secs(&dest), MODIFIED_SECS);
        assert_eq!(mode(&dest), 0o750);
        assert_eq!(flags(&dest) & HIDDEN_FLAG, HIDDEN_FLAG);
        assert_eq!(xattrs(&dest), xattrs(&source));
    }

    // ── Chunked arm (every other local pair) ────────────────────────────────

    /// The source's ACL goes on after the landing rename, which a `deny delete`
    /// ACL on the temp would refuse (`EACCES`).
    #[test]
    fn a_deny_delete_file_copied_chunked_lands_with_its_acl() {
        let fixture = MetadataFixture::new("matrix_chunked_acl");
        let source = fixture.deny_delete_file("acl.txt");
        let dst = dst_dir(&fixture);
        let dest = dst.join("acl.txt");

        copy_one(LocalCopyStrategy::Chunked, &source, &dest).expect("the copy lands");

        assert!(has_acl(&dest), "the landed file wears the ACL");
        assert_eq!(staging_temps(&dst), Vec::<PathBuf>::new(), "no temp stays behind");
    }

    /// Only mtime is set, and APFS drags birth down to it.
    #[test]
    fn a_chunked_copy_loses_the_files_birth_time_today() {
        // FLIPS IN M3: birth == BIRTH_SECS.
        let fixture = MetadataFixture::new("matrix_chunked_birth");
        let source = fixture.rich_file("f.txt");
        let dest = dst_dir(&fixture).join("f.txt");

        copy_one(LocalCopyStrategy::Chunked, &source, &dest).expect("the copy lands");

        assert_eq!(birth_secs(&dest), MODIFIED_SECS, "birth reads the source's mtime");
    }

    #[test]
    fn a_chunked_copy_loses_hidden_today() {
        // FLIPS IN M3: Hidden kept.
        let fixture = MetadataFixture::new("matrix_chunked_hidden");
        let source = fixture.rich_file("f.txt");
        let dest = dst_dir(&fixture).join("f.txt");

        copy_one(LocalCopyStrategy::Chunked, &source, &dest).expect("the copy lands");

        assert_eq!(flags(&dest) & HIDDEN_FLAG, 0);
    }

    #[test]
    fn a_locked_file_copied_chunked_lands_unlocked_today() {
        // FLIPS IN M3: lands Locked (after the rename).
        let fixture = MetadataFixture::new("matrix_chunked_locked");
        let source = fixture.locked_file("locked.txt");
        let dest = dst_dir(&fixture).join("locked.txt");

        copy_one(LocalCopyStrategy::Chunked, &source, &dest).expect("the copy lands");

        assert_eq!(flags(&dest) & LOCKED_FLAG, 0);
    }

    /// A keep cell: what the chunked arm carries today.
    #[test]
    fn a_chunked_copy_keeps_xattrs_mode_mtime_and_atime() {
        let fixture = MetadataFixture::new("matrix_chunked_keeps");
        let source = fixture.rich_file("f.txt");
        let dest = dst_dir(&fixture).join("f.txt");

        copy_one(LocalCopyStrategy::Chunked, &source, &dest).expect("the copy lands");

        assert_eq!(xattrs(&dest), xattrs(&source));
        assert_eq!(mode(&dest), 0o750);
        assert_eq!(modified_secs(&dest), MODIFIED_SECS);
        assert_eq!(accessed_secs(&dest), ACCESSED_SECS);
    }

    // ── Symlinks ────────────────────────────────────────────────────────────

    /// A copied link is a fresh `symlink(2)`: its own dates are the copy's.
    #[test]
    fn a_copied_symlink_loses_its_own_dates_today() {
        // FLIPS IN M3: the link's own birth and mtime kept.
        let fixture = MetadataFixture::new("matrix_symlink");
        let target = fixture.rich_file("f.txt");
        let link = fixture.symlink("link", &target);
        let dst = dst_dir(&fixture);

        copy_tree(&[link], &dst);

        let landed = dst.join("link");
        assert!(std::fs::symlink_metadata(&landed).expect("lstat").is_symlink());
        assert!(birth_secs(&landed) > ACCESSED_SECS, "the link was born at copy time");
        assert!(modified_secs(&landed) > ACCESSED_SECS, "and modified then too");
    }

    // ── Folders (the native folder pass, strategy-independent) ──────────────

    /// Copies the fixture's rich folder into `dst/`, answering (source, landed).
    fn copy_rich_folder(fixture: &MetadataFixture) -> (PathBuf, PathBuf) {
        let folder = fixture.rich_folder("dir");
        let dst = dst_dir(fixture);
        copy_tree(std::slice::from_ref(&folder), &dst);
        let landed = dst.join("dir");
        assert!(landed.join("child.txt").is_file(), "the folder's child lands");
        (folder, landed)
    }

    /// A keep cell: the folder pass dates what it created.
    #[test]
    fn a_copied_folder_keeps_its_mtime() {
        let fixture = MetadataFixture::new("matrix_folder_mtime");
        let (_, landed) = copy_rich_folder(&fixture);
        assert_eq!(modified_secs(&landed), MODIFIED_SECS);
    }

    /// The folder pass sets only mtime, and APFS drags birth down to it.
    #[test]
    fn a_copied_folder_loses_its_birth_time_today() {
        // FLIPS IN M4: birth == BIRTH_SECS.
        let fixture = MetadataFixture::new("matrix_folder_birth");
        let (_, landed) = copy_rich_folder(&fixture);
        assert_eq!(birth_secs(&landed), MODIFIED_SECS, "birth reads the source's mtime");
    }

    /// A `0700` folder lands at the umask default: a privacy regression.
    #[test]
    fn a_copied_folder_loses_its_mode_today() {
        // FLIPS IN M4: lands 0700.
        let fixture = MetadataFixture::new("matrix_folder_mode");
        let (_, landed) = copy_rich_folder(&fixture);
        let fresh = fixture.join("a fresh folder");
        std::fs::create_dir(&fresh).expect("create a reference folder");
        assert_eq!(mode(&landed), mode(&fresh), "the copy wears what `mkdir` gives");
        assert_ne!(mode(&landed), 0o700);
    }

    #[test]
    fn a_copied_folder_loses_its_tags_comment_and_finder_info_today() {
        // FLIPS IN M4: tags, comment, and FinderInfo kept.
        let fixture = MetadataFixture::new("matrix_folder_xattrs");
        let (_, landed) = copy_rich_folder(&fixture);
        assert_eq!(xattrs(&landed), Vec::new());
    }

    #[test]
    fn a_copied_folder_loses_hidden_today() {
        // FLIPS IN M4: Hidden kept.
        let fixture = MetadataFixture::new("matrix_folder_hidden");
        let (_, landed) = copy_rich_folder(&fixture);
        assert_eq!(flags(&landed) & HIDDEN_FLAG, 0);
    }

    #[test]
    fn a_copied_folder_loses_its_acl_today() {
        // FLIPS IN M4: the ACL kept (applied last, deepest first).
        let fixture = MetadataFixture::new("matrix_folder_acl");
        let (_, landed) = copy_rich_folder(&fixture);
        assert!(!has_acl(&landed));
    }

    // ── Moves ───────────────────────────────────────────────────────────────

    /// Every item under `root` that wears `uchg` or an ACL, `root` excluded.
    fn rename_blocked_items(root: &Path) -> Vec<PathBuf> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for entry in entries.flatten() {
                let path = entry.path();
                if flags(&path) & LOCKED_FLAG != 0 || has_acl(&path) {
                    out.push(path.clone());
                }
                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    walk(&path, out);
                }
            }
        }
        let mut found = Vec::new();
        walk(root, &mut found);
        found
    }

    /// The staged copies land unlocked and ACL-free INSIDE the staging tree, and
    /// take their Locked flag and ACL only once Phase 3 has renamed them into
    /// place. Phase 4 then can't delete either original (`uchg` refuses with
    /// `EPERM`, `deny delete` with `EACCES`), so the move stops there with every
    /// original kept: nothing is lost, both copies exist.
    #[test]
    fn a_cross_fs_move_of_a_folder_holding_a_locked_and_a_deny_delete_file_lands_both() {
        let fixture = MetadataFixture::new("matrix_move_folder");
        let folder = fixture.join("album");
        std::fs::create_dir(&folder).expect("create the folder");
        let locked = fixture.locked_file("album/locked.txt");
        let acl = fixture.deny_delete_file("album/acl.txt");
        let dst = dst_dir(&fixture);

        let result = run_cross_fs_move(
            std::slice::from_ref(&folder),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-move-folder",
        );

        let landed = dst.join("album");
        assert_eq!(flags(&landed.join("locked.txt")) & LOCKED_FLAG, LOCKED_FLAG);
        assert_eq!(birth_secs(&landed.join("locked.txt")), BIRTH_SECS);
        assert!(has_acl(&landed.join("acl.txt")), "the landed file wears the ACL");
        assert!(
            matches!(result, Err(WriteOperationError::PermissionDenied { .. })),
            "Phase 4 can't remove a Locked or `deny delete` original: {:?}",
            result.err()
        );
        assert!(locked.exists() && acl.exists(), "both originals stay");
        let names: Vec<String> = std::fs::read_dir(&dst)
            .expect("list dst")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["album".to_string()], "the staging folder is gone");
    }

    /// The staged copy lands unlocked inside the staging tree and takes `uchg`
    /// after Phase 3. Phase 4's delete of the Locked original is refused
    /// (`EPERM`), so the move stops with the original kept.
    #[test]
    fn a_cross_fs_move_of_a_locked_file_lands_locked_and_keeps_the_original() {
        let fixture = MetadataFixture::new("matrix_move_locked");
        let source = fixture.locked_file("locked.txt");
        let dst = dst_dir(&fixture);

        let result = run_cross_fs_move(
            std::slice::from_ref(&source),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-move-locked",
        );

        let landed = dst.join("locked.txt");
        assert_eq!(flags(&landed) & LOCKED_FLAG, LOCKED_FLAG, "the landed file is Locked");
        assert_eq!(birth_secs(&landed), BIRTH_SECS);
        assert!(
            matches!(
                result,
                Err(WriteOperationError::PermissionDenied {
                    errno: Some(libc::EPERM),
                    ..
                })
            ),
            "{:?}",
            result.err()
        );
        assert!(source.exists(), "the Locked original stays");
    }

    /// Phase 3 fails (the destination refuses new files, so the staged files
    /// can't be renamed into it), and the staging cleanup removes the whole
    /// staging tree: nothing in it wears a rename-blocker, because those only go
    /// on after Phase 3.
    #[test]
    fn a_cross_fs_move_whose_phase_3_fails_leaves_a_removable_staging_tree() {
        let fixture = MetadataFixture::new("matrix_move_phase3");
        let locked = fixture.locked_file("locked.txt");
        let acl = fixture.deny_delete_file("acl.txt");
        let dst = dst_dir(&fixture);
        // The staging folder is a subdirectory, which this still allows.
        add_acl_entry(&dst, "everyone deny add_file");

        let result = run_cross_fs_move(
            &[locked.clone(), acl.clone()],
            &dst,
            ConflictResolution::Stop,
            "op-matrix-move-phase3",
        );

        // The refusal names a staged item, so Phase 2 did stage both.
        assert!(
            matches!(
                &result,
                Err(WriteOperationError::IoError { path, .. }) if path.contains(cmdr_fs::staging::STAGING_DIR_PREFIX)
            ),
            "Phase 3 can't land a file: {:?}",
            result.err()
        );
        assert!(locked.exists() && acl.exists(), "both originals stay");
        crate::test_support::wait_until(Duration::from_secs(5), "the staging tree's removal", || {
            std::fs::read_dir(&dst).is_ok_and(|mut entries| entries.next().is_none())
        });
        assert_eq!(rename_blocked_items(&dst), Vec::<PathBuf>::new());
    }

    /// `rename(2)` refuses a Locked item (`EPERM`). Out of scope by David's
    /// decision (spec § "Decided"); M10 files the follow-up.
    #[test]
    fn a_same_fs_move_of_a_locked_file_is_refused_today() {
        // FLIPS: not in this plan (M10 files a follow-up issue).
        let fixture = MetadataFixture::new("matrix_same_fs_locked");
        let source = fixture.locked_file("locked.txt");
        let dst = dst_dir(&fixture);

        let result = run_same_fs_move(
            std::slice::from_ref(&source),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-same-fs-locked",
        );

        assert!(
            matches!(
                result,
                Err(WriteOperationError::PermissionDenied {
                    errno: Some(libc::EPERM),
                    ..
                })
            ),
            "{:?}",
            result.err()
        );
        assert!(source.exists(), "the source stays");
        assert!(!dst.join("locked.txt").exists());
    }
}

/// Linux has no birth to set, no BSD flags, and no `com.apple.*`: what a copy
/// owes there is `user.*` xattrs and the mode (spec § "Platforms").
#[cfg(target_os = "linux")]
mod linux {
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use crate::file_system::write_operations::overwrite::PostLanding;
    use crate::file_system::write_operations::state::WriteOperationState;
    use crate::file_system::write_operations::transfer::copy_strategy::{LocalCopyStrategy, copy_file_using};
    use crate::test_support::TestDir;

    const XATTR_NAME: &str = "user.app";
    const XATTR_VALUE: &[u8] = b"app value";

    /// A `0750` file carrying `user.app`, and where its copy goes.
    fn seed(dir: &TestDir) -> (PathBuf, PathBuf) {
        let source = dir.join("f.txt");
        std::fs::write(&source, b"linux file").expect("write the source");
        xattr::set(&source, XATTR_NAME, XATTR_VALUE).expect("this filesystem takes `user.*` xattrs");
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o750)).expect("chmod the source");
        let dst = dir.join("dst");
        std::fs::create_dir(&dst).expect("create dst");
        (source, dst.join("f.txt"))
    }

    fn copy(strategy: LocalCopyStrategy, source: &Path, dest: &Path) {
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
        copy_file_using(strategy, &state, source, dest, false, PostLanding::Apply, None).expect("the copy lands");
    }

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).expect("stat").permissions().mode() & 0o7777
    }

    /// A keep cell: the chunked arm lists and copies every xattr.
    #[test]
    fn a_chunked_copy_keeps_user_xattrs_and_mode() {
        let dir = TestDir::new("matrix_linux_chunked");
        let (source, dest) = seed(&dir);

        copy(LocalCopyStrategy::Chunked, &source, &dest);

        assert_eq!(
            xattr::get(&dest, XATTR_NAME).expect("read").as_deref(),
            Some(XATTR_VALUE)
        );
        assert_eq!(mode(&dest), 0o750);
    }

    /// `copy_file_range` moves bytes only, and the arm adds mode and dates.
    #[test]
    fn a_kernel_copy_range_copy_loses_user_xattrs_today() {
        // FLIPS IN M3: `user.*` xattrs kept.
        let dir = TestDir::new("matrix_linux_kernel_xattrs");
        let (source, dest) = seed(&dir);

        copy(LocalCopyStrategy::KernelCopyRange, &source, &dest);

        assert_eq!(xattr::get(&dest, XATTR_NAME).expect("read"), None);
    }

    /// A keep cell: the arm sets the source's mode on what it wrote.
    #[test]
    fn a_kernel_copy_range_copy_keeps_the_mode() {
        let dir = TestDir::new("matrix_linux_kernel_mode");
        let (source, dest) = seed(&dir);

        copy(LocalCopyStrategy::KernelCopyRange, &source, &dest);

        assert_eq!(mode(&dest), 0o750);
    }
}
