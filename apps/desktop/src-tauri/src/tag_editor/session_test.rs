//! A tag session against real audio files and a real Multi-Rename session: reads, in-memory
//! edits that follow their files through a reorder, tools over the whole session, and the
//! rules for remote and unreadable files.

use std::path::Path;

use cmdr_listing::testing::{TestListing, TestListingGuard};
use cmdr_local::reading::list_directory_core;
use cmdr_media_meta::test_fixtures::{m4a_file, mp3_frames};
use cmdr_tags::shared::{CoverEdit, SharedValue};
use cmdr_tags::tools::TagTool;
use cmdr_tags::{
    ContentKind, Cover, FileTags, NamePattern, PatternError, TagField, WriteOptions, read_tags, write_tags,
};

use super::session::{
    NamePreviewRow, NameValue, TagEdit, TagEditorError, changes, close, edit, is_open, open, preview_names,
    shared_cover, tool, unchanged_on_disk,
};
use super::view::{CoverSummary, ReadErrorCode, ReadOnlyReason, TagEditorView};
use crate::multi_rename::session::{RowMove, close as close_rename, move_row, open as open_rename};
use crate::test_support::TestDir;

pub(super) fn tags(pairs: &[(TagField, &str)]) -> FileTags {
    let mut tags = FileTags::default();
    for (field, value) in pairs {
        tags.fields.entry(*field).or_default().push((*value).to_string());
    }
    tags
}

/// A folder of `files` (name and bytes), listed under `volume`.
pub(super) fn folder(tag: &str, volume: &str, files: &[(&str, Vec<u8>)]) -> (TestDir, TestListingGuard) {
    let dir = TestDir::new(tag);
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).expect("scratch dir is writable");
    }
    let listing = TestListing::new()
        .volume(volume)
        .path(dir.to_path_buf())
        .entries(list_directory_core(&dir, &crate::listing_host::backends::AppListings).expect("the dir lists"))
        .insert(crate::listing_host::listings(), tag);
    (dir, listing)
}

/// Writes `pairs` into the tag of the audio file `name` in `dir`.
pub(super) fn tag_file(dir: &Path, name: &str, pairs: &[(TagField, &str)]) {
    write_tags(&dir.join(name), &tags(pairs), &WriteOptions::default()).expect("the tags write");
}

/// Three mp3s `a`, `b`, `c` with a title and album each, plus a jpg and a txt.
pub(super) fn album(tag: &str) -> (TestDir, TestListingGuard, String) {
    let (dir, listing) = folder(
        tag,
        "root",
        &[
            ("a.mp3", mp3_frames()),
            ("b.mp3", mp3_frames()),
            ("c.mp3", mp3_frames()),
            ("cover.jpg", vec![1, 2, 3]),
            ("notes.txt", b"x".to_vec()),
        ],
    );
    tag_file(&dir, "a.mp3", &[(TagField::Title, "Alpha"), (TagField::Album, "Book")]);
    tag_file(&dir, "b.mp3", &[(TagField::Title, "Beta"), (TagField::Album, "Book")]);
    tag_file(&dir, "c.mp3", &[(TagField::Title, "Gamma"), (TagField::Album, "Other")]);
    let session = open_rename(listing.id(), true, None, 0).expect("the rename session opens");
    (dir, listing, session.session_id)
}

fn titles(view: &TagEditorView) -> Vec<(String, Option<String>)> {
    view.tracks
        .iter()
        .map(|row| (row.file_name.clone(), row.title.clone()))
        .collect()
}

