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

#[test]
fn format_reads_n_of_m_track_and_disc_as_n() {
    let (_dir, _listing, id) = album("tag-text-nofm");
    open(&id, false).expect("opens");
    edit(&id, vec![set_all(TagField::Track, &["3/12"])]).expect("sets the track");
    edit(&id, vec![set_all(TagField::Disc, &["1/2"])]).expect("sets the disc");
    let view = run(&id, format(TagField::Title, "[=tag.disc]-[=tag.track]"), None).expect("formats");
    assert_eq!(titles(&view), vec![Some("1-03"), Some("1-03"), Some("1-03")]);
    close(&id);
    close_rename(&id);
}

fn text(fields: &[TagField], greek: bool, diacritics: bool, nfc: bool) -> TagTextTool {
    TagTextTool::Text {
        fields: fields.to_vec(),
        greek_to_latin: greek,
        remove_diacritics: diacritics,
        normalize_unicode: nfc,
    }
}

#[test]
fn text_greek_to_latin() {
    let (dir, _listing, id) = album("tag-text-greek");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "Αθήνα")]).expect("edits");
    let view = run(&id, text(&[TagField::Title], true, false, false), None).expect("runs");
    assert_eq!(titles(&view), vec![Some("Athina"), Some("Beta"), Some("Gamma")]);
    close(&id);
    close_rename(&id);
}

#[test]
fn text_remove_diacritics_on_title_leaves_comment() {
    let (dir, _listing, id) = album("tag-text-diacritics-field");
    open(&id, false).expect("opens");
    edit(
        &id,
        vec![
            set_title(&dir, "a.mp3", "Žluťoučký"),
            set_all(TagField::Comment, &["Žluťoučký"]),
        ],
    )
    .expect("edits");
    let view = run(&id, text(&[TagField::Title], false, true, false), None).expect("runs");
    assert_eq!(titles(&view)[0], Some("Zlutoucky"));
    assert_eq!(
        shared(&view, TagField::Comment),
        &SharedValue::Same(vec!["Žluťoučký".to_string()]),
        "Comment untouched"
    );
    let view = run(
        &id,
        text(&[TagField::Title, TagField::Comment], false, true, false),
        None,
    )
    .expect("runs");
    assert_eq!(
        shared(&view, TagField::Comment),
        &SharedValue::Same(vec!["Zlutoucky".to_string()]),
        "picking Comment explicitly strips it"
    );
    close(&id);
    close_rename(&id);
}

#[test]
fn text_normalize_unicode_composes() {
    let (dir, _listing, id) = album("tag-text-nfc");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "Cafe\u{301}")]).expect("edits");
    let view = run(&id, text(&[TagField::Title], false, false, true), None).expect("runs");
    assert_eq!(titles(&view)[0], Some("Caf\u{e9}"));
    close(&id);
    close_rename(&id);
}

#[test]
fn text_runs_in_multi_renames_order_and_stays_on_the_selected_tracks() {
    let (dir, _listing, id) = album("tag-text-scope-order");
    open(&id, false).expect("opens");
    // Greek first, then diacritics: the tonos mark left on the Latin `f` is the one the
    // other order (diacritics first, then Greek) never produces, so a swapped order fails here.
    edit(
        &id,
        vec![
            set_title(&dir, "a.mp3", "Ευ\u{301}"),
            set_title(&dir, "b.mp3", "Ευ\u{301}"),
        ],
    )
    .expect("edits");
    let scope = Some(vec![path_of(&dir, "b.mp3")]);
    let view = run(&id, text(&[TagField::Title], true, true, true), scope).expect("runs");
    assert_eq!(titles(&view), vec![Some("Ευ\u{301}"), Some("Ef\u{301}"), Some("Gamma")]);
    close(&id);
    close_rename(&id);
}

#[test]
fn text_is_one_undo_step() {
    let (dir, _listing, id) = album("tag-text-undo");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "Čas")]).expect("edits");
    let ran = run(&id, text(&[TagField::Title, TagField::Album], false, true, false), None).expect("runs");
    assert_eq!(titles(&ran)[0], Some("Cas"));
    let undone = super::history::undo_edit(&id).expect("undoes the run");
    assert_eq!(titles(&undone)[0], Some("Čas"), "one undo reverts the whole run");
    close(&id);
    close_rename(&id);
}

// What a mask's `[=tag.*]` placeholders read from a track's tags.

#[test]
fn media_facts_carry_the_tag_fields_masks_can_name() {
    use cmdr_media_meta::{MediaField, MediaValue};
    let mut tags = cmdr_tags::FileTags::default();
    for (field, values) in [
        (TagField::Performer, vec!["Jan Hartl", "Eva Novak"]),
        (TagField::Composer, vec!["Dvorak"]),
        (TagField::Publisher, vec!["CRo3"]),
        (TagField::Date, vec!["2010-09-20"]),
        (TagField::Track, vec!["3"]),
        (TagField::TrackTotal, vec!["12"]),
        (TagField::Disc, vec!["1"]),
        (TagField::DiscTotal, vec!["2"]),
        (TagField::Description, vec!["About it"]),
        (TagField::Translator, vec!["Pavel Eisner"]),
        (TagField::Www, vec!["https://a.cz", "https://b.cz"]),
    ] {
        tags.fields.insert(field, values.into_iter().map(String::from).collect());
    }
    let facts = super::text_tools::media_facts_of(&tags);
    let text = |field| match facts.get(field) {
        Some(MediaValue::Text(text)) => Some(text.into_owned()),
        _ => None,
    };
    assert_eq!(text(MediaField::TagPerformer).as_deref(), Some("Jan Hartl, Eva Novak"));
    assert_eq!(text(MediaField::TagComposer).as_deref(), Some("Dvorak"));
    assert_eq!(text(MediaField::TagPublisher).as_deref(), Some("CRo3"));
    assert_eq!(text(MediaField::TagDate).as_deref(), Some("2010-09-20"));
    assert_eq!(text(MediaField::TagDescription).as_deref(), Some("About it"));
    assert_eq!(text(MediaField::TagTranslator).as_deref(), Some("Pavel Eisner"));
    assert_eq!(text(MediaField::TagWww).as_deref(), Some("https://a.cz, https://b.cz"));
    assert_eq!(facts.get(MediaField::TagTotalTracks), Some(MediaValue::Number(12)));
    assert_eq!(facts.get(MediaField::TagTotalDiscs), Some(MediaValue::Number(2)));
    assert_eq!(facts.get(MediaField::TagYear), Some(MediaValue::Number(2010)));
}
