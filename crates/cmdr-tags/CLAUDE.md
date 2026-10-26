# cmdr-tags

The tag editor's engine: read and write the text tags and front cover of audio files (MP3, M4A/M4B, FLAC, Ogg Vorbis,
Opus). Pure file work, no `tauri`, no app types, no wording; the app words and drives it. `cmdr-media-meta` is the
read-only sibling that Multi-Rename uses; this crate is the writer.

## Module map

- `model.rs`: the vocabulary. `TagField` (15 managed fields, `ALL`, `keeps_diacritics`, `is_multi`), `FileTags`
  (`fields`, `cover`, `extras`), `Cover`, `ContentKind`, `TagError`.
- `format.rs`: `AudioFormat` and `format_of`, by extension only.
- `read.rs`: `read_tags`, one private reader per container (ID3v2, MP4 `ilst`, Vorbis comments). Test files are built in
  `test_files.rs` with lofty's concrete tag types.
- Later modules (write, tools, undo) land with their tasks; the field map and the undo store layout are in `DETAILS.md`.

## Must-knows

- **An empty field is an absent key.** `FileTags::fields` never holds an empty `Vec` or an empty string, and a write
  removes the frame, atom, or key instead of writing `""`, `None`, `n/a`, or "Unknown ...".
- **No diacritics in any written text field except `Comment`, `Description`, and `Www`** (`TagField::keeps_diacritics`).
  Stripping happens on write, so a value that reads back with diacritics came from a file we didn't write.
- **Artist = album artist = author; Performer = narrator; Composer is removed for audiobooks** (`ContentKind`).
- **Genre is multi-value**: repeated Vorbis keys, one freeform data entry per value; where a format stores one string,
  values join with `"; "`.
- **Track is plain** (`3`); `3/12` only when the caller passes a known total.
- **MP3 is written as ID3v2.3**, and frames, atoms, and keys we don't manage are left untouched on write. Unmanaged text
  items surface read-only in `FileTags::extras`.
- **Concrete lofty tag types, not the generic `Tag`**: the generic one loses format-native keys we depend on (why, in
  `DETAILS.md`).
- **A write is safe-overwrite**: temp copy in the same directory, fsync, rename over the original. ❌ Never write in
  place.
- **`format_of` goes by extension, case-insensitively.** An unknown extension is `None`, reported as
  `TagError::NotSupported`.
- **Errors are typed** (`TagError`); ❌ never classify one by its message text.

Field map, format quirks, and decisions: `DETAILS.md`.