pub(super) fn path_of(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

pub(super) fn set_title(dir: &Path, name: &str, title: &str) -> TagEdit {
    TagEdit::SetOne {
        path: path_of(dir, name),
        field: TagField::Title,
        values: vec![title.to_string()],
    }
}

pub(super) fn on_disk(dir: &Path, name: &str) -> FileTags {
    read_tags(&dir.join(name)).expect("the file reads")
}

fn shared(view: &TagEditorView, field: TagField) -> &SharedValue {
    &view
        .shared
        .iter()
        .find(|row| row.field == field)
        .expect("a row per field")
        .value
}

#[test]
fn open_lists_only_supported_audio() {
    let (dir, listing) = folder(
        "tag-editor-open",
        "root",
        &[
            ("a.mp3", mp3_frames()),
            ("b.m4a", m4a_file()),
            ("c.jpg", vec![1]),
            ("d.txt", vec![2]),
        ],
    );
    let session = open_rename(listing.id(), true, None, 0).expect("opens");
    let view = open(&session.session_id, false).expect("the tag session opens");
    let names: Vec<&str> = view.tracks.iter().map(|row| row.file_name.as_str()).collect();
    assert_eq!(names, vec!["a.mp3", "b.m4a"]);
    assert!(view.editable);
    assert_eq!(view.read_only_reason, None);
    assert!(!view.dirty);
    drop(dir);
    close(&session.session_id);
    close_rename(&session.session_id);
}

#[test]
fn shared_view_shows_multiple_values() {
    let (_dir, _listing, id) = album("tag-editor-shared");
    let view = open(&id, false).expect("opens");
    assert_eq!(shared(&view, TagField::Album), &SharedValue::Multiple);
    assert_eq!(shared(&view, TagField::Artist), &SharedValue::Empty);
    assert!(
        view.shared
            .iter()
            .all(|row| !matches!(row.field, TagField::Title | TagField::Track))
    );
    assert_eq!(view.cover, CoverSummary::Empty);
    close(&id);
    close_rename(&id);
}

#[test]
fn set_all_writes_every_file_in_memory_only() {
    let (dir, _listing, id) = album("tag-editor-set-all");
    open(&id, false).expect("opens");
    let view = edit(
        &id,
        vec![TagEdit::SetAll {
            field: TagField::Album,
            values: vec!["One book".to_string()],
        }],
    )
    .expect("edits");
    assert_eq!(
        shared(&view, TagField::Album),
        &SharedValue::Same(vec!["One book".to_string()])
    );
    assert!(view.dirty);
    assert!(view.tracks.iter().all(|row| row.changed));
    for name in ["a.mp3", "b.mp3", "c.mp3"] {
        let album = on_disk(&dir, name).fields.get(&TagField::Album).cloned();
        assert_ne!(album, Some(vec!["One book".to_string()]), "{name} is untouched on disk");
    }
    let listed = changes(&id, ContentKind::Audiobook).expect("changes");
    assert_eq!(listed.len(), 3);
    assert_eq!(listed[0].file_name, "a.mp3");
    assert_eq!(listed[0].fields, vec![TagField::Album]);
    close(&id);
    close_rename(&id);
}

/// Review focus 1: reorder after editing, every edit stays with its file.
#[test]
fn edits_follow_files_after_reorder() {
    let (dir, _listing, id) = album("tag-editor-reorder");
    let first = open(&id, false).expect("opens");
    assert_eq!(first.tracks[0].file_name, "a.mp3");

    edit(&id, vec![set_title(&dir, "a.mp3", "Alpha edited")]).expect("edits a");
    move_row(&id, 0, RowMove::Down).expect("a goes below b");
    move_row(&id, 1, RowMove::Down).expect("a goes below c");

    let view = edit(&id, vec![]).expect("a refresh");
    assert_eq!(
        titles(&view),
        vec![
            ("b.mp3".to_string(), Some("Beta".to_string())),
            ("c.mp3".to_string(), Some("Gamma".to_string())),
            ("a.mp3".to_string(), Some("Alpha edited".to_string())),
        ]
    );
    // A track is named by path, wherever its row is now.
    let view = edit(&id, vec![set_title(&dir, "b.mp3", "Beta edited")]).expect("edits b");
    assert_eq!(view.tracks[0].title.as_deref(), Some("Beta edited"));
    assert_eq!(view.tracks[2].title.as_deref(), Some("Alpha edited"));
    close(&id);
    close_rename(&id);
}

#[test]
fn tool_renumber_uses_current_order() {
    let (_dir, _listing, id) = album("tag-editor-renumber");
    open(&id, false).expect("opens");
    move_row(&id, 2, RowMove::Up).expect("c goes above b");
    let view = tool(&id, TagTool::Renumber { start: 1 }, None).expect("renumbers");
    let numbered: Vec<(&str, Option<&str>)> = view
        .tracks
        .iter()
        .map(|row| (row.file_name.as_str(), row.track.as_deref()))
        .collect();
    assert_eq!(
        numbered,
        vec![("a.mp3", Some("01")), ("c.mp3", Some("02")), ("b.mp3", Some("03"))]
    );
    close(&id);
    close_rename(&id);
}

#[test]
fn a_single_file_session_renumbers_to_no_track() {
    let (dir, listing) = folder("tag-editor-single", "root", &[("only.mp3", mp3_frames())]);
    tag_file(&dir, "only.mp3", &[(TagField::Track, "4")]);
    let session = open_rename(listing.id(), true, None, 0).expect("opens");
    let view = open(&session.session_id, false).expect("opens");
    assert_eq!(view.tracks[0].track.as_deref(), Some("4"));
    let view = tool(&session.session_id, TagTool::Renumber { start: 1 }, None).expect("renumbers");
    assert_eq!(view.tracks[0].track, None);
    close(&session.session_id);
    close_rename(&session.session_id);
}

#[test]
fn non_local_session_is_read_only() {
    let (_dir, listing) = folder("tag-editor-remote", "smb-share", &[("a.mp3", mp3_frames())]);
    let session = open_rename(listing.id(), true, None, 0).expect("opens");
    let view = open(&session.session_id, false).expect("opens read-only");
    assert!(!view.editable);
    assert_eq!(view.read_only_reason, Some(ReadOnlyReason::RemoteVolume));
    assert!(view.tracks.is_empty(), "an unreachable volume lists no tracks");
    assert_eq!(edit(&session.session_id, vec![]), Err(TagEditorError::ReadOnly));
    assert_eq!(
        tool(&session.session_id, TagTool::StripTrackPrefix, None),
        Err(TagEditorError::ReadOnly)
    );
    close(&session.session_id);
    close_rename(&session.session_id);
}

#[test]
fn unreadable_file_is_listed_not_written() {
    let (dir, listing) = folder(
        "tag-editor-unreadable",
        "root",
        &[
            ("a.mp3", mp3_frames()),
            ("bad.flac", vec![0x42; 64]),
            ("c.mp3", mp3_frames()),
        ],
    );
    tag_file(&dir, "a.mp3", &[(TagField::Title, "Alpha")]);
    tag_file(&dir, "c.mp3", &[(TagField::Title, "Gamma")]);
    let session = open_rename(listing.id(), true, None, 0).expect("opens");
    let id = session.session_id;
    let view = open(&id, false).expect("opens");
    assert_eq!(view.tracks.len(), 3);
    assert_eq!(view.tracks[1].file_name, "bad.flac");
    assert!(matches!(
        view.tracks[1].read_error,
        Some(ReadErrorCode::Parse | ReadErrorCode::Unreadable | ReadErrorCode::Io)
    ));
    assert_eq!(view.tracks[0].read_error, None);

    // An edit on the unreadable file refuses the whole batch.
    assert_eq!(
        edit(
            &id,
            vec![
                set_title(&dir, "c.mp3", "Gamma edited"),
                set_title(&dir, "bad.flac", "Never")
            ]
        ),
        Err(TagEditorError::UnknownTrack)
    );
    assert!(!open(&id, false).expect("still open").dirty, "nothing was applied");
    let view = edit(&id, vec![set_title(&dir, "c.mp3", "Gamma edited")]).expect("edits");
    assert_eq!(view.tracks[2].title.as_deref(), Some("Gamma edited"));
    assert_eq!(view.tracks[1].title, None);
    assert!(!view.tracks[1].changed);
    let listed = changes(&id, ContentKind::Audiobook).expect("changes");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].file_name, "c.mp3");
    close(&id);
    close_rename(&id);
}

