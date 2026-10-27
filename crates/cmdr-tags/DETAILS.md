# cmdr-tags details

## Field map

One `TagField` maps to a native key per format. Reading also accepts the aliases in the last column; a write removes
those aliases so the file never carries two copies.

| Field       | MP3 (ID3v2.3)                      | M4A/M4B (`ilst`)                       | FLAC / Ogg / Opus | Read aliases            |
| ----------- | ---------------------------------- | -------------------------------------- | ----------------- | ----------------------- |
| Title       | `TIT2`                             | `©nam`                                 | `TITLE`           |                         |
| Artist      | `TPE1`                             | `©ART`                                 | `ARTIST`          |                         |
| AlbumArtist | `TPE2`                             | `aART`                                 | `ALBUMARTIST`     | `ALBUM ARTIST` (Vorbis) |
| Album       | `TALB`                             | `©alb`                                 | `ALBUM`           |                         |
| Performer   | `TXXX:Performer`                   | freeform `PERFORMER`                   | `PERFORMER`       |                         |
| Composer    | `TCOM`                             | `©wrt`                                 | `COMPOSER`        |                         |
| Date        | `TYER` / `TDRC` (lofty downgrades) | `©day`                                 | `DATE`            |                         |
| Genre       | `TCON`                             | freeform `GENRE` (one entry per value) | `GENRE`, repeated | `©gen` (MP4)            |
| Publisher   | `TPUB`                             | freeform `publisher`                   | `PUBLISHER`       |                         |
| Translator  | `TXXX:Translator`                  | freeform `translator`                  | `TRANSLATOR`      |                         |
| Track       | `TRCK`                             | `trkn`                                 | `TRACKNUMBER`     |                         |
| Disc        | `TPOS`                             | `disk`                                 | `DISCNUMBER`      | `TXXX:DiscNumber` (MP3) |
| Comment     | `COMM` (lang `eng`, empty desc.)   | `©cmt`                                 | `COMMENT`         | `TXXX:Comment` (MP3)    |
| Description | `TXXX:Description`                 | freeform `description`                 | `DESCRIPTION`     |                         |
| Www         | `TXXX:www`                         | freeform `www`                         | `WWW`             |                         |

Cover: `APIC` type 3 (MP3), `covr` (M4A), `METADATA_BLOCK_PICTURE` or a FLAC picture block. "The cover" is the first
front-cover picture, else the first untyped (`Other`) picture (`read::cover_position`, shared by reader and writer);
every MP4 `covr` picture is untyped. A typed picture (back cover, booklet, artist, ...) is never the cover: a file with
only those has no cover, setting one adds a type 3 picture beside them, and removing the cover never touches them, so an
undo puts back exactly the front cover it removed. A write replaces or removes exactly the cover and keeps every other
picture, so an MP3 whose only picture is "Other" never ends up with two. A FLAC's pictures are one list for this: the
picture blocks, then the comments' `METADATA_BLOCK_PICTURE`s. A cover equal to the current one (MIME and bytes) is left
as it is, type and description included. With the cover removed, the reader shows the next untyped picture, if any.

Composer holds the author of the music, for audiobooks too (user, 2026-10-26); an audiobook write removes only `TPE3`
(conductor) on MP3. The totals are the fields `TrackTotal` / `DiscTotal` (`WriteOptions::track_total` / `disc_total`
only when the field is absent): a total goes in `TRCK` / `TPOS` as `N/Total`, in `trkn` / `disk`'s total slot, or as a
`TRACKTOTAL` / `DISCTOTAL` key (foobar2000's convention); without one the number is plain. A zero total is no total, and
a total field that isn't a whole number fails the write (`TagError::Write`) when its number is written. Vorbis
`TRACKTOTAL`, `TOTALTRACKS`, `DISCTOTAL`, and `TOTALDISCS` are managed: never extras, read as the total (a key wins over
`N/T`), written back as `TRACKTOTAL` / `DISCTOTAL` only. MP3's disc total is `TPOS`'s, else the legacy
`TXXX:DiscNumber`'s. Every item holding a total is a `Mapped` item of its total field (`totals::*_total_items`), so a
total the reader doesn't show (a second one that differs, one without its number, one that isn't a number) is named by
`WouldDrop` like any hidden value; equal duplicates collapse silently. The reader strips `/N` from track and disc in
every format and keeps the number's text (`03`), except MP4, which stores integers (`normalize_for` shows that before a
write). Vorbis is read without lofty's implicit conversions, which would turn `03` into `3`. An MP3's `TPE3` is read
apart, as crate-private `read::RawTags` (`read_raw`), for the read-back check and the undo store.

