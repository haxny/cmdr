//! Find/replace and Format-from-a-mask over a real tag session.

use cmdr_tags::TagField;
use cmdr_tags::shared::SharedValue;

use super::session::{TagEdit, TagEditorError, close, edit, open};
use super::session_test::{album, content, path_of, set_title};
use super::text_tools::{TagTextTool, run};
use super::view::TagEditorView;
use crate::multi_rename::mask::MaskError;
use crate::multi_rename::session::close as close_rename;

fn replace(fields: &[TagField], search: &str, with: &str) -> TagTextTool {
    TagTextTool::Replace {
        fields: fields.to_vec(),
        search: search.to_string(),
        replace: with.to_string(),
        case_sensitive: false,
        first_only: false,
        regex: false,
    }
}

fn format(field: TagField, mask: &str) -> TagTextTool {
    TagTextTool::Format {
        field,
        mask: mask.to_string(),
    }
}

fn titles(view: &TagEditorView) -> Vec<Option<&str>> {
    view.tracks.iter().map(|row| row.title.as_deref()).collect()
}

fn shared(view: &TagEditorView, field: TagField) -> &SharedValue {
    &view
        .shared
        .iter()
        .find(|row| row.field == field)
        .expect("a row per field")
        .value
}

fn set_all(field: TagField, values: &[&str]) -> TagEdit {
    TagEdit::SetAll {
        field,
        values: values.iter().map(ToString::to_string).collect(),
    }
}

#[test]
fn replace_cast_in_titles() {
    let (dir, _listing, id) = album("tag-text-cast");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "Kniha, cast 1")]).expect("edits");
    let view = run(&id, replace(&[TagField::Title], ", cast ", " 0"), None).expect("replaces");
    assert_eq!(titles(&view), vec![Some("Kniha 01"), Some("Beta"), Some("Gamma")]);
    close(&id);
    close_rename(&id);
}

#[test]
fn replace_regex_groups() {
    let (dir, _listing, id) = album("tag-text-regex");
    open(&id, false).expect("opens");
    edit(
        &id,
        vec![
            set_title(&dir, "a.mp3", "01 Predmluva"),
            set_title(&dir, "b.mp3", "2  Konec"),
        ],
    )
    .expect("edits");
    let tool = TagTextTool::Replace {
        fields: vec![TagField::Title],
        search: r"^(\d+)\s*".to_string(),
        replace: String::new(),
        case_sensitive: false,
        first_only: false,
        regex: true,
    };
    let view = run(&id, tool, None).expect("replaces");
    assert_eq!(titles(&view), vec![Some("Predmluva"), Some("Konec"), Some("Gamma")]);
    close(&id);
    close_rename(&id);
}

#[test]
fn replace_case_insensitive_by_default() {
    let (_dir, _listing, id) = album("tag-text-case");
    open(&id, false).expect("opens");
    let view = run(&id, replace(&[TagField::Title], "alpha", "Omega"), None).expect("replaces");
    assert_eq!(titles(&view)[0], Some("Omega"));
    let sensitive = TagTextTool::Replace {
        fields: vec![TagField::Title],
        search: "beta".to_string(),
        replace: "Zeta".to_string(),
        case_sensitive: true,
        first_only: false,
        regex: false,
    };
    let view = run(&id, sensitive, None).expect("replaces");
    assert_eq!(titles(&view)[1], Some("Beta"), "a case-sensitive search misses");
    close(&id);
    close_rename(&id);
}

#[test]
fn replace_only_listed_fields() {
    let (_dir, _listing, id) = album("tag-text-fields");
    open(&id, false).expect("opens");
    let view = run(&id, replace(&[TagField::Album], "Book", "Novel"), None).expect("replaces");
    assert_eq!(shared(&view, TagField::Album), &SharedValue::Multiple);
    assert_eq!(titles(&view), vec![Some("Alpha"), Some("Beta"), Some("Gamma")]);
    let view = run(&id, replace(&[TagField::Title], "Novel", "X"), None).expect("replaces");
    assert_eq!(titles(&view), vec![Some("Alpha"), Some("Beta"), Some("Gamma")]);
    close(&id);
    close_rename(&id);
}

#[test]
fn replace_multi_value_field_each_value() {
    let (_dir, _listing, id) = album("tag-text-multi");
    open(&id, false).expect("opens");
    edit(&id, vec![set_all(TagField::Genre, &["audiokniha", "drama"])]).expect("edits");
    let tool = TagTextTool::Replace {
        fields: vec![TagField::Genre],
        search: "a".to_string(),
        replace: "4".to_string(),
        case_sensitive: false,
        first_only: true,
        regex: false,
    };
    let view = run(&id, tool, None).expect("replaces");
    assert_eq!(
        shared(&view, TagField::Genre),
        &SharedValue::Same(vec!["4udiokniha".to_string(), "dr4ma".to_string()])
    );
    close(&id);
    close_rename(&id);
}