#[test]
fn diacritics_union_reported() {
    let (dir, _listing, id) = album("tag-editor-diacritics");
    let view = open(&id, false).expect("opens");
    assert!(view.diacritics.is_empty());
    let view = edit(
        &id,
        vec![
            set_title(&dir, "a.mp3", "Příběh"),
            TagEdit::SetOne {
                path: path_of(&dir, "b.mp3"),
                field: TagField::Comment,
                values: vec!["Čte: Jan".to_string()],
            },
            TagEdit::SetOne {
                path: path_of(&dir, "c.mp3"),
                field: TagField::Album,
                values: vec!["Labutí".to_string()],
            },
        ],
    )
    .expect("edits");
    // Comment keeps its diacritics, so it is not reported.
    assert_eq!(view.diacritics, vec![TagField::Title, TagField::Album]);
    close(&id);
    close_rename(&id);
}

#[test]
fn cover_edit_is_summarized_and_extras_are_listed() {
    let (_dir, _listing, id) = album("tag-editor-cover");
    open(&id, false).expect("opens");
    let cover = Cover {
        mime: "image/png".to_string(),
        data: vec![0; 12].into(),
    };
    let view = edit(
        &id,
        vec![TagEdit::Cover {
            edit: CoverEdit::Set(cover),
        }],
    )
    .expect("edits");
    assert!(matches!(
        view.cover,
        CoverSummary::Same {
            bytes: 12,
            width: None,
            height: None,
            ..
        }
    ));
    assert!(view.extras.is_empty());
    close(&id);
    close_rename(&id);
}

