# cmdr-tags details

## Field map

One `TagField` maps to a native key per format. Reading also accepts the aliases in the last column; a write removes
those aliases so the file never carries two copies.

| Field       | MP3 (ID3v2.3)                      | M4A/M4B (`ilst`)                                | FLAC / Ogg / Opus | MP3 read aliases  |
| ----------- | ---------------------------------- | ----------------------------------------------- | ----------------- | ----------------- |
| Title       | `TIT2`                             | `©nam`                                          | `TITLE`           |                   |
| Artist      | `TPE1`                             | `©ART`                                          | `ARTIST`          |                   |
| AlbumArtist | `TPE2`                             | `aART`                                          | `ALBUMARTIST`     |                   |
| Album       | `TALB`                             | `©alb`                                          | `ALBUM`           |                   |
| Performer   | `TXXX:Performer`                   | freeform `PERFORMER`                            | `PERFORMER`       |                   |
| Composer    | `TCOM` (removed for audiobooks)    | `©wrt` (removed for audiobooks)                 | `COMPOSER`        |                   |
| Date        | `TYER` / `TDRC` (lofty downgrades) | `©day`                                          | `DATE`            |                   |
| Genre       | `TCON`                             | freeform `GENRE` (one entry per value) + `©gen` | `GENRE`, repeated |                   |
| Publisher   | `TPUB`                             | freeform `publisher`                            | `PUBLISHER`       |                   |
| Translator  | `TXXX:Translator`                  | freeform `translator`                           | `TRANSLATOR`      |                   |
| Track       | `TRCK`                             | `trkn`                                          | `TRACKNUMBER`     |                   |
| Disc        | `TPOS`                             | `disk`                                          | `DISCNUMBER`      | `TXXX:DiscNumber` |
| Comment     | `COMM` (lang `eng`, empty desc.)   | `©cmt`                                          | `COMMENT`         | `TXXX:Comment`    |
| Description | `TXXX:Description`                 | freeform `description`                          | `DESCRIPTION`     |                   |
| Www         | `TXXX:www`                         | freeform `www`                                  | `WWW`             |                   |

Cover: `APIC` type 3 (MP3), `covr` (M4A), `METADATA_BLOCK_PICTURE` or a FLAC picture block. "The cover" is the first
front-cover picture, else the first picture (`read::cover_position`, shared by reader and writer); a write replaces or
removes exactly that one and keeps every other picture, so an MP3 whose only picture is "Other" never ends up with two.
A FLAC's pictures are one list for this: the picture blocks, then the comments' `METADATA_BLOCK_PICTURE`s. A cover equal
to the current one (MIME and bytes) is left as it is, type and description included. With the cover removed, the reader
shows the next picture as the cover.

Audiobooks also lose `TPE3` (conductor) on MP3. A known total (`WriteOptions::track_total` / `disc_total`) goes in
`TRCK` / `TPOS` as `N/Total`, in `trkn` / `disk`'s total slot, or as a `TRACKTOTAL` / `DISCTOTAL` key (foobar2000's
convention); without one the number is plain, and a total without its number isn't written. A zero total is no total.
Vorbis `TRACKTOTAL`, `TOTALTRACKS`, `DISCTOTAL`, and `TOTALDISCS` are managed: never extras, removed on write unless
`TRACKTOTAL` / `DISCTOTAL` is being written. The reader strips `/N` from track and disc in every format; the totals and
an MP3's `TPE3` are read apart, as crate-private `read::RawTags` (`read_raw`), for the read-back check and the undo
store.

M4A freeform atoms live under mean `com.apple.iTunes`. Reading prefers the freeform `GENRE` and never doubles values
with `©gen` (which holds them joined with `"; "`).

## Why concrete tag types

lofty's generic `Tag` normalizes keys across formats, which drops the format-native keys the map above depends on
(`TXXX` descriptions, freeform atom names, repeated Vorbis keys). Each format gets its own `Id3v2Tag`, `Ilst`, or
`VorbisComments`, so unmanaged frames, atoms, and keys round-trip untouched.

## How a write works

Each writer parses the original strictly (`ParsingMode::Strict`; Vorbis also without implicit conversions). The lenient
parse the reader uses drops what lofty can't decode (a `covr` atom with one unknown entry, a bad picture block, a second
`VORBIS_COMMENT` block), and saving from it would delete those from the file; strictly, such a file is refused as
`TagError::Parse`. Reading stays lenient. Then the writer removes everything its reader maps to a managed field
(aliases, duplicates, and case variants of TXXX descriptions, freeform names, and Vorbis keys included), removes the
extras that `FileTags::extras` no longer has a key for, then writes one frame, atom, or key per field. An extra the file
has and `extras` keeps stays as the file has it (its values in `extras` are ignored). An extra in `extras` that the file
lacks is added, so an undo can put back one an edit removed: a `TXXX` with that description (values null-separated), a
freeform atom of that name under mean `com.apple.iTunes` (one UTF-8 entry per value; an extra that had another mean
comes back under the iTunes one), or that Vorbis key once per value (Vorbis extra keys are uppercased first, as the
reader reports them; keys differing only in case merge). An extra the file lacks that can't be added (an empty key, no
values, or a key the reader would take for a managed field or a Vorbis total, like a `PERFORMER` that was under a
foreign MP4 mean) fails the write as `TagError::UnwritableExtra(key)`: never a silent skip. An ID3 extra in another
letter case than the file's `TXXX` renames the frame (the old one is dropped, the new one added). Single-value fields
write their first value only (the reader shows only that one), so a second `COMM` or a `TYER` beside `TDRC` goes away.

