# cmdr-tags

The tag editor's engine: read and write the text tags and front cover of audio files (MP3, M4A/M4B, FLAC, Ogg Vorbis,
Opus). Pure file work, no `tauri`, no app types, no wording; the app words and drives it. `cmdr-media-meta` is the
read-only sibling that Multi-Rename uses; this crate is the writer.

## Module map

- `model.rs`: the vocabulary. `TagField` (15 managed fields, `ALL`, `keeps_diacritics`, `is_multi`), `FileTags`
  (`fields`, `cover`, `extras`), `Cover`, `ContentKind`, `TagError`.
- `format.rs`: `AudioFormat` and `format_of`, by extension only.
- `read.rs`: `read_tags`, one private reader per container (ID3v2, MP4 `ilst`, Vorbis comments), and the key tables.
- `write.rs`: `write_tags` + `WriteOptions`, each writer the exact inverse of its reader. `save.rs`: `save_safely`. Test
  files are built in `test_files.rs` with lofty's concrete tag types.
- `shared.rs`: the foobar2000-style view. `shared_view` (per field `Empty`/`Same`/`Multiple`), `apply_edits` (`Edit`,
  `CoverEdit`), `changes` (the "what will change" list). Pure, never mutates input.
- Later modules (undo) land with their tasks; the undo store layout will be in `DETAILS.md`.

## Must-knows

- **An empty field is an absent key.** `FileTags::fields` never holds an empty `Vec` or an empty string, and a write
  removes the frame, atom, or key instead of writing `""`, `None`, `n/a`, or "Unknown ...".
- **No diacritics in any written text field except `Comment`, `Description`, and `Www`** (`TagField::keeps_diacritics`).
  The clean-up tools strip them; `write_tags` writes values as given.
- **Artist = album artist = author; Performer = narrator; Composer is removed for audiobooks** (`ContentKind`).
- **Genre is multi-value**: repeated Vorbis keys, one freeform data entry per value; where a format stores one string,
  values join with `"; "`.
- **Track is plain** (`3`); `3/12` only when the caller passes a known total.
- **MP3 is written as ID3v2.3**, and frames, atoms, and keys we don't manage are left untouched on write. Unmanaged text
  items surface read-only in `FileTags::extras`; dropping a key there removes it on write.
- **Concrete lofty tag types, not the generic `Tag`**: the generic one loses format-native keys we depend on (why, in
  `DETAILS.md`).
- **A write is safe-overwrite** (`save_safely`): temp clone in the same directory, save, read back and compare with the
  intended tags, fsync, rename over the original. ❌ Never write in place. lofty has corrupted files and dropped items
  silently; the strict parse before and the comparison after are the guards (`DETAILS.md`).
- **No silent loss on MP3**: frames ID3v2.3 can't hold fail the write as `TagError::WouldDrop` unless the caller sets
  `allow_frame_loss`.
- **`format_of` goes by extension, case-insensitively.** An unknown extension is `None`, reported as
  `TagError::NotSupported`.
- **Errors are typed** (`TagError`); ❌ never classify one by its message text.

Field map, format quirks, and decisions: `DETAILS.md`.
