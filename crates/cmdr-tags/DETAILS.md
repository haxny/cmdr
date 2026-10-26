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

Audiobooks also lose `TPE3` (conductor) on MP3. Track is `N/Total` in every format (`trkn`'s total slot on M4A) only
when `WriteOptions::track_total` is set; disc is always plain.

M4A freeform atoms live under mean `com.apple.iTunes`. Reading prefers the freeform `GENRE` and never doubles values
with `©gen` (which holds them joined with `"; "`).

## Why concrete tag types

lofty's generic `Tag` normalizes keys across formats, which drops the format-native keys the map above depends on
(`TXXX` descriptions, freeform atom names, repeated Vorbis keys). Each format gets its own `Id3v2Tag`, `Ilst`, or
`VorbisComments`, so unmanaged frames, atoms, and keys round-trip untouched.

## How a write works

Each writer reads the concrete tag from the original, removes everything its reader maps to a managed field (aliases,
duplicates, and case variants of TXXX descriptions, freeform names, and Vorbis keys included), removes the extras that
`FileTags::extras` no longer has a key for, then writes one frame, atom, or key per field. Extras are never written: one
still in `extras` stays as the file has it. Single-value fields write their first value only (the reader shows only that
one), so a second `COMM` or a `TYER` beside `TDRC` goes away.

- **MP3**: text is UTF-8 in memory; lofty writes UTF-16 for v2.3. The date is a `TDRC` timestamp that lofty splits into
  `TYER` (+ `TDAT`, `TIME`); a date it can't parse fails the write, because lofty would drop it silently. Writing v2.3
  drops v2.4-only frames (`TSOP`, `TSOA`, `TSOT`, `TMOO`, `TDRL`, ...): lofty's v2.3 writer has no mapping for them.
- **FLAC**: saving the comments rewrites every picture block from the comments' own pictures, so the file's picture
  blocks move into the comments first (block pictures ahead of in-tag ones, keeping the reader's cover).
- **M4A**: lofty panics on an ilst it can't encode, so a cover type outside JPEG/PNG/BMP/GIF is refused up front.

## Safe overwrite

`save_safely` resolves symlinks (the link stays a link), refuses a read-only file, and creates a temp file in the
target's directory (same volume, so the rename is atomic). It copies the original there with `fs::copy` (which keeps
permissions and, on macOS via `fcopyfile`, extended attributes such as Finder tags), saves the tags into the copy,
parses the copy back, fsyncs, renames it over the original, and fsyncs the directory. A failure anywhere leaves the
original untouched and the temp file removed; a crash leaves the original or the finished file, never half of one. A
hard link to the file keeps the old content (the rename makes a new inode).

The parse-back exists because lofty 0.25.4's `replace_range` truncates to `old_len - new_len` instead of
`start + new_len` when the edited range ends at EOF and shrinks: a FLAC whose metadata runs to the end of the file came
out cut mid-STREAMINFO (verified in `write_test::corrupting_save_is_refused`, 2026-10-26).

## Undo store layout

Defined with the undo task; this section is filled in when it lands.