M4A freeform atoms live under mean `com.apple.iTunes`. Genre is written only as the freeform `GENRE`, and a write
removes `©gen`: foobar2000 on macOS shows `©gen` and `GENRE` together, doubled; Plexamp reads only `©gen` and so loses
the genre (user's choice, 2026-10-26). Reading prefers the freeform `GENRE` and falls back to `©gen` (values joined with
`"; "`) only when there is no `GENRE`, so a file tagged by iTunes shows its genre and a write moves it to `GENRE`. A
`©gen` whose values differ from the shown genre fails the write as `WouldDrop("©gen")` (below).

Every reader splits a multi-value field's value on `"; "`, Vorbis included (a tagger may write `GENRE=a; b` instead of
repeating the key); the writer splits the caller's values the same way and writes Vorbis values as repeated keys.
Single-value fields are never split. A multi-value field lists each value once, in first-seen order, however many items
carry it (`ALBUMARTIST=X` beside `ALBUM ARTIST=X`, a repeated key, `TPE1` `"A\0A"`): it reads as `["X"]` and is written
once, and the repeats go without a `WouldDrop`, since every value they hold is shown.

`format_of` goes by extension, case-insensitively (`.M4B` is MP4); an unknown extension is `None`, and `read_tags` /
`write_tags` report it as `TagError::NotSupported`.

## Why concrete tag types

lofty's generic `Tag` normalizes keys across formats, which drops the format-native keys the map above depends on
(`TXXX` descriptions, freeform atom names, repeated Vorbis keys). Each format gets its own `Id3v2Tag`, `Ilst`, or
`VorbisComments`, so unmanaged frames, atoms, and keys round-trip untouched.

## How a write works

Each writer parses the original strictly (`ParsingMode::Strict`; Vorbis without implicit conversions, as the reader).
The lenient parse the reader uses drops what lofty can't decode (a `covr` atom with one unknown entry, a bad picture
block, a second `VORBIS_COMMENT` block), and saving from it would delete those from the file; strictly, such a file is
refused as `TagError::Parse`. Reading stays lenient. Then the writer removes everything its reader maps to a managed
field (aliases, duplicates, and case variants of TXXX descriptions, freeform names, and Vorbis keys included), removes
the extras that `FileTags::extras` no longer has a key for, then writes one frame, atom, or key per field. An extra the
file has and `extras` keeps stays as the file has it (its values in `extras` are ignored). An extra in `extras` that the
file lacks is added, so an undo can put back one an edit removed: a `TXXX` with that description (values
null-separated), a freeform atom of that name under mean `com.apple.iTunes` (one UTF-8 entry per value; an extra that
had another mean comes back under the iTunes one), or that Vorbis key once per value (Vorbis extra keys are uppercased
first, as the reader reports them; keys differing only in case merge). An extra the file lacks that can't be added (an
empty key, no values, or a key the reader would take for a managed field or a Vorbis total, like a `PERFORMER` that was
under a foreign MP4 mean) fails the write as `TagError::UnwritableExtra(key)`: never a silent skip. An ID3 extra in
another letter case than the file's `TXXX` renames the frame (the old one is dropped, the new one added). Single-value
fields write their first value only (the reader shows only that one).