/// Two covers with the same type and size: the summary's key tells them apart, so the
/// thumbnail follows the new one.
#[test]
fn cover_key_follows_the_bytes() {
    let (_dir, _listing, id) = album("tag-editor-cover-key");
    open(&id, false).expect("opens");
    let key_after = |byte: u8| {
        let cover = Cover {
            mime: "image/png".to_string(),
            data: vec![byte; 12].into(),
        };
        let view = edit(
            &id,
            vec![TagEdit::Cover {
                edit: CoverEdit::Set(cover),
            }],
        )
        .expect("edits");
        match view.cover {
            CoverSummary::Same { key, .. } => key,
            other => panic!("one shared cover, got {other:?}"),
        }
    };
    let first = key_after(0);
    assert_ne!(first, key_after(1));
    assert_eq!(first, key_after(0), "the same bytes keep the key");
    close(&id);
    close_rename(&id);
}

#[test]
fn shared_cover_is_the_common_cover_or_none() {
    let (_dir, _listing, id) = album("tag-editor-shared-cover");
    assert_eq!(shared_cover(&id), Err(TagEditorError::NotOpen));
    open(&id, false).expect("opens");
    assert_eq!(shared_cover(&id), Ok(None));
    let cover = Cover {
        mime: "image/png".to_string(),
        data: vec![1, 2, 3].into(),
    };
    edit(
        &id,
        vec![TagEdit::Cover {
            edit: CoverEdit::Set(cover.clone()),
        }],
    )
    .expect("edits");
    assert_eq!(shared_cover(&id), Ok(Some(cover)));
    close(&id);
    close_rename(&id);
}

#[test]
fn shared_cover_is_none_when_the_files_differ() {
    let (dir, _listing, id) = album("tag-editor-differing-cover");
    let mut one = tags(&[(TagField::Title, "Alpha"), (TagField::Album, "Book")]);
    one.cover = Some(Cover {
        mime: "image/png".to_string(),
        data: vec![9; 4].into(),
    });
    write_tags(&dir.join("a.mp3"), &one, &WriteOptions::default()).expect("writes the cover");
    open(&id, false).expect("opens");
    // One file has a cover, the others none.
    assert_eq!(shared_cover(&id), Ok(None));
    close(&id);
    close_rename(&id);
}

#[test]
fn a_closed_or_unopened_session_is_typed() {
    let (_dir, _listing, id) = album("tag-editor-closed");
    assert_eq!(edit(&id, vec![]), Err(TagEditorError::NotOpen));
    open(&id, false).expect("opens");
    close(&id);
    assert_eq!(tool(&id, TagTool::StripTrackPrefix, None), Err(TagEditorError::NotOpen));
    close_rename(&id);
    assert_eq!(open(&id, false), Err(TagEditorError::NoSession));
}

#[test]
fn open_keeps_edits_unless_restarted() {
    let (dir, _listing, id) = album("tag-editor-restart");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "Alpha edited")]).expect("edits");
    let kept = open(&id, false).expect("reopens");
    assert!(kept.dirty);
    assert_eq!(kept.tracks[0].title.as_deref(), Some("Alpha edited"));
    let fresh = open(&id, true).expect("restarts");
    assert!(!fresh.dirty);
    assert_eq!(fresh.tracks[0].title.as_deref(), Some("Alpha"));
    close(&id);
    close_rename(&id);
}

#[test]
fn an_unknown_path_refuses_the_batch() {
    let (dir, _listing, id) = album("tag-editor-unknown-path");
    open(&id, false).expect("opens");
    let outcome = edit(
        &id,
        vec![
            set_title(&dir, "a.mp3", "Alpha edited"),
            set_title(&dir, "notes.txt", "Never"),
        ],
    );
    assert_eq!(outcome, Err(TagEditorError::UnknownTrack));
    assert!(!open(&id, false).expect("open").dirty);
    close(&id);
    close_rename(&id);
}

