//! A spec whose name mask is a foobar2000 pattern (`MaskSyntax::Foobar`): it renders through the
//! evaluator, then the same pipeline as a Cmdr mask; an old spec without the field is Cmdr's.

use std::path::Path;

use super::foobar::FoobarError;
use super::mask::RowMedia;
use super::plan::{
    Compiled, InvalidNameReason, MaskSyntax, MediaLookup, MultiRenameSpec, NameEdits, RowStatus, SpecError, preview,
};
use super::transform::CaseChange;
use cmdr_fs::entry::FileEntry;
use cmdr_media_meta::{MediaFacts, MediaField};

const DIR: &str = "/Users/me/Audiobooks/Anne Frank";

/// The user's pattern from the screenshot that showed cut names.
const READ_BY: &str =
    "[%album artist% - ]'('%year%')' %album% '('cte %performer%, %publisher%')' - %disc%$num(%tracknumber%,2)";

fn file(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("{DIR}/{name}"), false, false)
}

fn foobar(name_mask: &str) -> MultiRenameSpec {
    MultiRenameSpec {
        name_mask: name_mask.to_string(),
        extension_mask: "[E]".to_string(),
        search: String::new(),
        replace: String::new(),
        case_sensitive: false,
        first_only: false,
        include_extension: false,
        regex: false,
        substitute: false,
        case: CaseChange::Unchanged,
        remove_diacritics: false,
        greek_to_latin: false,
        normalize_unicode: false,
        mask_syntax: MaskSyntax::Foobar,
    }
}

fn anne_frank() -> MediaFacts {
    MediaFacts {
        album_artist: Some("Anne Frank".to_string()),
        album: Some("Deník".to_string()),
        date: Some("1947".to_string()),
        performer: vec!["Věra Slunéčková".to_string()],
        publisher: Some("audioteka 2011".to_string()),
        track: Some(3),
        ..MediaFacts::default()
    }
}

/// Media facts by file name; a file not listed is `NotMedia`.
struct MediaByName<'a>(Vec<(&'a str, RowMedia<'a>)>);

impl MediaLookup for MediaByName<'_> {
    fn media(&self, entry: &FileEntry) -> RowMedia<'_> {
        self.0
            .iter()
            .find(|(name, _)| *name == entry.name)
            .map_or(RowMedia::NotMedia, |(_, media)| *media)
    }
}

fn run(spec: &MultiRenameSpec, folder: &[FileEntry], media: &MediaByName<'_>) -> Vec<(String, RowStatus)> {
    let compiled = Compiled::new(spec).expect("a valid spec");
    let rows: Vec<(usize, &FileEntry)> = folder.iter().enumerate().collect();
    preview(&compiled, Path::new(DIR), &rows, folder, &NameEdits::default(), media)
        .into_iter()
        .map(|p| (p.new_name, p.status))
        .collect()
}

#[test]
fn a_spec_saved_before_the_syntax_existed_is_a_cmdr_mask() {
    let old = serde_json::json!({
        "nameMask": "[N]", "extensionMask": "[E]", "search": "", "replace": "",
        "caseSensitive": false, "firstOnly": false, "includeExtension": false, "regex": false,
        "substitute": false, "case": "unchanged", "removeDiacritics": false,
    });
    let spec: MultiRenameSpec = serde_json::from_value(old).expect("an old spec loads");
    assert_eq!(spec.mask_syntax, MaskSyntax::Cmdr);
    let round_trip = serde_json::to_value(&spec).expect("serializes");
    assert_eq!(round_trip["maskSyntax"], "cmdr");
    let foobar_spec: MultiRenameSpec =
        serde_json::from_value(serde_json::to_value(foobar("%title%")).expect("serializes")).expect("loads");
    assert_eq!(foobar_spec.mask_syntax, MaskSyntax::Foobar);
}

