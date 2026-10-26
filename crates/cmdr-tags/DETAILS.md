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

Audiobooks also lose `TPE3` (conductor) on MP3. A known total (`WriteOptions::track_total`) goes in `TRCK` as `N/Total`,
in `trkn`'s total slot, or as a `TRACKTOTAL` key (foobar2000's convention); without one the track is plain. Disc is
always plain. Vorbis `TRACKTOTAL`, `TOTALTRACKS`, `DISCTOTAL`, and `TOTALDISCS` are managed: never extras, removed on
write unless `TRACKTOTAL` is being written. The reader strips `/N` from track and disc in every format.

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
extras that `FileTags::extras` no longer has a key for, then writes one frame, atom, or key per field. Extras are never
written: one still in `extras` stays as the file has it. Single-value fields write their first value only (the reader
shows only that one), so a second `COMM` or a `TYER` beside `TDRC` goes away.

- **MP3**: text is UTF-8 in memory; lofty writes UTF-16 for v2.3. The date is a `TDRC` timestamp that lofty splits into
  `TYER` (+ `TDAT`, `TIME`); a date it can't parse fails the write, because lofty would drop it silently. lofty's v2.3
  writer discards v2.4-only frames (`TSOP`, `TSOA`, `TSOT`, `TMOO`, `TDRL`, `RVA2`, ...; `write::DROPPED_BY_V23`, from
  lofty 0.25.4's `FRAMES_TO_DISCARD`) and outdated v2.2 IDs, so a write that would lose any fails with
  `TagError::WouldDrop` (the IDs) unless `WriteOptions::allow_frame_loss` is set; the app asks the user. The cover is
  replaced where it stands: `FrameList::insert` replaces any frame it considers equal, so it is used only for frames
  just removed.
- **FLAC**: saving the comments rewrites every picture block from the comments' own pictures, so the file's picture
  blocks move into the comments first (block pictures ahead of in-tag ones, keeping the reader's cover).
- **M4A**: lofty panics on an ilst it can't encode, so a cover type outside JPEG/PNG/BMP/GIF is refused up front. lofty
  moves the `stco` / `co64` chunk offsets when the tag grows and shifts `mdat` (verified in
  `write_test::m4a_chunk_offsets_follow_a_growing_tag`, lofty 0.25.4, 2026-10-26).
- **FLAC**: SEEKTABLE, APPLICATION, and CUESHEET blocks survive a write byte for byte
  (`write_test::flac_unmanaged_blocks_survive`).

After saving the copy, the writer reads it back (same strict parse) and compares it with what it meant to write: the
fields as the reader reports them (joined values split, numbers and ID3 dates normalized), the cover, and the extras
kept. Any difference refuses the write, so whatever lofty silently drops or changes never replaces the original.

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

Defined with the undo task; this section is filled in when it lands.
