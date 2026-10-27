//! Reading tags from a file name: an Mp3tag-style `%track% - %title%` pattern or a regular
//! expression with named groups. Pure; the tools module applies the result.

use regex::Regex;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::model::TagField;

/// How to read tags from a file name (without its extension).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum NamePattern {
    /// Mp3tag-style: literal text with `%field%` placeholders; the last one takes the rest.
    Format {
        /// The pattern, like `%track% - %title%`.
        pattern: String,
    },
    /// A regular expression whose named groups are tag fields (`(?<track>\d+)_(?<title>.+)`).
    Regex {
        /// The expression.
        pattern: String,
    },
}

/// Why a pattern can't be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PatternError {
    /// The regular expression doesn't compile.
    BadRegex,
    /// A placeholder or group names no field.
    #[serde(rename_all = "camelCase")]
    UnknownField {
        /// The unknown name as written.
        name: String,
    },
    /// The pattern has no placeholder or named group.
    NoFields,
}

/// A pattern ready to match stems.
#[derive(Debug, Clone)]
pub struct CompiledPattern {
    regex: Regex,
    /// Per capture group after the whole match: `None` for an unnamed group, `Some(None)` for `dummy`.
    groups: Vec<Option<Option<TagField>>>,
}

/// Compiles `p`, or says why it can't be used.
pub fn compile_pattern(p: &NamePattern) -> Result<CompiledPattern, PatternError> {
    match p {
        NamePattern::Format { pattern } => compile_format(pattern),
        NamePattern::Regex { pattern } => compile_regex(pattern),
    }
}

/// The field a name stands for; `Ok(None)` is `dummy`. Names are the wire names (any case)
/// plus Mp3tag's `albumartist` and `year`.
fn field_named(name: &str) -> Result<Option<TagField>, PatternError> {
    let lower = name.to_lowercase();
    let field = match lower.as_str() {
        "dummy" => return Ok(None),
        "title" => TagField::Title,
        "artist" => TagField::Artist,
        "albumartist" => TagField::AlbumArtist,
        "album" => TagField::Album,
        "performer" => TagField::Performer,
        "composer" => TagField::Composer,
        "date" | "year" => TagField::Date,
        "genre" => TagField::Genre,
        "publisher" => TagField::Publisher,
        "translator" => TagField::Translator,
        "track" => TagField::Track,
        "tracktotal" => TagField::TrackTotal,
        "disc" => TagField::Disc,
        "disctotal" => TagField::DiscTotal,
        "comment" => TagField::Comment,
        "description" => TagField::Description,
        "www" => TagField::Www,
        _ => return Err(PatternError::UnknownField { name: name.to_string() }),
    };
    Ok(Some(field))
}

fn compile_format(pattern: &str) -> Result<CompiledPattern, PatternError> {
    let placeholder = Regex::new(r"%([^%]+)%").expect("placeholder regex is valid");
    let matches: Vec<_> = placeholder.captures_iter(pattern).collect();
    if matches.is_empty() {
        return Err(PatternError::NoFields);
    }
    let mut source = String::from("(?s)^");
    let mut groups = Vec::new();
    let mut at = 0;
    for (i, caps) in matches.iter().enumerate() {
        let whole = caps.get(0).expect("group 0 always exists");
        source.push_str(&regex::escape(&pattern[at..whole.start()]));
        groups.push(Some(field_named(&caps[1])?));
        source.push_str(if i + 1 == matches.len() { "(.+)" } else { "(.+?)" });
        at = whole.end();
    }
    source.push_str(&regex::escape(&pattern[at..]));
    source.push('$');
    let regex = Regex::new(&source).map_err(|_| PatternError::BadRegex)?;
    Ok(CompiledPattern { regex, groups })
}

fn compile_regex(pattern: &str) -> Result<CompiledPattern, PatternError> {
    let regex = Regex::new(pattern).map_err(|_| PatternError::BadRegex)?;
    let mut groups = Vec::new();
    for name in regex.capture_names().skip(1) {
        groups.push(name.map(field_named).transpose()?);
    }
    if groups.iter().all(Option::is_none) {
        return Err(PatternError::NoFields);
    }
    Ok(CompiledPattern { regex, groups })
}

impl CompiledPattern {
    /// The `(field, value)` pairs `stem` yields, in pattern order, or `None` when it doesn't match.
    /// Matching runs on the NFC form; values are trimmed and empty ones dropped.
    pub fn read(&self, stem: &str) -> Option<Vec<(TagField, String)>> {
        let stem: String = stem.nfc().collect();
        let caps = self.regex.captures(&stem)?;
        let pairs = self
            .groups
            .iter()
            .enumerate()
            .filter_map(|(i, group)| {
                let field = (*group)??;
                let value = caps.get(i + 1)?.as_str().trim();
                (!value.is_empty()).then(|| (field, value.to_string()))
            })
            .collect();
        Some(pairs)
    }
}