#[test]
fn a_foobar_name_mask_renders_the_users_pattern_and_keeps_the_extension() {
    let facts = anne_frank();
    let media = MediaByName(vec![("03.mp3", RowMedia::Read(&facts))]);
    assert_eq!(
        run(&foobar(READ_BY), &[file("03.mp3")], &media),
        vec![(
            "Anne Frank - (1947) Deník (cte Věra Slunéčková, audioteka 2011) - 03.mp3".to_string(),
            RowStatus::Ready
        )]
    );
}

#[test]
fn the_pipeline_runs_after_a_foobar_pattern() {
    let facts = anne_frank();
    let media = MediaByName(vec![("03.mp3", RowMedia::Read(&facts))]);
    let spec = MultiRenameSpec {
        remove_diacritics: true,
        search: "audioteka".to_string(),
        replace: "Audioteka".to_string(),
        ..foobar(READ_BY)
    };
    assert_eq!(
        run(&spec, &[file("03.mp3")], &media)[0].0,
        "Anne Frank - (1947) Denik (cte Vera Sluneckova, Audioteka 2011) - 03.mp3"
    );
}

/// Still being read: the row waits. An audio file whose tags can't be read is the same
/// `Unreadable` problem a Cmdr mask gives (never renamed with empty fields); a file that isn't
/// media at all renders its fields empty, as foobar does.
#[test]
fn a_foobar_row_waits_for_its_tags_an_unreadable_one_is_a_problem_and_a_non_media_one_gets_empty_fields() {
    let media = MediaByName(vec![("a.mp3", RowMedia::Reading), ("b.mp3", RowMedia::Unreadable)]);
    let out = run(
        &foobar("[%artist% - ]%filename%"),
        &[file("a.mp3"), file("b.mp3"), file("c.txt")],
        &media,
    );
    assert_eq!(
        out,
        vec![
            ("a.mp3".to_string(), RowStatus::Reading),
            (
                "b.mp3".to_string(),
                RowStatus::Unreadable {
                    field: MediaField::TagArtist
                }
            ),
            ("c.txt".to_string(), RowStatus::Unchanged),
        ]
    );
    // A pattern of the name alone doesn't need the tags: an unreadable file renames as any other.
    let out = run(&foobar("$upper(%filename%)"), &[file("b.mp3")], &media);
    assert_eq!(out[0], ("B.mp3".to_string(), RowStatus::Ready));
}

/// A name part that renders empty (or spaces only) would turn the file into a hidden `.mp3`:
/// it's an empty name, in either syntax, never a ready rename.
#[test]
fn an_empty_name_part_is_an_invalid_name_in_either_syntax() {
    let empty = RowStatus::InvalidName {
        reason: InvalidNameReason::Empty,
    };
    let none = MediaByName(vec![]);
    // foobar's `[N]` is a section of literal text: always dropped.
    assert_eq!(run(&foobar("[N]"), &[file("a.mp3")], &none)[0].1, empty);
    // A field a non-media file has no value for.
    assert_eq!(run(&foobar("%album%"), &[file("c.pdf")], &none)[0].1, empty);
    assert_eq!(run(&foobar("%album%  "), &[file("c.pdf")], &none)[0].1, empty);
    // `%title%` falls back to the file name, so it's never empty.
    assert_eq!(
        run(&foobar("%title%"), &[file("c.pdf")], &none)[0].1,
        RowStatus::Unchanged
    );
    let cmdr = |mask: &str| MultiRenameSpec {
        mask_syntax: MaskSyntax::Cmdr,
        ..foobar(mask)
    };
    assert_eq!(run(&cmdr(""), &[file("a.mp3")], &none)[0].1, empty);
    assert_eq!(run(&cmdr("   "), &[file("a.mp3")], &none)[0].1, empty);
    assert_eq!(run(&cmdr("[N]"), &[file("a.mp3")], &none)[0].1, RowStatus::Unchanged);
}

