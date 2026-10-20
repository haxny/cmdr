use super::*;
use crate::proto::server::nfs3_wire::simple_attrs;

#[test]
fn a_file_maps_with_its_size_date_mode_and_numeric_owner() {
    let mut attrs = simple_attrs(FileType::Regular, 7, 1234);
    attrs.mode = 0o640;
    attrs.mtime = NfsTime {
        seconds: 1_611_909_015,
        nseconds: 0,
    };
    let entry = attrs_to_file_entry("a.txt", "nfs://nas:2049/Public/a.txt", &attrs);
    assert_eq!(entry.name, "a.txt");
    assert_eq!(entry.path, "nfs://nas:2049/Public/a.txt");
    assert!(!entry.is_directory && !entry.is_symlink);
    assert_eq!(entry.size, Some(1234));
    assert_eq!(entry.modified_at, Some(1_611_909_015));
    assert_eq!(entry.permissions, 0o100_640);
    assert_eq!(entry.owner, "1000");
    assert_eq!(entry.created_at, None, "v3 has no creation date");
}

#[test]
fn a_folder_has_no_size_and_a_link_is_the_link() {
    let folder = attrs_to_file_entry(
        "d",
        "nfs://nas:2049/Public/d",
        &simple_attrs(FileType::Directory, 2, 4096),
    );
    assert!(folder.is_directory);
    assert_eq!(folder.size, None);
    let link = attrs_to_file_entry("l", "nfs://nas:2049/Public/l", &simple_attrs(FileType::Symlink, 3, 12));
    assert!(link.is_symlink && !link.is_directory);
    assert_eq!(entry_kind(&simple_attrs(FileType::Symlink, 3, 12)), EntryKind::Symlink);
    assert_eq!(entry_kind(&simple_attrs(FileType::Fifo, 4, 0)), EntryKind::File);
}

#[test]
fn space_reads_available_as_this_users_and_used_as_not_free() {
    let stat = FsStat {
        total_bytes: 1000,
        free_bytes: 300,
        available_bytes: 250,
        total_files: 0,
        free_files: 0,
        available_files: 0,
    };
    assert_eq!(
        fsstat_to_space_info(&stat),
        SpaceInfo::Bounded {
            total_bytes: 1000,
            available_bytes: 250,
            used_bytes: 700
        }
    );
    let hostile = FsStat {
        free_bytes: 5000,
        available_bytes: 9000,
        ..stat
    };
    assert_eq!(
        fsstat_to_space_info(&hostile),
        SpaceInfo::Bounded {
            total_bytes: 1000,
            available_bytes: 1000,
            used_bytes: 0
        }
    );
}
