//! Find/replace and "format a field from a Multi-Rename mask", run over the tag session by
//! Multi-Rename's own engine (`Transform`/`Replace` for the search, `Mask` for the mask), so the
//! wildcard, case and regex rules and the `[N3-]`-style ranges are the ones Multi-Rename has.
//!
//! Both validate (compile the search, parse the mask) before anything is touched. Neither strips
//! diacritics: a result that brings them in shows in `view.diacritics` like a typed edit does.

use std::path::{Path, PathBuf};

use cmdr_media_meta::MediaFacts;
use cmdr_tags::{FileTags, TagField};
use serde::Deserialize;
use unicode_normalization::UnicodeNormalization;

use super::session::{TagEditorError, edit_scoped};
use super::view::TagEditorView;
use crate::multi_rename::mask::{Mask, NumberWidth, RowFacts, RowMedia};
use crate::multi_rename::transform::{CompiledTransform, Replace, Transform};

/// A text tool over tag fields.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum TagTextTool {
    /// Replaces `search` with `replace` in every value of each listed field. Case-insensitive
    /// unless `case_sensitive`; plain search takes Multi-Rename's wildcards, `regex` a regular
    /// expression with `$1` groups. A value that becomes empty is dropped, and a field with no
    /// value left is removed.
    Replace {
        fields: Vec<TagField>,
        search: String,
        replace: String,
        case_sensitive: bool,
        first_only: bool,
        regex: bool,
    },
    /// Rebuilds `field` from a Multi-Rename `mask`. `[N]` is the field's current FIRST value (a
    /// multi-value field loses its other values: the whole field becomes the one rendered value),
    /// `[=tag.album]` and the like read the file's other fields. A file whose mask has no value
    /// for it (a `FieldGap`) is left unchanged; the result is trimmed, and an empty one removes the field.
    Format { field: TagField, mask: String },
    /// Runs the ticked Multi-Rename text options over every value of each listed field, in
    /// Multi-Rename's order: Greek to Latin, Remove diacritics, Normalize Unicode (NFC), so the
    /// results match file renames. Fields not listed (Comment and Description by default in the
    /// UI) are untouched.
    Text {
        fields: Vec<TagField>,
        greek_to_latin: bool,
        remove_diacritics: bool,
        normalize_unicode: bool,
    },
}

/// Runs `tool` over the readable tracks at `scope` (every one for `None`).
pub fn run(session_id: &str, tool: TagTextTool, scope: Option<Vec<String>>) -> Result<TagEditorView, TagEditorError> {
    match tool {
        TagTextTool::Replace {
            fields,
            search,
            replace,
            case_sensitive,
            first_only,
            regex,
        } => {
            let compiled = Transform {
                replace: Some(Replace {
                    search,
                    replace,
                    case_sensitive,
                    first_only,
                    include_extension: false,
                    regex,
                    substitute: false,
                }),
                ..Transform::default()
            }
            .compile()
            .map_err(|_| TagEditorError::BadRegex)?;
            edit_scoped(session_id, scope, move |_, before, _| {
                Ok(replace_in(before, &fields, &compiled))
            })
        }
        TagTextTool::Format { field, mask } => {
            let mask = Mask::parse(&mask).map_err(|error| TagEditorError::BadMask { error })?;
            edit_scoped(session_id, scope, move |paths, before, _| {
                Ok(format_field(paths, before, field, &mask))
            })
        }
        TagTextTool::Text {
            fields,
            greek_to_latin,
            remove_diacritics,
            normalize_unicode,
        } => edit_scoped(session_id, scope, move |_, before, _| {
            let clean = |value: &str| {
                let mut text = value.to_string();
                if greek_to_latin {
                    text = crate::multi_rename::transliterate::greek_to_latin(&text);
                }
                if remove_diacritics {
                    text = cmdr_tags::text::remove_diacritics(&text);
                }
                if normalize_unicode {
                    text = text.nfc().collect();
                }
                text
            };
            Ok(map_values(before, &fields, clean))
        }),
    }
}