#[test]
fn a_tag_value_with_a_slash_is_an_invalid_name_through_a_foobar_pattern() {
    let facts = MediaFacts {
        album: Some("AC/DC Live".to_string()),
        ..MediaFacts::default()
    };
    let media = MediaByName(vec![("01.mp3", RowMedia::Read(&facts))]);
    assert!(matches!(
        run(&foobar("%album% - %tracknumber%"), &[file("01.mp3")], &media)[0].1,
        RowStatus::InvalidName {
            reason: InvalidNameReason::DisallowedCharacter { .. }
        }
    ));
}

#[test]
fn a_foobar_pattern_uses_media_only_with_a_tag_field() {
    assert!(Compiled::new(&foobar("%title%")).expect("valid").uses_media());
    assert!(
        !Compiled::new(&foobar("%filename% - %directoryname%"))
            .expect("valid")
            .uses_media()
    );
}

#[test]
fn a_broken_foobar_pattern_is_a_typed_spec_error_with_its_place() {
    let Err(SpecError::Pattern { error }) = Compiled::new(&foobar("Deník %nope%")) else {
        panic!("an unknown field is a pattern error");
    };
    assert_eq!(
        error,
        FoobarError::UnknownField {
            name: "nope".to_string(),
            at: "Deník ".len(),
        }
    );
    // The same text as a Cmdr mask is Cmdr's to judge: `%` is a literal there.
    let cmdr = MultiRenameSpec {
        mask_syntax: MaskSyntax::Cmdr,
        ..foobar("Deník %nope%")
    };
    assert!(Compiled::new(&cmdr).is_ok());
}

#[test]
fn the_extension_mask_stays_cmdr_syntax_under_a_foobar_name_mask() {
    let spec = MultiRenameSpec {
        extension_mask: "[E]_[C]".to_string(),
        ..foobar("%filename%")
    };
    let out = run(&spec, &[file("a.mp3")], &MediaByName(vec![]));
    assert_eq!(out[0].0, "a.mp3_1");
}

/// The seven presets Cmdr ships: key, pattern, and the name each makes for the Anne Frank fixture below.
/// `foobar_preset_vectors.json` is the one source; the frontend's `spec.test.ts` pins `PRESET_SEEDS` to it.
#[derive(serde::Deserialize)]
struct ShippedPreset {
    #[serde(rename = "nameKey")]
    name_key: String,
    pattern: String,
    expected: String,
}

fn shipped_presets() -> Vec<ShippedPreset> {
    serde_json::from_str(include_str!("foobar_preset_vectors.json")).expect("the preset vectors are JSON")
}

fn shipped_spec(pattern: &str) -> MultiRenameSpec {
    MultiRenameSpec {
        remove_diacritics: true,
        ..foobar(pattern)
    }
}

#[test]
fn each_shipped_pattern_names_the_anne_frank_book_as_the_user_files_it() {
    let facts = MediaFacts {
        title: Some("Kapitola třetí".to_string()),
        artist: Some("Anne Frank".to_string()),
        disc: Some(1),
        ..anne_frank()
    };
    let media = MediaByName(vec![("03.mp3", RowMedia::Read(&facts))]);
    let presets = shipped_presets();
    assert_eq!(presets.len(), 7);
    for ShippedPreset {
        name_key,
        pattern,
        expected,
    } in presets
    {
        let out = run(&shipped_spec(&pattern), &[file("03.mp3")], &media);
        assert_eq!(out, vec![(expected, RowStatus::Ready)], "{name_key}");
    }
}

#[test]
fn the_shipped_patterns_all_compile_and_read_the_tags() {
    for ShippedPreset { name_key, pattern, .. } in shipped_presets() {
        let compiled = Compiled::new(&shipped_spec(&pattern)).unwrap_or_else(|e| panic!("{name_key}: {e:?}"));
        assert!(compiled.uses_media(), "{name_key}");
    }
}
