# cmdr-media-meta

What a photo, video, or music file says about itself: EXIF, video capture dates, and media tags, one read per file
answering every field (`MediaFacts`). Pure parsing over `Read + Seek`, plus a byte source over `Volume::read_range`. No
`tauri`, no app types, no wording; Multi-Rename's `[=source.field]` placeholders read it
(`apps/desktop/src-tauri/src/multi_rename/CLAUDE.md`).

## Module map

- `types.rs`: the vocabulary. `MediaField` (`name` / `parse` / `kind` / `source`, `ALL`), `MediaFacts::get`,
  `MediaValue`, `ReadError` (with its `io::Error` round trip), `is_media_name`.
- `read.rs`: `read_media`, dispatching by extension to `exif`, `lofty`, `isobmff.rs` (MP4/MOV dates), and `ebml.rs`
  (MKV/WebM dates and tags). `holes.rs` maps cover art a tag parser would read only to skip.
- `source.rs`: `ByteSource`, `VolumeByteSource` (the async bridge), `BlockReader` (64 KiB blocks, byte budget, holes).

## Must-knows

- **A field's mask name is `MediaField::name` (`exif.taken`); across IPC it's the camelCase variant (`exifTaken`).** The
  frontend keys its wording off the IPC name. ❌ Renaming either breaks saved presets or the sheet.
- **`exif.camera` says the make once**: a model starting with the make's first word stands alone (`Canon EOS R5`).
- **Text is trimmed of spaces and NULs, and blank text is no value**, in `MediaFacts::get`: readers store what the file
  says.
- **`VolumeByteSource::read_at` blocks on the runtime**: ❌ only ever call it on a blocking thread.
- **Every read is bounded**: `MAX_FETCHED_BYTES_PER_FILE` (`TooLarge`), the caller's `Deadline` (`TimedOut`), and its
  cancel flag (`Cancelled`). A reader raises them through `Read` as `ReadError::into_io` and gets them back with
  `ReadError::from_io`, never by matching message text.
- **CR3 and RAF count as media but no reader takes them**: they read as `NotSupported` (an honest "can't read"), never
  as a photo without a date.

Formats, costs per file, and decisions: `DETAILS.md`.