/// `files` with `clean` applied to each value of `fields`.
fn map_values(files: &[FileTags], fields: &[TagField], clean: impl Fn(&str) -> String) -> Vec<FileTags> {
    files
        .iter()
        .map(|file| {
            let mut out = file.clone();
            for field in fields {
                if let Some(values) = out.fields.get_mut(field) {
                    *values = values.iter().map(|value| clean(value)).collect();
                }
            }
            out
        })
        .collect()
}

/// `files` with the search run over each value of `fields`, dropping values that end up empty.
fn replace_in(files: &[FileTags], fields: &[TagField], compiled: &CompiledTransform) -> Vec<FileTags> {
    files
        .iter()
        .map(|file| {
            let mut out = file.clone();
            for field in fields {
                if let Some(values) = out.fields.get_mut(field) {
                    *values = values
                        .iter()
                        .map(|value| compiled.apply(value, "").0)
                        .filter(|value| !value.is_empty())
                        .collect();
                    if values.is_empty() {
                        out.fields.remove(field);
                    }
                }
            }
            out
        })
        .collect()
}

/// `files` with `field` rebuilt from `mask`, each file's position in the scope as its counter.
fn format_field(paths: &[PathBuf], files: &[FileTags], field: TagField, mask: &Mask) -> Vec<FileTags> {
    let all_facts: Vec<MediaFacts> = files.iter().map(media_facts_of).collect();
    let number_width = NumberWidth::of(&all_facts);
    paths
        .iter()
        .zip(files)
        .zip(&all_facts)
        .enumerate()
        .map(|(position, ((path, tags), facts))| {
            let current = tags
                .fields
                .get(&field)
                .and_then(|values| values.first())
                .cloned()
                .unwrap_or_default();
            let row = RowFacts {
                file_name: &current,
                is_directory: true,
                parent: &folder_name(path.parent()),
                grandparent: &folder_name(path.parent().and_then(Path::parent)),
                modified: None,
                position,
                media: RowMedia::Read(facts),
                number_width,
            };
            let mut out = tags.clone();
            if let Ok(text) = mask.render(&row) {
                let text = text.trim();
                if text.is_empty() {
                    out.fields.remove(&field);
                } else {
                    out.fields.insert(field, vec![text.to_string()]);
                }
            }
            out
        })
        .collect()
}

fn folder_name(folder: Option<&Path>) -> String {
    folder
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What the `[=tag.*]` placeholders read from `tags`: the text fields' first value (the
/// multi-value fields, performer, composer, translator and web address, all of them), the track
/// and disc and their totals as integers, the year from the first four digits of Date, and Date
/// as stored.
pub(super) fn media_facts_of(tags: &FileTags) -> MediaFacts {
    let first = |field: TagField| tags.fields.get(&field).and_then(|values| values.first()).cloned();
    let all = |field: TagField| tags.fields.get(&field).cloned().unwrap_or_default();
    // `3/12` is track 3 of 12: the N, as the mask side reads it.
    let number = |field: TagField| {
        first(field).and_then(|text| text.split('/').next().and_then(|n| n.trim().parse::<i64>().ok()))
    };
    MediaFacts {
        title: first(TagField::Title),
        artist: first(TagField::Artist),
        album_artist: first(TagField::AlbumArtist),
        album: first(TagField::Album),
        genre: first(TagField::Genre),
        comment: first(TagField::Comment),
        track: number(TagField::Track),
        disc: number(TagField::Disc),
        year: first(TagField::Date).and_then(|date| {
            let digits: String = date.chars().take(4).collect();
            digits.parse::<i64>().ok().filter(|_| digits.len() == 4)
        }),
        performer: all(TagField::Performer),
        composer: all(TagField::Composer),
        publisher: first(TagField::Publisher),
        date: first(TagField::Date),
        total_tracks: number(TagField::TrackTotal),
        total_discs: number(TagField::DiscTotal),
        description: first(TagField::Description),
        translator: all(TagField::Translator),
        www: all(TagField::Www),
        ..MediaFacts::default()
    }
}
