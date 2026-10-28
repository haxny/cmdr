# Tag editor details

## Flow

`open(session_id, restart)` reads every supported file (`cmdr_tags::format_of`) of the Multi-Rename session, outside the
store's lock. Without `restart` an existing tag session comes back with its edits; with it, everything is read again.
Every other call runs under the lock on in-memory tags and first drops entries that left the layout.

Guaranteed: the file set is the one at `open` (Multi-Rename's names are fixed per session, a reorder only changes the
order); each file has a `Fingerprint` (size, mtime, inode, ctime) taken before its read, and (in tests) `unchanged_on_disk` tells
whether it still holds (stat'ed outside the lock); after the app's own write `mark_written` makes the written tags the
disk copy and takes a fresh fingerprint; closing or evicting the rename session closes the tag session (hooks in
`multi_rename/session.rs`), and a `NoSession` answer removes a leftover one. Not done: nothing re-reads or follows a
file that something else renamed.

## Decisions

- **Codes, not text.** `ReadOnlyReason` and `ReadErrorCode` serialize as camelCase strings (`remoteVolume`,
  `unreadable`, `unsupported`, `io`, `parse`); the UI words them. Why: the app's copy changes, the codes don't.
- **`specta` feature on `cmdr-tags`.** The engine's `TagField`, `Edit`, `SharedValue`, `CoverEdit`, `Cover`, `TagTool`,
  `CaseTool`, `ContentKind`, `TagError` (its `io::ErrorKind` as a string), and `RestoreReport` derive `specta::Type`
  behind the feature, which the app enables, so commands name them without mirror types.
- **No orphan tag session.** A rename session that closes while `open` reads runs its close hook before the tag session
  exists. `open` asks `files_of` again after inserting (never under the tag lock: order rename → tag) and drops what it
  inserted if the rename session is gone.
- **Cover size** comes from the image header (`image`), under `contain_panics`; `None` when unreadable.
- A failed read of one file is a row with a `readError`, never a `TagEditorError`. Cover-file errors (`BadCoverName`,
  `CoverTooLarge`, `CoverUnreadable`, `CoverNotAnImage`, `CoverNotAFile`) come from `cover.rs`: a name must be one plain file name that
  resolves directly inside the session's folder (no separators, `..`, or symlink out; a symlink to a file in the folder
  reads that file), it must be a regular file (checked before the open, so a FIFO named `x.jpg` can't block a pool
  thread), it is opened at the resolved path with `O_NOFOLLOW | O_NONBLOCK` (a symlink or FIFO swapped in after the
  check fails), and it is at most 10 MiB, enforced on the read too. Its MIME type comes from the magic bytes (JPEG
  `FF D8 FF`, PNG `89 50 4E 47 0D 0A 1A 0A`, WebP `RIFF????WEBP`), never the name: a WebP saved as `.jpg` is written as
  `image/webp`, anything else is `CoverNotAnImage`. `TaskFailed` is a blocking task that didn't finish.
- **Thumbnail.** `get_cover_image(session, key)` answers a cover as a `data:` URL (base64, typed by the bytes), because
  the app's CSP (`tauri.conf.json` `img-src`) allows `data:` but not `blob:`: the one a `CoverChoice::key` names, or for
  no key the shared one. `CoverSummary::Same::key` is a hash of the bytes, so the panel fetches again when a cover of
  the same type and size replaces another.
- **Every cover of the book** (`cover_choices.rs`): `list_tag_covers` groups the readable files' edited covers by MIME,
  size, and the cached `Loaded::cover_key` (no hashing), most carried first (ties in file order), the files without a
  cover last as `key: "none"`. The covers are copied out under the lock (shared buffers) and their pixel sizes read
  after it. `use_cover_for_all(key)` gives every readable file that cover (`"none"` removes them), a key no file
  carries any more is `UnknownCover`. `set_cover_from_bytes` (a paste) takes at most 10 MiB sniffed by the magic bytes;
  `set_cover_from_path` (a drop) reads only a regular file at an absolute path: `symlink_metadata` refuses a folder,
  FIFO, or symlink (`CoverNotAFile`), the open is `O_NOFOLLOW | O_NONBLOCK`, and the open file is checked again, then
  the same cap and sniff. Each is an in-memory `CoverEdit::Set` / `Remove` through `edit`: listed by `changes`,
  written only by Write, undone like any write. A dropped path can be anywhere: the user dragged it; it only ever
  becomes an image by its bytes.
