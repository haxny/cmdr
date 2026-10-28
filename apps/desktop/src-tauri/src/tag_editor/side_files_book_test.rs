//! Which folder is the book's side files go into (one book only, a disc folder's parent), and an
//! undo whose side-file put-back failed staying retryable.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use cmdr_listing::testing::{TestListing, TestListingGuard};
use cmdr_local::reading::list_directory_core;
use cmdr_media_meta::test_fixtures::mp3_frames;
use cmdr_tags::TagField;

use super::apply::{ApplyRequest, apply_with};
use super::apply_test::{done, request, store};
use super::session::{TagEdit, edit, open};
use super::side_files::{SideFile, SideFileChange, SideFileWithheld, book_folder, side_file_changes};
use super::undo::undo_with;
use crate::multi_rename::session::open_for_tags;
use crate::test_support::TestDir;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_string()).collect()
}

#[test]
fn the_book_is_the_folder_opened_or_the_files_folder_and_a_disc_folder_gives_its_parent() {
    let dir = Path::new("/Audiobooks/Book");
    // Files: their folder.
    assert_eq!(book_folder(dir, &names(&["01.mp3", "02.mp3"])), Some(dir.to_path_buf()));
    // One folder opened, discs inside it: that folder.
    assert_eq!(
        book_folder(dir, &names(&["Part/CD 1/01.mp3", "Part/CD 2/01.mp3"])),
        Some(dir.join("Part"))
    );
    // One folder whose tracks all sit in one disc: still that folder.
    assert_eq!(
        book_folder(dir, &names(&["Part/Disc 1/01.mp3"])),
        Some(dir.join("Part"))
    );
    // A disc folder opened, or several of one book: their parent.
    assert_eq!(book_folder(dir, &names(&["Disc 1/01.mp3"])), Some(dir.to_path_buf()));
    assert_eq!(
        book_folder(dir, &names(&["CD1/01.mp3", "cd 2/01.mp3", "DISK 3/01.mp3"])),
        Some(dir.to_path_buf())
    );
    // Files in a disc folder: the folder above it.
    assert_eq!(
        book_folder(&dir.join("Disc 2"), &names(&["01.mp3"])),
        Some(dir.to_path_buf())
    );
    // Not disc names: each is a book of its own.
    for other in ["CDx", "Disc", "Discs 1", "Disc 1a", "Series 1"] {
        assert_eq!(
            book_folder(dir, &names(&[&format!("{other}/01.mp3"), "Other/01.mp3"])),
            None,
            "{other}"
        );
    }
    // Two folders, or a folder and loose files: more than one book.
    assert_eq!(book_folder(dir, &names(&["Series 1/01.mp3", "Series 2/01.mp3"])), None);
    assert_eq!(book_folder(dir, &names(&["Series 1/01.mp3", "loose.mp3"])), None);
    // Files that aren't tracks don't count.
    assert_eq!(
        book_folder(dir, &names(&["Series 1/01.mp3", "notes.txt"])),
        Some(dir.join("Series 1"))
    );
}

/// The mp3s at `files` (paths below a new folder), listed, and a Tags session opened on all its rows.
fn opened(tag: &str, files: &[&str]) -> (TestDir, TestListingGuard, String) {
    let dir = TestDir::new(tag);
    for path in files {
        std::fs::create_dir_all(dir.join(path).parent().expect("a parent")).expect("dirs are made");
        std::fs::write(dir.join(path), mp3_frames()).expect("writes");
    }
    let listing = TestListing::new()
        .volume("root")
        .path(dir.to_path_buf())
        .entries(list_directory_core(&dir, &crate::listing_host::backends::AppListings).expect("the dir lists"))
        .insert(crate::listing_host::listings(), tag);
    let id = open_for_tags(listing.id(), true, None, 0).expect("opens").session_id;
    (dir, listing, id)
}

fn narrator(name: &str) -> TagEdit {
    TagEdit::SetAll {
        field: TagField::Performer,
        values: vec![name.to_string()],
    }
}

fn with_side_files() -> ApplyRequest {
    ApplyRequest {
        side_files: true,
        ..request()
    }
}

#[test]
fn two_book_folders_get_no_side_file_and_the_list_says_why() {
    let (dir, _listing, id) = opened("side-two-books", &["Series 1/01.mp3", "Series 2/01.mp3"]);
    let (_store_dir, store) = store("side-two-books-store");
    open(&id, false).expect("opens");
    edit(&id, vec![narrator("Vladimir Kudla")]).expect("edits");

    assert_eq!(
        side_file_changes(&id).expect("lists"),
        vec![SideFileChange::Withheld {
            file: SideFile::Reader,
            reason: SideFileWithheld::NotOneBook,
        }]
    );
    let report = apply_with(&store, &id, with_side_files()).expect("applies");
    assert_eq!(report.written.len(), 2, "the tags are written");
    assert!(report.side_files_written.is_empty());
    for folder in [dir.to_path_buf(), dir.join("Series 1"), dir.join("Series 2")] {
        assert!(!folder.join("reader.txt").exists(), "nothing in {}", folder.display());
    }
    done(&id);
}

#[test]
fn an_opened_disc_folder_gets_its_side_file_in_the_book_above_it() {
    let (dir, _listing, id) = opened("side-disc", &["Disc 1/01.mp3", "Disc 1/02.mp3"]);
    let (_store_dir, store) = store("side-disc-store");
    open(&id, false).expect("opens");
    edit(&id, vec![narrator("Vladimir Kudla")]).expect("edits");

    let report = apply_with(&store, &id, with_side_files()).expect("applies");

    assert_eq!(
        report.side_files_written,
        vec![dir.join("reader.txt").to_string_lossy().into_owned()]
    );
    assert!(!dir.join("Disc 1").join("reader.txt").exists());
    done(&id);
}

/// Makes `folder` read-only (or writable again).
fn read_only(folder: &Path, on: bool) {
    let mode = if on { 0o555 } else { 0o755 };
    std::fs::set_permissions(folder, std::fs::Permissions::from_mode(mode)).expect("the mode is set");
}

#[test]
fn a_side_file_that_couldnt_be_put_back_is_put_back_by_the_next_undo() {
    // The tracks sit in `Disc 1`, the side file in the book folder above: only the side file's
    // folder goes read-only, so the tracks restore and the side file fails.
    let (dir, _listing, id) = opened("side-undo-retry", &["Disc 1/01.mp3"]);
    let (_store_dir, store) = store("side-undo-retry-store");
    open(&id, false).expect("opens");
    edit(&id, vec![narrator("Vladimir Kudla")]).expect("edits");
    let report = apply_with(&store, &id, with_side_files()).expect("applies");
    let apply_id = report.apply_id.expect("undoable");
    let reader: PathBuf = dir.join("reader.txt");
    assert!(reader.exists());

    read_only(&dir, true);
    let first = undo_with(&store, &apply_id, Some(&id));
    read_only(&dir, false);
    let first = first.expect("undoes the tracks");
    assert_eq!(first.restored, vec![dir.join("Disc 1").join("01.mp3")]);
    assert_eq!(first.failed.len(), 1, "reader.txt couldn't be removed");
    assert!(reader.exists());

    let second = undo_with(&store, &apply_id, Some(&id)).expect("the record is still there");
    assert!(second.restored.contains(&reader), "{second:?}");
    assert!(second.failed.is_empty());
    assert!(!reader.exists(), "the created file is gone");
    done(&id);
}