#[test]
fn a_file_rewritten_after_open_reports_changed() {
    let (dir, _listing, id) = album("tag-editor-fingerprint");
    open(&id, false).expect("opens");
    let a = dir.join("a.mp3");
    assert_eq!(unchanged_on_disk(&id, &a), Ok(true));
    tag_file(&dir, "a.mp3", &[(TagField::Title, "Rewritten elsewhere")]);
    assert_eq!(unchanged_on_disk(&id, &a), Ok(false));
    assert_eq!(unchanged_on_disk(&id, &dir.join("b.mp3")), Ok(true));
    assert_eq!(unchanged_on_disk(&id, &dir.join("notes.txt")), Ok(false), "not a track");
    close(&id);
    close_rename(&id);
}

#[test]
fn closing_the_rename_session_drops_the_tag_session() {
    let (_dir, _listing, id) = album("tag-editor-closes-with-rename");
    open(&id, false).expect("opens");
    assert!(is_open(&id));
    close_rename(&id);
    assert!(!is_open(&id));
}

#[test]
fn a_tag_session_whose_rename_session_vanished_is_removed_on_next_use() {
    let (_dir, _listing, id) = album("tag-editor-stale");
    open(&id, false).expect("opens");
    // Gone from the rename side without its hook: the stale tag session cleans itself up.
    crate::multi_rename::session::close(&id);
    super::session::reinsert_for_test(&id);
    assert!(is_open(&id));
    assert_eq!(edit(&id, vec![]), Err(TagEditorError::NoSession));
    assert!(!is_open(&id));
}

#[test]
fn a_reachable_non_root_volume_is_listed_but_read_only() {
    use crate::file_system::volume::manager::get_volume_manager;
    use cmdr_local::LocalPosixVolume;
    get_volume_manager().register_if_absent(
        "tag-editor-share",
        std::sync::Arc::new(LocalPosixVolume::local_folder("Share", "/", crate::volume_host::host())),
    );
    let (dir, listing) = folder("tag-editor-share-dir", "tag-editor-share", &[("a.mp3", mp3_frames())]);
    tag_file(&dir, "a.mp3", &[(TagField::Title, "Alpha")]);
    let session = open_rename(listing.id(), true, None, 0).expect("opens");
    let view = open(&session.session_id, false).expect("opens read-only");
    assert!(!view.editable);
    assert_eq!(view.read_only_reason, Some(ReadOnlyReason::RemoteVolume));
    assert_eq!(view.tracks.len(), 1);
    assert_eq!(view.tracks[0].title.as_deref(), Some("Alpha"));
    assert_eq!(edit(&session.session_id, vec![]), Err(TagEditorError::ReadOnly));
    close_rename(&session.session_id);
}

/// An undo another sheet asked for: what this session never edited follows the disk, an edit
/// stays, and a field the disk lost (or gained) follows it too when the session left it alone.
#[test]
fn rebase_keeps_edits_and_follows_the_disk_elsewhere() {
    let old = tags(&[
        (TagField::Title, "Alpha"),
        (TagField::Album, "Kniha"),
        (TagField::Genre, "x"),
    ]);
    let edited = tags(&[
        (TagField::Title, "Alpha mine"),
        (TagField::Album, "Kniha"),
        (TagField::Genre, "x"),
    ]);
    let now = tags(&[
        (TagField::Title, "Alpha"),
        (TagField::Album, "Book"),
        (TagField::Comment, "c"),
    ]);

    let rebased = super::session::rebase(&old, &edited, &now);

    assert_eq!(
        rebased,
        tags(&[
            (TagField::Title, "Alpha mine"),
            (TagField::Album, "Book"),
            (TagField::Comment, "c")
        ])
    );
}

fn numbers(view: &TagEditorView) -> Vec<(&str, Option<&str>)> {
    view.tracks
        .iter()
        .map(|row| (row.file_name.as_str(), row.track.as_deref()))
        .collect()
}

fn format_pattern(pattern: &str) -> NamePattern {
    NamePattern::Format {
        pattern: pattern.to_string(),
    }
}