- **MP3**: text is UTF-8 in memory; lofty writes UTF-16 for v2.3. The date is a `TDRC` timestamp that lofty splits into
  `TYER` (+ `TDAT`, `TIME`); a date it can't parse fails the write, because lofty would drop it silently. lofty's v2.3
  writer discards v2.4-only frames (`TSOP`, `TSOA`, `TSOT`, `TMOO`, `TDRL`, `RVA2`, ...; `write::DROPPED_BY_V23`, from
  lofty 0.25.4's `FRAMES_TO_DISCARD`) and outdated v2.2 IDs, cuts a `TDOR` to its year (`TORY`), and merges `TMCL` into
  `TIPL` (one `IPLS`; a lone `TIPL` round-trips). A write that would lose any of these fails with `TagError::WouldDrop`
  (the IDs) unless `WriteOptions::allow_frame_loss` is set; the app asks the user. The cover is replaced where it
  stands: `FrameList::insert` replaces any frame it considers equal, so it is used only for frames just removed.
- **FLAC**: saving the comments rewrites every picture block from the comments' own pictures, so the file's picture
  blocks move into the comments first (block pictures ahead of in-tag ones, keeping the reader's cover).
- **M4A**: lofty panics on an ilst it can't encode, so a cover type outside JPEG/PNG/BMP/GIF is refused up front. lofty
  moves the `stco` / `co64` chunk offsets when the tag grows and shifts `mdat` (verified in
  `write_test::m4a_chunk_offsets_follow_a_growing_tag`, lofty 0.25.4, 2026-10-26).
- **MP3 behind junk**: a strict parse doesn't search for an ID3v2 tag after junk bytes, so when only the lenient parse
  finds one the write is refused (`TagError::Parse`) instead of adding a second tag in front.
- **FLAC icons**: lofty keeps one picture per icon type when the pictures are gathered; a file with two fails the write
  as `TagError::WouldDrop(["PICTURE:Icon"])`.
- **FLAC**: SEEKTABLE, APPLICATION, and CUESHEET blocks survive a write byte for byte
  (`write_test::flac_unmanaged_blocks_survive`).

After saving the copy, the writer reads it back (same strict parse) and compares it with what it meant to write: the
fields as the reader reports them (joined values split, numbers and ID3 dates normalized), the cover, the extras, and
the `RawTags` (totals as passed, `TPE3` as kept, removed, or restored). Any difference refuses the write, so whatever
lofty silently drops or changes never replaces the original.

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

## Undo store layout

`UndoStore` (`undo.rs`) keeps one directory per apply under a root the app owns, named by the caller's `ApplyId` (one
safe path component: not empty, no `/`, `\`, NUL, or `..`, no leading `.`; anything else is
`TagError::Io(InvalidInput, _)`):

```
<root>/<id>/manifest.json   {"version":1,"seq":N,"entries":[{"path","fields","extras","cover":{"file":"<k>.cover","mime"}|null,"raw":{"trackTotal","discTotal","conductor"}}]}
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
  existing `id` replaces it. Staging directories are neither applies nor counted.
- **`record(id, paths)` reads the files itself**, each with one parse (`read::read_snapshot`): the `FileTags` and the
  `RawTags` (track and disc totals, MP3 `TPE3`) that `FileTags` doesn't hold, so the two can't describe different states
  of the file. Call it before the apply writes. A file that can't be read fails the whole record; nothing is stored.
- **`repath`** takes net moves (recorded path, final path), never the steps of a chain; they apply all at once (a swap
  works), and a move whose source isn't recorded is ignored.
- **`restore`** writes each entry back as it was (`write::restore_tags`): as `ContentKind::Music` whatever the caller
  passes, so a composer comes back, with that entry's recorded totals, and `TPE3` when it had one. Of the caller's
  `WriteOptions` only `allow_frame_loss` applies. A file not at its path is `missing`; any write error is `failed` (an
  extra that can't come back is `TagError::UnwritableExtra`) and the rest continue. `None` in `cover` removes the cover
  again. It never deletes the record; the caller `forget`s it.
- **Limits**: an MP4 extra under a non-iTunes mean comes back under `com.apple.iTunes` (one named like a managed key
  can't come back: `failed`). Vorbis total keys come back under their canonical names (`TOTALTRACKS` as `TRACKTOTAL`).