#[test]
fn replace_to_empty_removes_value() {
    let (_dir, _listing, id) = album("tag-text-empty");
    open(&id, false).expect("opens");
    edit(&id, vec![set_all(TagField::Genre, &["audiokniha", "drama"])]).expect("edits");
    let view = run(&id, replace(&[TagField::Genre], "drama", ""), None).expect("replaces");
    assert_eq!(
        shared(&view, TagField::Genre),
        &SharedValue::Same(vec!["audiokniha".to_string()])
    );
    let view = run(&id, replace(&[TagField::Genre, TagField::Title], "*", ""), None).expect("replaces");
    assert_eq!(shared(&view, TagField::Genre), &SharedValue::Empty, "no values left");
    assert_eq!(titles(&view), vec![None, None, None]);
    close(&id);
    close_rename(&id);
}

#[test]
fn format_range_cuts() {
    let (dir, _listing, id) = album("tag-text-range");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "01 Predmluva")]).expect("edits");
    let view = run(&id, format(TagField::Title, "[N3-]"), None).expect("formats");
    // The cut leaves the separator space, which a tag value doesn't keep.
    assert_eq!(titles(&view)[0], Some("Predmluva"));
    close(&id);
    close_rename(&id);
}

#[test]
fn format_uses_other_fields() {
    let (_dir, _listing, id) = album("tag-text-other");
    open(&id, false).expect("opens");
    let view = run(&id, format(TagField::Title, "[=tag.album] - [N]"), None).expect("formats");
    assert_eq!(
        titles(&view),
        vec![Some("Book - Alpha"), Some("Book - Beta"), Some("Other - Gamma")]
    );
    close(&id);
    close_rename(&id);
}

#[test]
fn format_gap_leaves_the_file_unchanged() {
    let (_dir, _listing, id) = album("tag-text-gap");
    open(&id, false).expect("opens");
    let view = run(&id, format(TagField::Title, "[=tag.genre]"), None).expect("formats");
    assert_eq!(titles(&view), vec![Some("Alpha"), Some("Beta"), Some("Gamma")]);
    close(&id);
    close_rename(&id);
}

#[test]
fn format_bad_mask_is_typed() {
    let (_dir, _listing, id) = album("tag-text-badmask");
    let before = open(&id, false).expect("opens");
    assert_eq!(
        run(&id, format(TagField::Title, "[N"), None),
        Err(TagEditorError::BadMask {
            error: MaskError::Unclosed { at: 0 }
        })
    );
    assert_eq!(
        content(&open(&id, false).expect("a refresh")),
        content(&before),
        "nothing changed"
    );
    close(&id);
    close_rename(&id);
}

#[test]
fn bad_regex_is_typed() {
    let (_dir, _listing, id) = album("tag-text-badregex");
    let before = open(&id, false).expect("opens");
    let tool = TagTextTool::Replace {
        fields: vec![TagField::Title],
        search: "(".to_string(),
        replace: String::new(),
        case_sensitive: false,
        first_only: false,
        regex: true,
    };
    assert_eq!(run(&id, tool, None), Err(TagEditorError::BadRegex));
    assert_eq!(
        content(&open(&id, false).expect("a refresh")),
        content(&before),
        "nothing changed"
    );
    close(&id);
    close_rename(&id);
}

#[test]
fn scope_respected() {
    let (dir, _listing, id) = album("tag-text-scope");
    open(&id, false).expect("opens");
    let scope = Some(vec![path_of(&dir, "b.mp3")]);
    let view = run(&id, replace(&[TagField::Title], "e", "E"), scope.clone()).expect("replaces");
    assert_eq!(titles(&view), vec![Some("Alpha"), Some("BEta"), Some("Gamma")]);
    let view = run(&id, format(TagField::Title, "[U][N]"), scope).expect("formats");
    assert_eq!(titles(&view), vec![Some("Alpha"), Some("BETA"), Some("Gamma")]);
    let unknown = Some(vec![path_of(&dir, "nope.mp3")]);
    assert_eq!(
        run(&id, replace(&[TagField::Title], "a", "b"), unknown),
        Err(TagEditorError::UnknownTrack)
    );
    close(&id);
    close_rename(&id);
}

#[test]
fn diacritics_reported() {
    let (_dir, _listing, id) = album("tag-text-diacritics");
    let view = open(&id, false).expect("opens");
    assert!(view.diacritics.is_empty());
    let view = run(&id, replace(&[TagField::Title], "Alpha", "Čas"), None).expect("replaces");
    assert_eq!(view.diacritics, vec![TagField::Title]);
    assert_eq!(titles(&view)[0], Some("Čas"), "not stripped silently");
    close(&id);
    close_rename(&id);
}
