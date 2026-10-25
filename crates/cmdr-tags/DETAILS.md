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
| Composer    | `TCOM` (removed for audiobooks)    | not written                                     | `COMPOSER`        |                   |
| Date        | `TYER` / `TDRC` (lofty downgrades) | `©day`                                          | `DATE`            |                   |
| Genre       | `TCON`                             | freeform `GENRE` (one entry per value) + `©gen` | `GENRE`, repeated |                   |
| Publisher   | `TPUB`                             | freeform `publisher`                            | `PUBLISHER`       |                   |
| Translator  | `TXXX:Translator`                  | freeform `translator`                           | `TRANSLATOR`      |                   |
| Track       | `TRCK`                             | `trkn`                                          | `TRACKNUMBER`     |                   |
| Disc        | `TPOS`                             | `disk`                                          | `DISCNUMBER`      | `TXXX:DiscNumber` |
| Comment     | `COMM` (lang `eng`, empty desc.)   | `©cmt`                                          | `COMMENT`         | `TXXX:Comment`    |
| Description | `TXXX:Description`                 | freeform `description`                          | `DESCRIPTION`     |                   |
| Www         | `TXXX:www`                         | freeform `www`                                  | `WWW`             |                   |

Cover: `APIC` type 3 (MP3), `covr` (M4A), `METADATA_BLOCK_PICTURE` or a FLAC picture block. Front cover only.

M4A freeform atoms live under mean `com.apple.iTunes`. Reading prefers the freeform `GENRE` and never doubles values
with `©gen` (which holds them joined with `"; "`).

## Why concrete tag types

lofty's generic `Tag` normalizes keys across formats, which drops the format-native keys the map above depends on
(`TXXX` descriptions, freeform atom names, repeated Vorbis keys). Each format gets its own `Id3v2Tag`, `Ilst`, or
`VorbisComments`, so unmanaged frames, atoms, and keys round-trip untouched.

## Safe overwrite

A write copies the file to a temp file in the same directory (same volume, so the rename is atomic), writes the tags
there, fsyncs, and renames over the original. A crash leaves the original or the finished file, never half of one.

## Undo store layout

Defined with the undo task; this section is filled in when it lands.