**Hidden values never go silently.** Each reader lists every item it maps to a managed field (`read::Mapped`:
`id3_items`, `ilst_items`, `vorbis_items`) and builds the fields from that list; the writer removes all of them. So
before writing it checks the same list against what the reader showed (`read::hidden_items`): an item holding a value
the reader doesn't show is one the write would lose. That covers a second value of a single-value field (`TALB`
`"A\0B"`, a second `TITLE`), a legacy `TXXX:Comment` / `TXXX:DiscNumber` beside `COMM` / `TPOS`, a `COMM` in another
language, a `TYER` beside `TDRC`, and MP4's `©gen` beside the freeform `GENRE`. An item whose values all show (an
identical duplicate, a legacy frame equal to the `COMM`) goes silently; any other fails the write as
`TagError::WouldDrop` naming it (`TXXX:Comment`, `COMM:ces`, `©gen`, `TITLE#2` for a key's second occurrence), together
with any ID3v2.3 losses below, unless `WriteOptions::allow_frame_loss` is set. A file with only the legacy frames reads
them as the fields and is written with `COMM` / `TPOS`.

- **MP3**: text is UTF-8 in memory; lofty writes UTF-16 for v2.3. The date is a `TDRC` timestamp that lofty splits into
  `TYER` (+ `TDAT`, `TIME`); a date it can't parse or hold (month 20, as some taggers leave `2010-20-09`) fails the
  write as `TagError::BadDate(date)`, because lofty would drop it silently; the UI names the date to correct. lofty's
  v2.3 writer discards v2.4-only frames (`TSOP`, `TSOA`, `TSOT`, `TMOO`, `TDRL`, `RVA2`, ...; `write::DROPPED_BY_V23`,
  from lofty 0.25.4's `FRAMES_TO_DISCARD`) and outdated v2.2 IDs, cuts a `TDOR` to its year (`TORY`), and merges `TMCL`
  into `TIPL` (one `IPLS`; a lone `TIPL` round-trips). A write that would lose any of these fails with
  `TagError::WouldDrop` (the IDs) unless `WriteOptions::allow_frame_loss` is set; the app asks the user. The cover is
  replaced where it stands: `FrameList::insert` replaces any frame it considers equal, so it is used only for frames
  just removed.
- **FLAC**: saving the comments rewrites every picture block from the comments' own pictures, so the file's picture
  blocks move into the comments first (block pictures ahead of in-tag ones, keeping the reader's cover).
- **M4A**: lofty panics on an ilst it can't encode, so a cover type outside JPEG/PNG/BMP/GIF is refused up front. lofty
  moves the `stco` / `co64` chunk offsets of every track when the tag grows and shifts `mdat`, an audiobook's chapter
  text track included (verified in `write_test::m4a_chunk_offsets_follow_a_growing_tag` and
  `write_m4b_test::m4b_chapter_track_offsets_follow_a_growing_tag`, lofty 0.25.4, 2026-10-26).
- **MP3 behind junk**: a strict parse doesn't search for an ID3v2 tag after junk bytes, so when only the lenient parse
  finds one the write is refused (`TagError::Parse`) instead of adding a second tag in front.
- **FLAC icons**: lofty keeps one picture per icon type when the pictures are gathered; a file with two fails the write
  as `TagError::WouldDrop(["PICTURE:Icon"])`.
- **FLAC**: SEEKTABLE, APPLICATION, and CUESHEET blocks survive a write byte for byte
  (`write_test::flac_unmanaged_blocks_survive`).

After saving the copy, the writer reads it back (same strict parse) and compares it with what it meant to write: the
fields as the reader reports them (joined values split, numbers and ID3 dates normalized), the cover, the extras, and
the `RawTags` (`TPE3` as kept, removed, or restored). Any difference refuses the write, so whatever lofty silently drops
or changes never replaces the original.

## Safe overwrite

`save_safely` (`save.rs`) resolves symlinks (the link stays a link) and refuses a read-only file. It `fs::copy`s the
original to a fresh, not-yet-existing temp path in the same directory (same volume, so the rename is atomic). On APFS
`fs::copy` then clones the file with `fclonefileat`, so a GB-size M4B costs no data copy; elsewhere it copies. Both keep
permissions and, on macOS, extended attributes such as Finder tags. The tags are saved into the copy, which is read back
and checked (above), fsynced, renamed over the original, and the directory is fsynced. A failure anywhere, a panic
inside lofty included (`catch_unwind`), leaves the original untouched and the temp file removed; a crash leaves the
original or the finished file, never half of one. A directory fsync failing after the rename is only logged: the new
file is already in place. A hard link to the file keeps the old content (the rename makes a new inode).

The read-back exists because lofty 0.25.4's `replace_range` truncates to `old_len - new_len` instead of
`start + new_len` when the edited range ends at EOF and shrinks: a FLAC whose metadata runs to the end of the file came
out cut mid-STREAMINFO (verified in `write_test::corrupting_save_is_refused`, 2026-10-26).

## Serialized shapes

The types the app's IPC carries derive `Serialize` / `Deserialize` in camelCase, like the model types: `FileTags`,
`TagField`, `ContentKind`, `WriteOptions` (`{"kind","trackTotal","discTotal","allowFrameLoss"}`), `RestoreReport`
(`failed` as `[path, error]` pairs), `ApplyId` (a plain string), `CaseTool` (`"sentence"`, ...), and `TagError` as
`{"type": "<variant>", "data": ...}`, an `Io` error's data being `[kind, message]` with the kind's `std::io::ErrorKind`
name (`"NotFound"`; an unknown name reads back as `Other`). Shapes are pinned in `serde_test.rs`.
`WriteOptions::default()` is lossless music: `ContentKind::Music` (an audiobook write deletes `TPE3`, so the caller
picks it on purpose), no totals, no frame loss.

## Undo store layout

`UndoStore` (`undo.rs`) keeps one directory per apply under a root the app owns, named by the caller's `ApplyId` (one
safe path component: not empty, no `/`, `\`, NUL, or `..`, no leading `.`; anything else is
`TagError::Io(InvalidInput, _)`):

```
<root>/<id>/manifest.json   {"version":1,"seq":N,"entries":[{"path","fields","extras","cover":{"file":"<k>.cover","mime"}|null,"raw":{"trackTotal"?,"discTotal"?,"conductor"},"written"?:{"size","modified","inode","changed"}}]}
<root>/<id>/<k>.cover       one file per distinct cover; entries with the same bytes share it
<root>/.<id>.staging/       a record being built
```

- **Staging, then swap.** `record` builds the whole record in `.<id>.staging` (each cover, then the manifest, each via
  `<name>.<random>.tmp`, fsynced and renamed), removes any old `<id>`, and renames the staging directory into place. An
  apply without a readable `manifest.json` doesn't exist: restoring it is `TagError::Io(NotFound, _)`, a damaged one
  `TagError::Parse`. `open` creates the root, removes leftover `.<id>.staging` directories, and inside apply directories
  removes only temp names the store makes (`manifest.json` or `<k>.cover`, optional `.<random>`, `.tmp`). A crash
  between removing an old `<id>` and the rename loses that old record, which was being replaced anyway.
- **Covers are deduplicated by content within a record** (sequential names, matched on the bytes, so no hash collision
  can mix covers up): a 100-chapter M4B stores its cover once.
- **`seq`** orders applies across the store (one more than the highest present). After each `record` the lowest go until
  both `UndoLimits` hold (default 50 applies and 512 MiB, manifests plus covers, each file counted once); the apply just
  recorded is never pruned, and a prune that fails is logged, not returned, since the record is complete. Recording an
  existing `id` replaces it. Staging directories are neither applies nor counted. An apply directory whose manifest
  won't load (damaged, or another manifest version) can't be restored but still counts toward both limits, and goes
  first, oldest by the directory's modification time, so a pile of them can't fill the disk past `max_bytes`.
- **`record(id, paths)` reads the files itself**, each with one parse (`read::read_snapshot`): the `FileTags` and the
  `RawTags` (MP3 `TPE3`) that `FileTags` doesn't hold, so the two can't describe different states of the file. A
  record's `raw` may also carry `trackTotal` / `discTotal` (records that keep the totals there, not in `fields`); the
  reader never fills them. Call it before the apply writes. A file that can't be read fails the whole record; nothing is
  stored.
- **`repath`** takes net moves (recorded path, final path), never the steps of a chain; they apply all at once (a swap
  works), and a move whose source isn't recorded is ignored. A sealed entry takes the fingerprint of the file at its new
  path (a rename moves the ctime), so the caller repaths right after its own renames.
- **`retain(id, keep)`** drops the entries whose path isn't in `keep`, and the covers only they used, so an app that
  recorded every file it meant to write can narrow the record to the files it did write before anyone restores it.
- **`seal(id, written)`** is `retain` plus each file's `Fingerprint` (`fingerprint.rs`: size, mtime, inode, ctime) right
  after the apply wrote it, stored in the manifest as `written` (so it survives a restart; an unsealed entry has none
  and the manifest version stays 1). Why: a record is keyed by path, and a rename after the apply can put another file
  at that path (a swap of `01`/`03`); without the check, undo writes one file's old tags into another.
- **`restore`** writes each entry back as it was (`write::restore_tags`): as `ContentKind::Music` whatever the caller
  passes, totals from its recorded fields (`raw.trackTotal` / `discTotal` for an absent total field), and `TPE3` when it
  had one. Of the caller's `WriteOptions` only `allow_frame_loss` applies. A file not at its path is `missing`; a sealed
  entry whose file no longer has its fingerprint is `changed` and untouched (checked right before its write); any write
  error is `failed` (an extra that can't come back is `TagError::UnwritableExtra`) and the rest continue. `None` in
  `cover` removes the cover again. It never deletes the record; the caller `forget`s it.
- **Limits**: an MP4 extra under a non-iTunes mean comes back under `com.apple.iTunes` (one named like a managed key
  can't come back: `failed`). Vorbis total keys come back under their canonical names (`TOTALTRACKS` as `TRACKTOTAL`).

## Notes for the UI (Plan 2)

- **Apply the no-diacritics rule before writing.** `write_tags` writes values as given; strip every field for which
  `TagField::keeps_diacritics` is false (`tools::strip_diacritics`) before the apply.
- **Wrap every call in the app's `contain_panics`** (`apps/desktop/src-tauri/src/crash_reporter/contain.rs`). The writer
  catches lofty's panics as `TagError::Write`, but a caught panic still files a crash report unless the app contains it.
- **One `UndoStore` writer.** Its calls read and rewrite the store's directories without locking; keep one store behind
  a `Mutex` and never open two on one root.
- **List what the kind removes before writing.** `removed_by_kind(path, kind)` names what a write as `kind` drops beyond
  the fields (`KindRemoval::Conductor`: an MP3's `TPE3` under Audiobook), parsing the file without writing.
- **Show `TagError::WouldDrop` to the user with the option to proceed.** It names what the write would lose (values the
  editor doesn't show, frames ID3v2.3 can't hold); proceeding is the same write with `WriteOptions::allow_frame_loss`.
- **One dispatcher, one check, file length.** `tools::TagTool` (`{"kind":"renumber","start":1}`, camelCase kinds) with
  `apply_tool` runs any tool, so the app and MCP share the set. `tools::fields_with_diacritics` lists the fields that
  must not carry diacritics and do (for the warning before a write). `renumber` pads to the last number's width
  (`tools::track_width`: at least two digits, `001` past 99) and on a single file removes its Track and TrackTotal (a
  one-file book has no track number). `read_duration(path)` reads lofty's audio properties (which `read_tags` skips) and
  is `None` for anything unreadable.
- **Record before the apply writes**: `UndoStore::record(id, paths)` reads the files itself.
