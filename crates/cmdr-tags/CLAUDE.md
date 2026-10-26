# cmdr-tags

The tag editor's engine: read and write the text tags and front cover of audio files (MP3, M4A/M4B, FLAC, Ogg Vorbis,
Opus). Pure file work, no `tauri`, no app types, no wording; the app words and drives it. `cmdr-media-meta` is the
read-only sibling that Multi-Rename uses; this crate is the writer.

## Module map

- `model.rs`: `TagField` (15 fields, `keeps_diacritics`, `is_multi`), `FileTags`, `Cover`, `ContentKind`, `TagError`.
- `format.rs`: `format_of`, by extension. `read.rs`: `read_tags`, one reader per container, the key tables.
- `write.rs`: `write_tags` + `WriteOptions`, each writer the inverse of its reader; `write_extras.rs`; `save.rs`:
  `save_safely`. Test files come from `test_files.rs`.
- `shared.rs`: the foobar2000-style view, edits, and change list. `tools.rs` / `text.rs`: pure clean-up tools.
- `undo.rs`: `UndoStore`, an apply's old tags, written back after renames.

## Must-knows

- **An empty field is an absent key**: a write removes it, never writes `""`, `None`, `n/a`, or "Unknown ...".
- **`write_tags` writes values as given.** The no-diacritics rule (all fields but `Comment`, `Description`, `Www`) is
  applied by the tools and the app, before writing.
- **Artist = album artist = author; Performer = narrator; Composer is removed for audiobooks** (`ContentKind`).
- **Genre is multi-value**; one-string formats join values with `"; "`, and every reader splits on it.
- **Track and disc are plain**; `N/Total` only when the caller passes a total.
- **MP3 is written as ID3v2.3.** Unmanaged frames, atoms, and keys stay untouched; unmanaged text items are
  `FileTags::extras`.
- **No silent loss**: a value the reader hides that differs from the one it shows, or a frame ID3v2.3 can't hold, fails
  the write as `TagError::WouldDrop` unless `allow_frame_loss`.
- **The cover is a front cover, else an untyped picture**; back covers and booklets are never touched.
- **Safe overwrite only** (`save_safely`): strict parse, write a temp clone, read back and compare, rename. ❌ Never
  write in place: lofty has corrupted files and dropped items silently.
- **Concrete lofty tag types, not the generic `Tag`**, which loses format-native keys.
- **`UndoStore` has one writer at a time**; callers serialize access.
- **Errors are typed** (`TagError`); ❌ never classify one by its message text.

Field map, write mechanics, undo layout, serialized shapes, and notes for the UI: `DETAILS.md`.
