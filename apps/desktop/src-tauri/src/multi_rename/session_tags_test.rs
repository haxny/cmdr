//! A Tags window opened over folders: the audio files below a selected folder, named by their path
//! below the listing's folder, with the caps and refusals that keep a huge or remote folder out.

use super::tags::{TAGS_FOLDER_DEPTH, TAGS_FOLDER_FILE_CAP};
use super::{MultiRenameError, OpenHandshake, close, files_of, held_twin, hold, open_for_tags, release, reopen};
use crate::test_support::TestDir;
use cmdr_listing::ListingId;
use cmdr_listing::testing::{TestListing, TestListingGuard};
use cmdr_local::reading::list_directory_core;

/// A scratch folder with `files` (relative paths, parents made), listed under `volume`.
fn tree(tag: &str, volume: &str, files: &[&str]) -> (TestDir, TestListingGuard) {
    tree_with(tag, volume, files, true)
}

/// [`tree`] with the pane showing hidden files or not.
fn tree_with(tag: &str, volume: &str, files: &[&str], hidden: bool) -> (TestDir, TestListingGuard) {
    let dir = TestDir::new(tag);
    for file in files {
        let path = dir.join(file);
        if file.ends_with('/') {
            std::fs::create_dir_all(path).expect("dirs are made");
            continue;
        }
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs are made");
        std::fs::write(path, b"x").expect("scratch dir is writable");
    }
    let listing = TestListing::new()
        .include_hidden(hidden)
        .volume(volume)
        .path(dir.to_path_buf())
        .entries(list_directory_core(&dir, &crate::listing_host::backends::AppListings).expect("the dir lists"))
        .insert(crate::listing_host::listings(), tag);
    (dir, listing)
}

/// The row `name` is on in the pane (which shows hidden files or not, as the listing was made).
fn row_of(listing: &TestListingGuard, name: &str) -> usize {
    let id = ListingId::new(listing.id());
    let rows = [true, false]
        .into_iter()
        .find_map(|hidden| {
            crate::listing_host::listings()
                .selection_names(&id, hidden, None, 0)
                .ok()
        })
        .expect("the rows");
    rows.iter().position(|row| row == name).expect("the row is listed")
}

fn names_of(session_id: &str) -> Vec<String> {
    files_of(session_id).expect("the session is open").names.to_vec()
}

fn opened(listing: &TestListingGuard, hidden: bool, rows: &[usize]) -> Result<String, MultiRenameError> {
    open_for_tags(listing.id(), hidden, Some(rows), 0).map(|opened| opened.session_id)
}

#[test]
fn a_selected_folder_opens_its_audio_files_including_subfolders() {
    let (_dir, listing) = tree(
        "tags-folder-book",
        "root",
        &[
            "Book/Disc 2/01.mp3",
            "Book/Disc 1/01.mp3",
            "Book/cover.jpg",
            "Book/Disc 1/notes.txt",
        ],
    );
    let id = opened(&listing, true, &[row_of(&listing, "Book")]).expect("the folder opens");
    assert_eq!(names_of(&id), vec!["Book/Disc 1/01.mp3", "Book/Disc 2/01.mp3"]);
    close(&id);
}

#[test]
fn plain_files_and_folders_mix_in_the_order_selected_and_a_file_reached_twice_counts_once() {
    let (_dir, listing) = tree(
        "tags-folder-mix",
        "root",
        &["a.mp3", "Book/02.mp3", "Book/01.mp3", "b.txt"],
    );
    let (a, book, b) = (
        row_of(&listing, "a.mp3"),
        row_of(&listing, "Book"),
        row_of(&listing, "b.txt"),
    );
    let id = opened(&listing, true, &[a, book, book, a, b]).expect("opens");
    // A plain file is taken as it is (the window filters the non-audio ones), as it was before.
    assert_eq!(names_of(&id), vec!["a.mp3", "Book/01.mp3", "Book/02.mp3", "b.txt"]);
    close(&id);
}

