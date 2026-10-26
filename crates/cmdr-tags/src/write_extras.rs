//! Which extras a write removes and adds. An extra the file has and `FileTags::extras`
//! keeps stays as the file has it; one `extras` dropped goes; one the file lacks is added,
//! or, when it can't be, fails the write.

use std::collections::{BTreeMap, BTreeSet};

use crate::format::AudioFormat;
use crate::model::{FileTags, TagError};
use crate::read::{VORBIS_TOTALS, freeform_field, txxx_field, vorbis_field};

/// The keys of the extras the file has (as the reader reports them) that `tags` dropped.
pub(crate) fn dropped_extras(current: &FileTags, tags: &FileTags) -> BTreeSet<String> {
    current
        .extras
        .keys()
        .filter(|key| !tags.extras.contains_key(*key))
        .cloned()
        .collect()
}

/// `tags` with its extras' keys in uppercase, as the Vorbis reader reports them (values of
/// keys that differ only in case merged).
pub(crate) fn upper_extra_keys(tags: &FileTags) -> FileTags {
    let mut extras: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (key, values) in &tags.extras {
        extras
            .entry(key.to_ascii_uppercase())
            .or_default()
            .extend(values.iter().cloned());
    }
    FileTags { extras, ..tags.clone() }
}

/// The extras `tags` has that the file lacks, which the write adds, with their values
/// trimmed and empties dropped, as the reader reports them. One with an empty key, no
/// values, or a key the reader of `format` would take for a managed field (or a Vorbis
/// total) can't be added: [`TagError::UnwritableExtra`].
pub(crate) fn added_extras(
    current: &FileTags,
    tags: &FileTags,
    format: AudioFormat,
) -> Result<Vec<(String, Vec<String>)>, TagError> {
    let managed = |key: &str| match format {
        AudioFormat::Mp3 => txxx_field(key).is_some(),
        AudioFormat::Mp4 => freeform_field(key).is_some(),
        AudioFormat::Flac | AudioFormat::OggVorbis | AudioFormat::Opus => {
            let key = key.to_ascii_uppercase();
            vorbis_field(&key).is_some() || VORBIS_TOTALS.contains(&key.as_str())
        }
    };
    tags.extras
        .iter()
        .filter(|(key, _)| !current.extras.contains_key(*key))
        .map(|(key, values)| {
            let values: Vec<String> = values
                .iter()
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .collect();
            if key.is_empty() || values.is_empty() || managed(key) {
                Err(TagError::UnwritableExtra(key.clone()))
            } else {
                Ok((key.clone(), values))
            }
        })
        .collect()
}