#[test]
fn tool_runs_on_scope_only() {
    let mp3s: Vec<(&str, Vec<u8>)> = ["a.mp3", "b.mp3", "c.mp3", "d.mp3"]
        .into_iter()
        .map(|name| (name, mp3_frames()))
        .collect();
    let (dir, listing) = folder("tag-editor-scope", "root", &mp3s);
    let id = open_rename(listing.id(), true, None, 0).expect("opens").session_id;
    open(&id, false).expect("opens");
    let scope = vec![path_of(&dir, "d.mp3"), path_of(&dir, "b.mp3")];
    let view = tool(&id, TagTool::Renumber { start: 1 }, Some(scope)).expect("renumbers");
    // Scoped files keep session order, whatever order the scope lists them in.
    assert_eq!(
        numbers(&view),
        vec![
            ("a.mp3", None),
            ("b.mp3", Some("01")),
            ("c.mp3", None),
            ("d.mp3", Some("02"))
        ]
    );
    let unchanged = tool(&id, TagTool::Renumber { start: 5 }, Some(vec![])).expect("an empty scope is a no-op");
    assert_eq!(unchanged, view);
    close(&id);
    close_rename(&id);
}

#[test]
fn unknown_scope_path_is_unknown_track() {
    let (dir, _listing, id) = album("tag-editor-scope-unknown");
    let before = open(&id, false).expect("opens");
    let scope = vec![path_of(&dir, "a.mp3"), path_of(&dir, "nope.mp3")];
    assert_eq!(
        tool(&id, TagTool::Renumber { start: 1 }, Some(scope.clone())),
        Err(TagEditorError::UnknownTrack)
    );
    assert_eq!(
        preview_names(&id, format_pattern("%title%"), Some(scope)),
        Err(TagEditorError::UnknownTrack)
    );
    assert_eq!(open(&id, false).expect("a refresh"), before, "nothing changed");
    close(&id);
    close_rename(&id);
}

#[test]
fn store_normalizes_m4a() {
    let (dir, listing) = folder(
        "tag-editor-m4a",
        "root",
        &[("a.m4a", m4a_file()), ("b.m4a", m4a_file())],
    );
    tag_file(&dir, "a.m4a", &[(TagField::Track, "1")]);
    tag_file(&dir, "b.m4a", &[(TagField::Track, "2")]);
    let id = open_rename(listing.id(), true, None, 0).expect("opens").session_id;
    open(&id, false).expect("opens");
    for _ in 0..2 {
        let view = tool(&id, TagTool::Renumber { start: 1 }, None).expect("renumbers");
        assert_eq!(numbers(&view), vec![("a.m4a", Some("1")), ("b.m4a", Some("2"))]);
        assert!(!view.dirty, "an M4A holds `01` as 1, which is on disk already");
        assert!(changes(&id, ContentKind::Music).expect("changes").is_empty());
    }
    close(&id);
    close_rename(&id);
}

#[test]
fn from_file_name_uses_stems() {
    let (dir, listing) = folder(
        "tag-editor-from-name",
        "root",
        &[("01 - Alpha.mp3", mp3_frames()), ("02 - Beta.mp3", mp3_frames())],
    );
    let id = open_rename(listing.id(), true, None, 0).expect("opens").session_id;
    open(&id, false).expect("opens");
    let pattern = format_pattern("%track% - %title%");
    let scope = vec![path_of(&dir, "02 - Beta.mp3")];
    let view = tool(&id, TagTool::FromFileName { pattern }, Some(scope)).expect("reads the names");
    assert_eq!(titles(&view)[0].1, None, "outside the scope");
    assert_eq!(view.tracks[1].title.as_deref(), Some("Beta"));
    assert_eq!(view.tracks[1].track.as_deref(), Some("02"));
    close(&id);
    close_rename(&id);
}

#[test]
fn bad_pattern_is_typed() {
    let (_dir, _listing, id) = album("tag-editor-bad-pattern");
    let before = open(&id, false).expect("opens");
    let bad = NamePattern::Regex {
        pattern: "(".to_string(),
    };
    let refused = TagEditorError::BadPattern {
        error: PatternError::BadRegex,
    };
    assert_eq!(
        tool(&id, TagTool::FromFileName { pattern: bad.clone() }, None),
        Err(refused.clone())
    );
    assert_eq!(preview_names(&id, bad, None), Err(refused));
    assert_eq!(open(&id, false).expect("a refresh"), before, "nothing changed");
    close(&id);
    close_rename(&id);
}

