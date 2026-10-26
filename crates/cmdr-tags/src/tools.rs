//! Pure clean-up tools over a session's files. Each takes the files in session order and
//! returns new `FileTags`; the input is never mutated.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::model::{FileTags, TagField};
use crate::text::{remove_diacritics, sentence_case, title_case};

/// How [`change_case`] rewrites a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaseTool {
    /// First letter upper, the rest lower.
    Sentence,
    /// Every word starts upper, the rest lower.
    Title,
    /// All lower.
    Lower,
    /// All upper.
    Upper,
}

/// Every tool as one serializable value, so the app and the MCP server share one tool set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TagTool {
    /// [`change_case`] on `fields`.
    Case {
        /// Fields to change.
        fields: Vec<TagField>,
        /// The case to apply.
        case: CaseTool,
    },
    /// [`strip_diacritics`] on `fields`.
    StripDiacritics {
        /// Fields to strip.
        fields: Vec<TagField>,
    },
    /// [`strip_track_prefix`].
    StripTrackPrefix,
    /// [`renumber`] from `start`.
    Renumber {
        /// The first track number.
        start: u32,
    },
    /// [`album_artist_from_artist`].
    AlbumArtistFromArtist,
    /// [`narrator_from_text`].
    NarratorFromText,
    /// [`clear_redundant_titles`].
    ClearRedundantTitles,
    /// [`remove_extras`] for `keys`.
    RemoveExtras {
        /// Native keys of the extras to drop.
        keys: Vec<String>,
    },
}

/// Runs `tool` over `files`; the input is never mutated.
pub fn apply_tool(files: &[FileTags], tool: &TagTool) -> Vec<FileTags> {
    match tool {
        TagTool::Case { fields, case } => change_case(files, fields, *case),
        TagTool::StripDiacritics { fields } => strip_diacritics(files, fields),
        TagTool::StripTrackPrefix => strip_track_prefix(files),
        TagTool::Renumber { start } => renumber(files, *start),
        TagTool::AlbumArtistFromArtist => album_artist_from_artist(files),
        TagTool::NarratorFromText => narrator_from_text(files),
        TagTool::ClearRedundantTitles => clear_redundant_titles(files),
        TagTool::RemoveExtras { keys } => remove_extras(files, keys),
    }
}

/// The fields of `tags` that must not carry diacritics and do: those not kept
/// ([`TagField::keeps_diacritics`]) whose values change under [`remove_diacritics`].
pub fn fields_with_diacritics(tags: &FileTags) -> Vec<TagField> {
    tags.fields
        .iter()
        .filter(|(field, values)| !field.keeps_diacritics() && values.iter().any(|v| remove_diacritics(v) != *v))
        .map(|(field, _)| *field)
        .collect()
}

/// Matches "Čte: Tomáš Jirman, Eva Nováková": a narrator cue, then one or more names joined by
/// ", " / " a " / " and ". A name is 1-4 capitalized words (hyphens and apostrophes allowed) or
/// initials like "J."; words are separated by blanks, never newlines. `(?i)` would let `\p{Lu}`
/// match lowercase too, so the name words switch it off.
static NARRATOR: LazyLock<Regex> = LazyLock::new(|| {
    let word = r"(?-i:\p{Lu}\.|\p{Lu}[\p{L}'’-]*)";
    let name = format!(r"{word}(?:[ \t]+{word}){{0,3}}");
    // A second or later narrator needs at least two words, so "Čte: Jan Novák, Povídky o lásce" stops at Novák.
    let later_name = format!(r"{word}(?:[ \t]+{word}){{1,3}}");
    let pattern = format!(
        r"(?i)\b(?:čte|čtou|cte|ctou|interpret|vypráví|vypravi)[ \t]*:[ \t]*({name}(?:(?:[ \t]*,[ \t]*|[ \t]+(?-i:a|and)[ \t]+){later_name})*)"
    );
    Regex::new(&pattern).expect("narrator regex is valid")
});

/// Separates several narrators inside one matched segment.
static NARRATOR_SEPARATOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ \t]*,[ \t]*|[ \t]+(?:a|and)[ \t]+").expect("separator regex is valid"));

/// Applies `edit` to every value of the given fields; a field that ends up empty is removed.
fn map_fields(files: &[FileTags], fields: &[TagField], edit: impl Fn(&str) -> String) -> Vec<FileTags> {
    files
        .iter()
        .map(|file| {
            let mut out = file.clone();
            for field in fields {
                if let Some(values) = out.fields.get_mut(field) {
                    *values = values.iter().map(|v| edit(v)).filter(|v| !v.is_empty()).collect();
                    if values.is_empty() {
                        out.fields.remove(field);
                    }
                }
            }
            out
        })
        .collect()
}

/// Changes the case of every value in `fields`.
pub fn change_case(files: &[FileTags], fields: &[TagField], tool: CaseTool) -> Vec<FileTags> {
    map_fields(files, fields, |value| match tool {
        CaseTool::Sentence => sentence_case(value),
        CaseTool::Title => title_case(value),
        CaseTool::Lower => value.to_lowercase(),
        CaseTool::Upper => value.to_uppercase(),
    })
}

/// Strips diacritics from `fields`, skipping the ones that keep them (comment, description, www).
pub fn strip_diacritics(files: &[FileTags], fields: &[TagField]) -> Vec<FileTags> {
    let stripped: Vec<TagField> = fields.iter().copied().filter(|f| !f.keeps_diacritics()).collect();
    map_fields(files, &stripped, remove_diacritics)
}

fn is_prefix_separator(c: char) -> bool {
    matches!(c, ' ' | '_' | '.' | '-')
}

/// The leading number of a string, and the rest after it.
fn split_number(text: &str) -> Option<(u32, &str)> {
    let digits = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(text.len());
    let number = text[..digits].parse().ok()?;
    Some((number, &text[digits..]))
}

