# cmdr-media-meta details

The design card is GitHub issue #657. This crate is its engine half: reading. The app's Multi-Rename owns the cache, the
loader, the grammar, and every word a user sees (`apps/desktop/src-tauri/src/multi_rename/DETAILS.md`).

## Why its own crate

Parsing EXIF, tags, and two container formats is self-contained and testable without the app. As a `crates/` member it
compiles and tests alone, `index-crate-isolation` keeps `tauri` out of it, and the app can't reach into its parsers.

## The fields

- `exif`: `taken` (Date), `camera`, `make`, `model`, `lens` (Text), `iso`, `focal` (whole mm), `width`, `height`,
  `orientation` (Number).
- `tag`: `title`, `artist`, `albumartist`, `album`, `genre`, `comment` (Text), `track`, `disc`, `year` (Number).
- `FieldKind` decides the formats a placeholder takes: dates the mask's date grammar, numbers a digit count, text the
  range grammar.
- `MediaField::parse` takes the exact name only (case matters, as in every placeholder).
- `exif.taken` covers videos too: a phone's videos sit beside its photos.

## Which files are media

`is_media_name` goes by extension, in any case; a leading dot belongs to the name, as in the mask.

- Photos: jpg, jpeg, heic, heif, tif, tiff, png, webp, dng, cr2, cr3, nef, arw, orf, rw2, raf.
- Audio: mp3, m4a, m4b, flac, ogg, oga, opus, wav, aif, aiff, ape, wv, mpc, aac.
- Video: mp4, mov, m4v, 3gp, mkv, webm, mka.

**Decision**: CR3 and RAF are in the list though `kamadak-exif` reads neither (ISOBMFF and Fuji containers). **Why**:
leaving them out would make such a photo `NotMedia`, which the preview words as "no date taken", a quiet lie. In the
list, `read_media` answers `NotSupported` without fetching a byte, and the row says it can't read the file.

## Errors through `Read`

Parsers (`exif`, `lofty`) see only `Read + Seek`, so the block reader's typed refusals travel as `io::Error`:
`ReadError::into_io` wraps one in a private payload type, and `ReadError::from_io` downcasts it back. Any other
`io::Error` becomes `Io(text)`, whose text is for the log only. The round trip is typed, ❌ never a message match.

## The byte source

- `BlockReader` fetches 64 KiB blocks on demand and keeps the eight most recently used. A header walk is one or two
  round trips on a NAS, and a parser's back-and-forth between a header and a few boxes stays in the kept blocks.
- **Budget**: every fetched byte counts against `MAX_FETCHED_BYTES_PER_FILE` (4 MiB, 64 blocks); a fetch that would pass
  it is `TooLarge`. A kept block costs nothing to read again.
- **Holes** (`with_holes`, any order, overlaps merged) read as zeros and are never fetched: a block wholly inside one is
  never asked for, and a block that straddles one serves real bytes only up to the hole's start. Cover art is a hole, so
  an MP3 with a 3 MB cover costs its header blocks, not 3 MB.
- **Short answers** are asked again for the rest; an empty answer is the end of the file. A file that shrank since the
  listing ends where its bytes do, without an error.
- `VolumeByteSource::read_at` checks the cancel flag and the deadline before each round trip, so a closed sheet or a
  spent budget stops at the next block. The `read_range` runs detached under what's left of the deadline
  (`timeout_detached_within`): on a timeout the transfer finishes on its own, which keeps an MTP transaction whole.
  `VolumeError::NotSupported` maps to `ReadError::NotSupported`; any other volume error is logged there (marked, at
  debug) and becomes a pathless `Io`.
- Tests: `source_test.rs`, over a recording in-memory source, `InMemoryVolume`, and `cmdr_fs::testing::WedgedVolume`
  (never answers) for the timeout.

## Dependencies

- `kamadak-exif` 0.6.1 (as `exif`), the version the app already pins.
- `lofty` 0.25.4 (default features), MIT OR Apache-2.0, with `lofty_attr` 0.13.0 and `ogg_pager` 0.7.2 new to the graph;
  `cargo deny check` passes (verified 2026-10-22, crates.io and `cargo deny`; repo not archived, last push 2026-10-17).
- `serde` + `specta` (pinned `=2.0.0-rc.24`, as `cmdr-fs`): `MediaField` crosses IPC inside the app's row statuses.
- `tokio` (`rt`) for `Handle::block_on` in the byte source; `cmdr-fs` for `Volume`, `VolumeError`, and `Deadline`.

## Overlap with the agent's EXIF reader

`apps/desktop/src-tauri/src/agent/tools/read/inspect/exif.rs` also reads EXIF, but shapes model-facing strings
(`1/250 s`); this crate answers typed values. They stay separate for now.