#[test]
fn preview_names_unmatched_as_none() {
    let (dir, listing) = folder(
        "tag-editor-preview-names",
        "root",
        &[("01 - Alpha.mp3", mp3_frames()), ("notes.mp3", mp3_frames())],
    );
    let id = open_rename(listing.id(), true, None, 0).expect("opens").session_id;
    open(&id, false).expect("opens");
    let rows = preview_names(&id, format_pattern("%track% - %title%"), None).expect("previews");
    assert_eq!(
        rows,
        vec![
            NamePreviewRow {
                path: path_of(&dir, "01 - Alpha.mp3"),
                file_name: "01 - Alpha.mp3".to_string(),
                fields: Some(vec![
                    NameValue {
                        field: TagField::Track,
                        value: "01".to_string(),
                    },
                    NameValue {
                        field: TagField::Title,
                        value: "Alpha".to_string(),
                    },
                ]),
            },
            NamePreviewRow {
                path: path_of(&dir, "notes.mp3"),
                file_name: "notes.mp3".to_string(),
                fields: None,
            },
        ]
    );
    assert!(
        !edit(&id, vec![]).expect("a refresh").dirty,
        "a preview changes nothing"
    );
    close(&id);
    close_rename(&id);
}

/// The single-file rule is about the book: one selected track of a bigger book keeps a number.
#[test]
fn a_one_track_scope_keeps_its_number() {
    let (dir, _listing, id) = album("tag-editor-scope-one");
    open(&id, false).expect("opens");
    let scope = vec![path_of(&dir, "b.mp3")];
    let view = tool(&id, TagTool::Renumber { start: 2 }, Some(scope)).expect("renumbers");
    assert_eq!(
        numbers(&view),
        vec![("a.mp3", None), ("b.mp3", Some("02")), ("c.mp3", None)]
    );
    close(&id);
    close_rename(&id);
}

/// A damaged file is still part of the book: one readable track of two keeps its number.
#[test]
fn an_unreadable_file_still_counts_as_part_of_the_book() {
    let (dir, listing) = folder(
        "tag-editor-book-unreadable",
        "root",
        &[("01.mp3", mp3_frames()), ("02.flac", vec![0x42; 64])],
    );
    tag_file(&dir, "01.mp3", &[(TagField::Track, "4")]);
    let session = open_rename(listing.id(), true, None, 0).expect("opens");
    let id = session.session_id;
    open(&id, false).expect("opens");
    let view = tool(&id, TagTool::Renumber { start: 1 }, None).expect("renumbers");
    assert_eq!(view.tracks[0].track.as_deref(), Some("01"));
    let wizard = TagTool::AutoNumber(cmdr_tags::NumberOptions {
        track_start: 1,
        track_total: false,
        track_pad: true,
        disc: None,
    });
    let view = tool(&id, wizard, None).expect("numbers");
    assert_eq!(view.tracks[0].track.as_deref(), Some("01"));
    close(&id);
    close_rename(&id);
}

/// A total without its number won't be written, so the view and the change list don't show it.
#[test]
fn a_total_without_its_number_is_dropped_in_store() {
    let (dir, _listing, id) = album("tag-editor-orphan-total");
    open(&id, false).expect("opens");
    let edits = vec![
        TagEdit::SetOne {
            path: path_of(&dir, "a.mp3"),
            field: TagField::TrackTotal,
            values: vec!["9".to_string()],
        },
        TagEdit::SetOne {
            path: path_of(&dir, "a.mp3"),
            field: TagField::DiscTotal,
            values: vec!["2".to_string()],
        },
    ];
    let view = edit(&id, edits).expect("edits");
    assert!(!view.dirty, "nothing is left to write");
    assert!(changes(&id, ContentKind::Audiobook).expect("changes").is_empty());
    // With the number, the total stays.
    let edits = vec![
        TagEdit::SetOne {
            path: path_of(&dir, "a.mp3"),
            field: TagField::Track,
            values: vec!["1".to_string()],
        },
        TagEdit::SetOne {
            path: path_of(&dir, "a.mp3"),
            field: TagField::TrackTotal,
            values: vec!["9".to_string()],
        },
    ];
    let view = edit(&id, edits).expect("edits");
    assert!(view.dirty);
    close(&id);
    close_rename(&id);
}
