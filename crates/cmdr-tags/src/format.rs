//! Which tag container a file uses, decided by its extension.

/// An audio format the editor reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioFormat {
    /// MP3, tagged with ID3v2.3.
    Mp3,
    /// M4A, M4B, and MP4 audio: `ilst` atoms.
    Mp4,
    /// FLAC: Vorbis comments.
    Flac,
    /// Ogg Vorbis: Vorbis comments.
    OggVorbis,
    /// Opus: Vorbis-style comments.
    Opus,
}

/// The format of `file_name`, by extension (case-insensitive), or `None` when
/// the editor doesn't handle it.
pub fn format_of(file_name: &str) -> Option<AudioFormat> {
    let (stem, ext) = file_name.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    match ext.to_ascii_lowercase().as_str() {
        "mp3" => Some(AudioFormat::Mp3),
        "m4a" | "m4b" | "mp4" => Some(AudioFormat::Mp4),
        "flac" => Some(AudioFormat::Flac),
        "ogg" | "oga" => Some(AudioFormat::OggVorbis),
        "opus" => Some(AudioFormat::Opus),
        _ => None,
    }
}