#[test]
fn with_no_rows_named_every_row_is_taken_and_its_folders_open() {
    let (_dir, listing) = tree("tags-folder-all", "root", &["a.mp3", "Book/01.mp3"]);
    let session = open_for_tags(listing.id(), true, None, 0).expect("opens").session_id;
    assert_eq!(names_of(&session), vec!["Book/01.mp3", "a.mp3"]);
    close(&session);
}

#[test]
fn a_folder_named_like_audio_is_not_a_track() {
    let (_dir, listing) = tree("tags-folder-lookalike", "root", &["x.mp3/", "y.mp3"]);
    let id = opened(&listing, true, &[row_of(&listing, "x.mp3"), row_of(&listing, "y.mp3")]).expect("opens");
    assert_eq!(names_of(&id), vec!["y.mp3"]);
    close(&id);
}

#[test]
fn more_than_the_cap_refuses_with_too_many_tracks() {
    let names: Vec<String> = (0..=TAGS_FOLDER_FILE_CAP).map(|n| format!("Big/{n:05}.mp3")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let (_dir, listing) = tree("tags-folder-cap", "root", &refs);
    let refused = opened(&listing, true, &[row_of(&listing, "Big")]);
    assert!(
        matches!(refused, Err(MultiRenameError::TooManyTracks { limit }) if limit as usize == TAGS_FOLDER_FILE_CAP),
        "got {refused:?}"
    );
}

#[test]
fn the_cap_itself_is_still_taken() {
    let names: Vec<String> = (0..TAGS_FOLDER_FILE_CAP).map(|n| format!("Big/{n:05}.mp3")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let (_dir, listing) = tree("tags-folder-at-cap", "root", &refs);
    let id = opened(&listing, true, &[row_of(&listing, "Big")]).expect("exactly the cap opens");
    assert_eq!(names_of(&id).len(), TAGS_FOLDER_FILE_CAP);
    close(&id);
}

#[test]
fn hidden_files_follow_include_hidden() {
    let files = ["Book/.secret.mp3", "Book/.Trash/09.mp3", "Book/01.mp3"];
    let (_dir, shown_listing) = tree_with("tags-folder-hidden-shown", "root", &files, true);
    let shown = opened(&shown_listing, true, &[row_of(&shown_listing, "Book")]).expect("opens");
    assert_eq!(
        names_of(&shown),
        vec!["Book/.secret.mp3", "Book/.Trash/09.mp3", "Book/01.mp3"]
    );

    let (_dir2, hidden_listing) = tree_with("tags-folder-hidden-off", "root", &files, false);
    let hidden = opened(&hidden_listing, false, &[row_of(&hidden_listing, "Book")]).expect("opens");
    assert_eq!(names_of(&hidden), vec!["Book/01.mp3"]);
    close(&shown);
    close(&hidden);
}

#[test]
fn folders_below_the_depth_limit_are_left_out() {
    let deep = ["d"; TAGS_FOLDER_DEPTH].join("/");
    let too_deep = ["d"; TAGS_FOLDER_DEPTH + 1].join("/");
    let (_dir, listing) = tree(
        "tags-folder-depth",
        "root",
        &[&format!("Book/{deep}/ok.mp3"), &format!("Book/{too_deep}/no.mp3")],
    );
    let id = opened(&listing, true, &[row_of(&listing, "Book")]).expect("opens");
    assert_eq!(names_of(&id), vec![format!("Book/{deep}/ok.mp3")]);
    close(&id);
}

#[cfg(unix)]
#[test]
fn a_symlinked_folder_is_not_followed() {
    let (dir, listing) = tree("tags-folder-symlink", "root", &["Book/01.mp3", "Elsewhere/99.mp3"]);
    std::os::unix::fs::symlink(dir.join("Elsewhere"), dir.join("Book/link")).expect("the link is made");
    let id = opened(&listing, true, &[row_of(&listing, "Book")]).expect("opens");
    assert_eq!(names_of(&id), vec!["Book/01.mp3"]);
    close(&id);
}

#[test]
fn a_folder_on_a_volume_std_fs_cannot_reach_refuses_with_folders_need_local_disk() {
    let (_dir, listing) = tree("tags-folder-remote", "not-a-local-volume", &["Book/01.mp3", "a.mp3"]);
    let refused = opened(&listing, true, &[row_of(&listing, "Book")]);
    assert!(
        matches!(refused, Err(MultiRenameError::FoldersNeedLocalDisk)),
        "got {refused:?}"
    );
    // Plain files there keep today's behavior.
    let id = opened(&listing, true, &[row_of(&listing, "a.mp3")]).expect("plain files open");
    assert_eq!(names_of(&id), vec!["a.mp3"]);
    close(&id);
}

#[test]
fn held_twin_tells_two_windows_over_the_same_book_apart_from_one_over_another() {
    let (_dir, listing) = tree("tags-folder-twin", "root", &["A/Disc 1/01.mp3", "B/Disc 1/01.mp3"]);
    let (a, b) = (row_of(&listing, "A"), row_of(&listing, "B"));
    let (first, again, other) = (
        opened(&listing, true, &[a]).expect("opens"),
        opened(&listing, true, &[a]).expect("opens"),
        opened(&listing, true, &[b]).expect("opens"),
    );
    hold(&first).expect("held");
    assert_eq!(held_twin(&again), Some(first.clone()));
    assert_eq!(held_twin(&other), None);
    release(&first);
    for id in [first, again, other] {
        close(&id);
    }
}

#[test]
fn reopening_a_sheets_session_leaves_out_a_folder_named_like_audio() {
    let (_dir, listing) = tree("tags-folder-reopen", "root", &["x.mp3/", "y.mp3"]);
    let sheet = super::open(listing.id(), true, None, 0).expect("the sheet opens");
    let copy = reopen(&sheet.session_id).expect("reopens").session_id;
    assert_eq!(names_of(&copy), vec!["y.mp3"]);
    close(&sheet.session_id);
    close(&copy);
}

#[test]
fn appledouble_files_are_never_tracks_even_with_hidden_files_shown() {
    let (_dir, listing) = tree(
        "tags-folder-appledouble",
        "root",
        &["Book/._01.mp3", "Book/01.mp3", "Book/Disc 1/._02.mp3"],
    );
    let id = opened(&listing, true, &[row_of(&listing, "Book")]).expect("opens");
    assert_eq!(names_of(&id), vec!["Book/01.mp3"]);
    close(&id);
}

#[cfg(unix)]
#[test]
fn a_link_to_a_track_already_taken_is_one_row_not_two() {
    let (dir, listing) = tree("tags-folder-alias", "root", &["Book/01.mp3", "Elsewhere/99.mp3"]);
    // `00-link.mp3` sorts before its target, so a name-only dedupe would keep the link and the file.
    std::os::unix::fs::symlink(dir.join("Book/01.mp3"), dir.join("Book/00-link.mp3")).expect("link");
    std::os::unix::fs::symlink(dir.join("Elsewhere/99.mp3"), dir.join("Book/98-a.mp3")).expect("link");
    std::os::unix::fs::symlink(dir.join("Elsewhere/99.mp3"), dir.join("Book/99-b.mp3")).expect("link");
    let id = opened(&listing, true, &[row_of(&listing, "Book")]).expect("opens");
    // The real file wins over its alias; two links to one outside file make one row.
    assert_eq!(names_of(&id), vec!["Book/01.mp3", "Book/98-a.mp3"]);
    close(&id);
}

#[test]
fn a_walk_the_caller_gave_up_on_stops_and_leaves_no_session() {
    let (_dir, listing) = tree("tags-folder-cancel", "root", &["Book/01.mp3", "Book/02.mp3"]);
    let plan = super::plan_for_tags(listing.id(), true, Some(&[row_of(&listing, "Book")]), 0).expect("plans");
    assert!(plan.has_folder());
    let handshake = OpenHandshake::new();
    handshake.give_up();
    let refused = plan.open(&handshake);
    assert!(matches!(refused, Err(MultiRenameError::TimedOut)), "got {refused:?}");
    assert_eq!(sessions_over(&listing), 0);
}

#[test]
fn a_caller_that_gives_up_after_the_session_was_stored_closes_it() {
    // The timer fires between the store and the caller taking the answer: the answer is dropped, so the
    // give-up must take the stored session with it.
    let (_dir, listing) = tree("tags-folder-cancel-late", "root", &["a.mp3"]);
    let plan = super::plan_for_tags(listing.id(), true, Some(&[row_of(&listing, "a.mp3")]), 0).expect("plans");
    let handshake = OpenHandshake::new();
    let opened = plan.open(&handshake).expect("the walk won the race");
    assert_eq!(sessions_over(&listing), 1);
    handshake.give_up();
    assert_eq!(sessions_over(&listing), 0);
    assert!(files_of(&opened.session_id).is_err());
}

#[cfg(unix)]
#[test]
fn unreadable_folders_are_counted_and_the_rest_still_opens() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, listing) = tree("tags-folder-unreadable", "root", &["Book/01.mp3", "Book/Locked/02.mp3"]);
    let locked = dir.join("Book/Locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let readable_anyway = std::fs::read_dir(&locked).is_ok(); // running as root
    let opened = open_for_tags(listing.id(), true, Some(&[row_of(&listing, "Book")]), 0).expect("opens");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
    if !readable_anyway {
        assert_eq!(opened.unreadable, 1);
        assert_eq!(names_of(&opened.session_id), vec!["Book/01.mp3"]);
    }
    close(&opened.session_id);
}

fn sessions_over(listing: &TestListingGuard) -> usize {
    use crate::ignore_poison::IgnorePoison;
    super::SESSIONS
        .lock_ignore_poison()
        .values()
        .filter(|s| s.listing_id == listing.id())
        .count()
}

#[test]
fn a_book_opened_from_its_parent_is_named_after_the_book() {
    let (_dir, listing) = tree(
        "tags-folder-title",
        "root",
        &["Dracula/Disc 1/01.mp3", "Dracula/Disc 2/01.mp3"],
    );
    let id = opened(&listing, true, &[row_of(&listing, "Dracula")]).expect("opens");
    let info = crate::tag_editor::window::hold(&id).expect("holds");
    assert_eq!(info.folder_name, "Dracula");
    release(&id);
    close(&id);
}

#[cfg(unix)]
#[test]
fn a_plain_selected_file_and_a_link_to_it_in_a_selected_folder_are_one_row_whichever_comes_first() {
    let (dir, listing) = tree(
        "tags-folder-plain-alias",
        "root",
        &["01.mp3", "Book/00-link.mp3", "Book/02.mp3"],
    );
    std::fs::remove_file(dir.join("Book/00-link.mp3")).expect("rm");
    std::os::unix::fs::symlink(dir.join("01.mp3"), dir.join("Book/00-link.mp3")).expect("link");
    let (plain, book) = (row_of(&listing, "01.mp3"), row_of(&listing, "Book"));
    for rows in [[plain, book], [book, plain]] {
        let id = opened(&listing, true, &rows).expect("opens");
        let mut names = names_of(&id);
        names.sort();
        assert_eq!(names, vec!["01.mp3", "Book/02.mp3"], "rows {rows:?}");
        close(&id);
    }
}

#[cfg(unix)]
#[test]
fn a_non_utf8_name_counts_as_unreadable_only_when_it_could_be_a_track() {
    use std::os::unix::ffi::OsStrExt;
    let (dir, listing) = tree("tags-folder-nonutf8", "root", &["Book/01.mp3"]);
    for name in [&b"bad\xff.txt"[..], &b"bad\xff.mp3"[..]] {
        if std::fs::write(dir.join("Book").join(std::ffi::OsStr::from_bytes(name)), b"x").is_err() {
            return; // this file system (APFS) refuses names that are not UTF-8
        }
    }
    let opened = open_for_tags(listing.id(), true, Some(&[row_of(&listing, "Book")]), 0).expect("opens");
    assert_eq!(opened.unreadable, 1);
    close(&opened.session_id);
}
