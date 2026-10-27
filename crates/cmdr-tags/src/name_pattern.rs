//! Reading tags from a file name: an Mp3tag-style `%track% - %title%` pattern or a regular
//! expression with named groups. Pure; the tools module applies the result.

use regex::{Regex, RegexBuilder};
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

/// Compiled-size cap for a pattern (the crate default is 10 MB); a file name needs far less.
const REGEX_SIZE_LIMIT: usize = 1 << 20;

fn build_regex(source: &str) -> Result<Regex, PatternError> {
    RegexBuilder::new(source)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map_err(|_| PatternError::BadRegex)
}

/// A pattern is real when at least one group sets a field (not only `dummy` or unnamed groups).
fn sets_a_field(groups: &[Option<Option<TagField>>]) -> bool {
    groups.iter().any(|g| matches!(g, Some(Some(_))))
}

fn compile_format(pattern: &str) -> Result<CompiledPattern, PatternError> {
    let pattern: String = pattern.nfc().collect();
    // `%%` is a literal percent sign; otherwise `%name%` is a placeholder.
    let token = Regex::new(r"%%|%([^%]+)%").expect("placeholder regex is valid");
    let placeholders = token.captures_iter(&pattern).filter(|c| c.get(1).is_some()).count();
    let mut source = String::from("(?s)^");
    let mut groups = Vec::new();
    let mut at = 0;
    for caps in token.captures_iter(&pattern) {
        let whole = caps.get(0).expect("group 0 always exists");
        source.push_str(&regex::escape(&pattern[at..whole.start()]));
        at = whole.end();
        match caps.get(1) {
            None => source.push_str(&regex::escape("%")),
            Some(name) => {
                groups.push(Some(field_named(name.as_str())?));
                source.push_str(if groups.len() == placeholders { "(.+)" } else { "(.+?)" });
            }
        }
    }
    if !sets_a_field(&groups) {
        return Err(PatternError::NoFields);
    }
    source.push_str(&regex::escape(&pattern[at..]));
    source.push('$');
    Ok(CompiledPattern {
        regex: build_regex(&source)?,
        groups,
    })
}

/// Regex mode matches anywhere in the stem unless the pattern anchors itself with `^` / `$`.
fn compile_regex(pattern: &str) -> Result<CompiledPattern, PatternError> {
    let pattern: String = pattern.nfc().collect();
    let regex = build_regex(&pattern)?;
    let mut groups = Vec::new();
    for name in regex.capture_names().skip(1) {
        groups.push(name.map(field_named).transpose()?);
    }
    if !sets_a_field(&groups) {
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
