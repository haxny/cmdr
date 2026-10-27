//! Tags as a format will hold them, so the editor shows what a write will leave.

use crate::format::AudioFormat;
use crate::model::FileTags;

/// `tags` as `format` holds them. MP4 (`trkn` / `disk`) stores the track and disc numbers and
/// totals as 16-bit integers, so there each becomes a plain integer (`01` → `1`) and one it
/// can't hold (not a number, zero, past 65535) is dropped; the write would fail or store
/// nothing. Every other field, and every field of the other formats, passes unchanged.
pub fn normalize_for(format: AudioFormat, tags: &FileTags) -> FileTags {
    match format {
        AudioFormat::Mp4 => FileTags {
            fields: tags
                .fields
                .iter()
                .filter_map(|(field, values)| {
                    if !field.is_number() {
                        return Some((*field, values.clone()));
                    }
                    let first = values.first()?.trim();
                    let number = first.parse::<u16>().ok().filter(|n| *n > 0)?;
                    Some((*field, vec![number.to_string()]))
                })
                .collect(),
            ..tags.clone()
        },
        AudioFormat::Mp3 | AudioFormat::Flac | AudioFormat::OggVorbis | AudioFormat::Opus => tags.clone(),
    }
}