- **`TagEdit` names a track by path** and is translated to the engine's row-based `Edit`; a path that is not a readable
  track refuses the whole batch with `UnknownTrack` (nothing half-applies).
- **`changes(session_id, kind)`** lists, per changed file, the fields, cover, and extras the edits change, plus what the
  kind removes beyond them (`cmdr_tags::removed_by_kind`: an MP3's `TPE3` under Audiobook), read from the file after the
  tag lock, so the review list says everything `apply` with that kind will do.
- **Tools take a scope.** `tool(session_id, tool, scope)` runs over the readable tracks at `scope` (paths as
  `TrackRow::path` gives them), in session order, or over every readable track when `scope` is `None`; the rest keep
  their tags. A path the session doesn't have refuses the run (`UnknownTrack`); an unreadable track in the scope is left
  out (it is never written anyway); `Some([])` changes nothing. Each tool sees a `ToolContext` (each file's parent folder
  and stem), so `autoNumber` can start a disc per folder and `fromFileName` reads the names. The context also carries the session's readable track count
  (`session_files`): the numbering tools drop the track number only for a one-file BOOK, so a one-track scope of a
  bigger book is numbered; with a partial scope TrackTotal counts the files numbered (as Mp3tag does). A bad file-name pattern is
  `BadPattern` before anything is touched; `preview_names` shows what a pattern reads per file (`fields: None` when the
  name doesn't match), matching outside the lock and changing nothing.
- **Every stored edit is normalized for its format** (`store` → `cmdr_tags::normalize_for`): an M4A holds track and disc
  numbers as integers, so a padded `01` is kept as `1`, and the tab shows what the write will leave. A name with no
  supported extension is stored as given.
- **«Multiple values» rows carry their values.** `SharedRow::choices` is `cmdr_tags::shared::distinct_values` (most
  common first, at most 50) for a `Multiple` row, empty for every other.
- **Audio tokens are per tag session.** `playback::audio_url` mints a `cmdr-media://` token only for a readable track
  of the session (canonical-path match, never a path the frontend made up) and keeps it under the session id;
  `release` drops one, and `session::close` drops all that remain. The viewer's `session::close_session` is not
  the only place a media token is freed (`file_viewer/media.rs`). `open_in_player` opens foobar2000
  (`com.foobar2000.mac`) when installed, else each file in its default app; `UnknownTrack` for a path outside the
  session, `PlayerUnavailable` when nothing could start.
- **Text tools run in the app.** `text_tools.rs` builds on `multi_rename::{mask, transform}` (`Mask`, `Transform`,
  `Replace`), which `cmdr-tags` can't depend on; they validate first, change nothing on a bad pattern, never strip
  diacritics, and take a scope like every other tool.
- **`Cover.data` is `Arc<[u8]>`** so the on-disk and edited tags and every file given one cover share the bytes.
- **Covers are hashed once per read, never per view.** `Loaded::cover_key` is taken in `read_entry` and, for a cover new
  to the session, in `store` (once per shared buffer, however many files get it); `view::build` and `same_cover` compare
  keys and sizes, never bytes. Every edit rebuilds the view, and 22 tracks with a 2 MiB cover each are 44 MiB to hash.
  `big_book_test.rs` counts the hashes and, ignored, times an audiobook (debug build, 2026-10-27: an edit
  ~6.6 ms before, ~0.35 ms after; open ~22 ms before, ~120 ms after, the hashing moved there; the change list ~10 ms).
- **The log**: `activity.rs` writes one `info` line per action under target `tag_editor` (open, edit, tool, text tool,
  cover from file, changes, apply, undo: counts and milliseconds), and the apply's failures by kind at `debug`. No paths
  or values, and errors by kind only.

## Write and undo

`apply(app, session_id, ApplyRequest { kind, allowFrameLoss, skip })` (`apply.rs`):

1. `pending` copies out the readable files whose edited tags differ from the disk's, in current order, minus `skip`; a
   read-only session refuses (`ReadOnly`).
2. Each file's fingerprint is checked (stat outside every lock); a moved one is `changedOnDisk`, never written.
3. `UndoStore::record(tags-<unix millis>-<session>, paths)` BEFORE any write; a refusal is `TagEditorError::Undo` and
   nothing is written. Millis never repeat within the process, so two applies can't share (and replace) a record.
4. Each file is re-checked right before its write (the TOCTOU window shrinks to one stat), then `write_tags` with
   `WriteOptions { kind, track_total: None, disc_total: None, allow_frame_loss }` (the totals travel as the fields
   `TrackTotal` / `DiscTotal`, so an edit keeps them). `WouldDrop` → `wouldDrop`, any other
   error → `failed`, a lofty panic (`contain_panics`) → `failed` with `TagError::Write`.
5. `UndoStore::seal` narrows the record to the files written (`forget` when none), each with its `Fingerprint` taken
   right after its write, so an undo never rewrites a file the apply skipped, failed on, or found changed, nor one that
   changed after the write. If `seal` refuses, the record is forgotten instead and the report has no `applyId` and
   `undoUnavailable: true`: the full record could revert someone else's later change. A written file that can't be
   stat'ed after its write is left out of the record (logged, no path).
6. `mark_written` reads each written file again (outside the lock) and makes that the disk copy and fingerprint, and the
   edited copy too when it still equals what was written (a "Write anyway" may have dropped items). Then
   `multi_rename::media::SessionMedia::reread` for the written names in every session over the folder (`media_over`), so
   `[=tag.*]` previews show the new values.

`undo(app, apply_id)` (`undo.rs`) restores (`allow_frame_loss: false`) only the files still holding the fingerprint the
write left; the others are `RestoreReport::changed` and untouched. Why: the record is keyed by path, and a Multi-Rename
after the write can give a path to another file (renumbering `01…10` after a reorder swaps names), so a path-only
restore would write one chapter's old tags into another. The fingerprint is in the record, so this holds after an app
restart. The undo forgets the record when nothing failed (kept
for a retry otherwise), re-reads the restored files into every open tag session that holds them (the undo can come
from a later sheet than the one that wrote; each file is read once, outside the tag lock), and re-reads them in the
media cache. It works after the session closed: the record holds paths. The asking sheet's session (`undo_tag_apply`'s
`session_id`) takes the files as read, its unwritten edits to them dropped: its user confirmed that. Any other session
keeps its edits: `session::rebase` moves each field, the cover, and each extra it never edited onto the restored disk,
and its disk copy and fingerprint become the new ones. Today only one Multi-Rename sheet is open at a time (one main
window, and `openMultiRename` returns while a sheet is up), so "another session" is a defensive case.
(Verified 2026-10-27 by reading `routes/(main)/+page.svelte` `openMultiRename`.)

The store is one `UndoStore` at `<app data>/tag-undo` behind a `Mutex` held for a whole apply or undo (one writer);
no app data folder is `TagEditorError::NoAppDataDir`. Tests call `apply_with` / `undo_with` with their own store, and
`apply_with_before_write` to change a file between the record and its write.

Undo after "Write anyway" (verified on lofty via `undo_after_write_anyway_restores_the_other_tags`, 2026-10-27): the
snapshot doesn't carry the dropped ID3v2.4-only frame (ID3 extras are `TXXX` items only, `read::id3_items`), so the
restore (`allow_frame_loss: false`) writes every other old tag back cleanly; the dropped frame stays gone.

**Non-UTF-8 paths.** Paths cross IPC as lossy strings (`TrackRow::path`). Such a file is listed and written like any
other, but `TagEdit::SetOne` can't name it (`UnknownTrack`, its `SetAll` edits still apply), and `skip` compares the
lossy form, so a skip may cover two files whose names differ only in invalid bytes (the safe direction).

**Known gap.** A media read already in flight when the write lands may store the old facts; the next listing change
(the write moves the file's mtime) queues it again.