/// The title without its leading track number, if it starts with the file's track (or the
/// track before it, for a book whose preface is "00").
fn without_track_prefix(title: &str, track: u32) -> Option<String> {
    let (number, rest) = split_number(title)?;
    if number != track && number.checked_add(1) != Some(track) {
        return None;
    }
    let after = rest.trim_start_matches(is_prefix_separator);
    let separators = &rest[..rest.len() - after.len()];
    if separators.is_empty() || after.is_empty() {
        return None;
    }
    Some(if separators.contains('_') {
        after.replace('_', " ")
    } else {
        after.to_string()
    })
}

/// Removes a leading track number from the title when it equals the file's track or track - 1.
/// Files without a numeric track stay as they are.
pub fn strip_track_prefix(files: &[FileTags]) -> Vec<FileTags> {
    files
        .iter()
        .map(|file| {
            let mut out = file.clone();
            let track = file
                .fields
                .get(&TagField::Track)
                .and_then(|v| v.first())
                .and_then(|t| split_number(t.trim()))
                .map(|(n, _)| n);
            let title = file.fields.get(&TagField::Title).and_then(|v| v.first());
            if let (Some(track), Some(title)) = (track, title)
                && let Some(stripped) = without_track_prefix(title, track)
            {
                set_first(&mut out, TagField::Title, &stripped);
            }
            out
        })
        .collect()
}

/// Numbers the files `start`, `start + 1`, ... in session order. Disc is untouched. A single
/// file has its track removed instead: a one-file book has no track number.
pub fn renumber(files: &[FileTags], start: u32) -> Vec<FileTags> {
    if let [only] = files {
        let mut out = only.clone();
        out.fields.remove(&TagField::Track);
        return vec![out];
    }
    files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let mut out = file.clone();
            let number = u32::try_from(index).map_or(u32::MAX, |i| start.saturating_add(i));
            out.fields.insert(TagField::Track, vec![number.to_string()]);
            out
        })
        .collect()
}

/// Sets the album artist to a copy of the artist where there is none yet.
pub fn album_artist_from_artist(files: &[FileTags]) -> Vec<FileTags> {
    files
        .iter()
        .map(|file| {
            let mut out = file.clone();
            if !file.fields.contains_key(&TagField::AlbumArtist)
                && let Some(artist) = file.fields.get(&TagField::Artist)
            {
                out.fields.insert(TagField::AlbumArtist, artist.clone());
            }
            out
        })
        .collect()
}

/// Replaces the first value of `field` (the rest stay), or removes that value when `text` is empty.
fn set_first(tags: &mut FileTags, field: TagField, text: &str) {
    let Some(values) = tags.fields.get_mut(&field) else {
        return;
    };
    if text.is_empty() {
        values.remove(0);
    } else {
        values[0] = text.to_string();
    }
    if values.is_empty() {
        tags.fields.remove(&field);
    }
}

/// Finds "Čte: Name Surname" in `text`; returns the names and the text without the cue.
fn take_narrator(text: &str) -> Option<(Vec<String>, String)> {
    let caps = NARRATOR.captures(text)?;
    let whole = caps.get(0)?;
    let names = NARRATOR_SEPARATOR
        .split(caps.get(1)?.as_str())
        .map(remove_diacritics)
        .collect();
    let mut end = whole.end();
    let tail = &text[end..];
    if tail.starts_with(',') || (tail.starts_with('.') && !tail.starts_with("..")) {
        end += 1;
    }
    let before = text[..whole.start()].trim_end();
    let after = text[end..].trim_start();
    let joined = if before.is_empty() || after.is_empty() {
        format!("{before}{after}")
    } else {
        format!("{before} {after}")
    };
    let trimmed = joined.trim();
    let rest = if trimmed.starts_with("..") {
        trimmed
    } else {
        trimmed.trim_start_matches(['.', ',']).trim_start()
    };
    let rest = rest.to_string();
    Some((names, rest))
}

/// Moves a narrator named in the comment (else the title) to Performer and cuts the cue from
/// its source. A file that already has a Performer is left alone.
pub fn narrator_from_text(files: &[FileTags]) -> Vec<FileTags> {
    files
        .iter()
        .map(|file| {
            let mut out = file.clone();
            if file.fields.contains_key(&TagField::Performer) {
                return out;
            }
            for source in [TagField::Comment, TagField::Title] {
                let Some(text) = file.fields.get(&source).and_then(|v| v.first()) else {
                    continue;
                };
                if let Some((names, rest)) = take_narrator(text) {
                    out.fields.insert(TagField::Performer, names);
                    set_first(&mut out, source, &rest);
                    break;
                }
            }
            out
        })
        .collect()
}

fn fold(text: &str) -> String {
    remove_diacritics(text).to_lowercase()
}

/// Removes the title where it only repeats the album (ignoring case and diacritics).
pub fn clear_redundant_titles(files: &[FileTags]) -> Vec<FileTags> {
    files
        .iter()
        .map(|file| {
            let mut out = file.clone();
            let title = file.fields.get(&TagField::Title).and_then(|v| v.first());
            let album = file.fields.get(&TagField::Album).and_then(|v| v.first());
            if let (Some(title), Some(album)) = (title, album)
                && fold(title) == fold(album)
            {
                out.fields.remove(&TagField::Title);
            }
            out
        })
        .collect()
}

/// Drops the given unmanaged text items (by their format-native keys).
pub fn remove_extras(files: &[FileTags], keys: &[String]) -> Vec<FileTags> {
    files
        .iter()
        .map(|file| {
            let mut out = file.clone();
            out.extras.retain(|key, _| !keys.contains(key));
            out
        })
        .collect()
}
