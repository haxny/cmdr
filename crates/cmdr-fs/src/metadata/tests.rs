//! The helper against real macOS items: what it puts on, what it clears, and
//! that it never reaches through a symlink.

#[cfg(target_os = "macos")]
mod macos {
    use super::super::*;
    use crate::testing::metadata_fixture::{LOCKED_FLAG, MetadataFixture, flags, has_acl};

    fn locked_and_acld(fixture: &MetadataFixture, name: &str) -> std::path::PathBuf {
        let path = fixture.deny_delete_file(name);
        crate::testing::metadata_fixture::set_flags(&path, LOCKED_FLAG);
        path
    }

    #[test]
    fn apply_landed_puts_the_acl_then_the_flags_on() {
        let fixture = MetadataFixture::new("metadata_apply_landed");
        let source = fixture.deny_delete_file("source.txt");
        let landed = fixture.join("landed.txt");
        std::fs::write(&landed, b"x").expect("write");
        let mut set = LandedMetadata::of_source_acl(&source);
        set.merge(LandedMetadata {
            acl: None,
            flags: UserFlags::LOCKED,
        });

        let report = apply_landed(&landed, &set);

        assert_eq!(
            report,
            MetadataReport {
                acl: ClassOutcome::Kept,
                flags: ClassOutcome::Kept,
            }
        );
        assert!(has_acl(&landed));
        assert_eq!(flags(&landed) & LOCKED_FLAG, LOCKED_FLAG);
    }

    #[test]
    fn apply_landed_adds_flags_to_what_the_item_wears() {
        let fixture = MetadataFixture::new("metadata_apply_adds");
        let landed = fixture.rich_file("hidden.txt");
        let set = LandedMetadata {
            acl: None,
            flags: UserFlags::LOCKED,
        };

        apply_landed(&landed, &set);

        let raw = flags(&landed);
        assert_eq!(raw & LOCKED_FLAG, LOCKED_FLAG);
        assert_eq!(raw & libc::UF_HIDDEN, libc::UF_HIDDEN, "Hidden stays");
    }

    #[test]
    fn a_plain_source_has_an_empty_landed_set() {
        let fixture = MetadataFixture::new("metadata_plain_source");
        let plain = fixture.join("plain.txt");
        std::fs::write(&plain, b"x").expect("write");
        assert!(LandedMetadata::of_source_acl(&plain).is_empty());
    }

    #[test]
    fn clear_blockers_hands_back_what_it_cleared() {
        let fixture = MetadataFixture::new("metadata_clear");
        let path = locked_and_acld(&fixture, "both.txt");

        let Unblock::Cleared(cleared) = clear_blockers(&path) else {
            panic!("both blockers should clear");
        };

        assert_eq!(cleared.flags(), UserFlags::LOCKED);
        assert!(cleared.has_acl());
        assert_eq!(flags(&path) & LOCKED_FLAG, 0);
        assert!(!has_acl(&path));
        std::fs::remove_file(&path).expect("nothing refuses the unlink now");
    }

    #[test]
    fn clear_blockers_on_a_plain_file_clears_nothing() {
        let fixture = MetadataFixture::new("metadata_clear_plain");
        let plain = fixture.rich_file("plain.txt");
        assert_eq!(clear_blockers(&plain), Unblock::NothingToClear);
        assert_eq!(
            flags(&plain) & libc::UF_HIDDEN,
            libc::UF_HIDDEN,
            "Hidden isn't a blocker"
        );
    }

    /// The link is what's being removed; the user's Locked target stays Locked.
    #[test]
    fn clear_blockers_never_reaches_through_a_symlink() {
        let fixture = MetadataFixture::new("metadata_clear_link");
        let target = locked_and_acld(&fixture, "target.txt");
        let link = fixture.join("link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        assert_eq!(clear_blockers(&link), Unblock::NothingToClear);

        assert_eq!(flags(&target) & LOCKED_FLAG, LOCKED_FLAG);
        assert!(has_acl(&target));
    }

    #[test]
    fn the_removal_helpers_remove_what_refuses_an_unlink() {
        let fixture = MetadataFixture::new("metadata_remove");
        let locked = fixture.locked_file("locked.txt");
        let acl = fixture.deny_delete_file("acl.txt");
        let dir = fixture.join("dir");
        std::fs::create_dir(&dir).expect("mkdir");
        crate::testing::metadata_fixture::add_deny_delete_acl(&dir);

        remove_file_clearing_blockers(&locked).expect("the Locked file goes");
        remove_file_clearing_blockers(&acl).expect("the `deny delete` file goes");
        remove_dir_clearing_blockers(&dir).expect("the `deny delete` folder goes");

        assert!(!locked.exists() && !acl.exists() && !dir.exists());
    }

    #[test]
    fn the_tree_removal_clears_every_level() {
        let fixture = MetadataFixture::new("metadata_remove_tree");
        let root = fixture.join("tree");
        std::fs::create_dir_all(root.join("inner")).expect("mkdir");
        locked_and_acld(&fixture, "tree/inner/both.txt");
        fixture.locked_file("tree/locked.txt");
        crate::testing::metadata_fixture::add_acl_entry(&root.join("inner"), "everyone deny delete_child");

        remove_tree_clearing_blockers(&root).expect("the whole tree goes");

        assert!(!root.exists());
    }

    #[test]
    fn a_plain_item_never_reaches_the_helper() {
        let fixture = MetadataFixture::new("metadata_plain_remove");
        let plain = fixture.join("plain.txt");
        std::fs::write(&plain, b"x").expect("write");
        let before = helper_calls_on_this_thread();

        remove_file_clearing_blockers(&plain).expect("removed");

        assert_eq!(helper_calls_on_this_thread(), before);
    }
}
