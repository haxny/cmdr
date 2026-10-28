# Tag editor details

## Flow

`open(session_id, restart)` reads every supported file (`cmdr_tags::format_of`) of the Multi-Rename session, outside the
store's lock. Without `restart` an existing tag session comes back with its edits; with it, everything is read again.
Every other call runs under the lock on in-memory tags and first drops entries that left the layout.

Guaranteed: the file set is the one at `open` (Multi-Rename's names are fixed per session, a reorder only changes the
order); each file has a `Fingerprint` (size, mtime, inode, ctime) taken before its read, and (in tests)
`unchanged_on_disk` tells whether it still holds (stat'ed outside the lock); after the app's own write `mark_written`
makes the written tags the disk copy and takes a fresh fingerprint; closing or evicting the rename session closes the
tag session (hooks in `multi_rename/session.rs`), and a `NoSession` answer removes a leftover one. Not done: nothing
re-reads or follows a file that something else renamed.

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
  `CoverTooLarge`, `CoverUnreadable`, `CoverNotAnImage`, `CoverNotAFile`) come from `cover.rs`: a name must be one plain
  file name that resolves directly inside the session's folder (no separators, `..`, or symlink out; a symlink to a file
  in the folder reads that file), it must be a regular file (checked before the open, so a FIFO named `x.jpg` can't
  block a pool thread), it is opened at the resolved path with `O_NOFOLLOW | O_NONBLOCK` (a symlink or FIFO swapped in
  after the check fails), and it is at most 10 MiB, enforced on the read too. Its MIME type comes from the magic bytes
  (JPEG `FF D8 FF`, PNG `89 50 4E 47 0D 0A 1A 0A`, WebP `RIFF????WEBP`), never the name: a WebP saved as `.jpg` is
  written as `image/webp`, anything else is `CoverNotAnImage`. `TaskFailed` is a blocking task that didn't finish.
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
- **A drop's read waits as long as it moves.** A dropped file can sit on a NAS: a 3–10 MiB cover over SMB may take
  seconds, and a hung mount blocks for minutes. Decision: `set_from_path_within` reads through
  `cmdr_fs::deadline::blocking_typed_result_until_stalled` with a `ProgressWatch` fed by `read_counted`'s byte count (64
  KiB chunks), and gives up only once the count stood still for `COVER_READ_STALL_LIMIT` (10 s, `SESSION_IO_TIMEOUT`'s
  scale), as the viewer's pulling open does. Why: the fixed 2 s read tier refused a slow but healthy share. The edit is
  made only from the awaited answer, so a read given up on (`CoverTimedOut`, worded as "the drive stopped answering")
  and finishing later changes nothing. Each read holds a permit of `DROP_READS` (two at once) until it ends, given up on
  or not; with none free a drop is refused as `CoverBusy` (worded apart from the stall: earlier reads are still stuck)
  without reading, so repeated drops onto a dead mount never stack blocking threads. Tested: a read moving for 2.4 s
  against a 1 s stall limit is taken; a stalled one is refused and its late end changes nothing; a third drop with two
  stuck is refused unread.
- **A paste** has no file name, so its refusal is `PastedNotAnImage`, worded apart from a file's `CoverNotAnImage`.
  A TIFF (magic `II*\0` / `MM\0*`, at most 40 MiB: an uncompressed clipboard photo is big) is decoded
  (`cover_image.rs`, `contain_panics`, `image::Limits`: 20,000 px a side, 256 MiB) and re-encoded as PNG; when that PNG
  is over 10 MiB, as JPEG at quality 90; still over is `CoverTooLarge`. The conversion runs under a 15 s deadline
  (`set_from_bytes_within`, `PasteTimedOut`), with the same read-then-edit split as a drop: the edit follows only an
  in-time answer.
- **Thumbnails** (`get_cover_thumbnail`): a tile shows a PNG of at most 176 px a side (twice the 88 px tile), made once
  per cover key and byte length and cached process-wide (64 entries, then it starts over), under a 5 s deadline. Only
  the large view (`get_cover_image`) ships the full picture, so per-chapter art never puts N full bitmaps in the
  section.
- **`TagEdit` names a track by path** and is translated to the engine's row-based `Edit`; a path that is not a readable
  track refuses the whole batch with `UnknownTrack` (nothing half-applies).
- **`changes(session_id, kind)`** lists, per changed file, the fields, cover, and extras the edits change, plus what the
  kind removes beyond them (`removals.rs` over `cmdr_tags::removed_by_kind`: an MP3's `TPE3` under Audiobook), read
  from the file after the tag lock, so the review list says everything `apply` with that kind will do. A file that
  can't be read for it is `removesUnknown` (typed `Removals::Unknown`), ❌ never an empty "removes nothing".
- **The view takes a scope too.** `set_view_scope(session_id, paths)` (`set_tag_view_scope`) stores the Tracks pane's
  selection in `TagSession::view_scope`; `view_of` then builds `shared` and its choices over those tracks alone (an
  unreadable one counts in `scope_count` but holds no value) and everything else (tracks, cover, extras, dirty,
  diacritics, bad dates) over every track. Paths the session doesn't have are dropped; a scope with none left, now or
  after a reload, is every track (`scope_count: None`). Not an edit: no history step.
- **Every view is stamped** (`TagEditorView::view_seq`, one counter taken in `view_of` under `SESSIONS`' lock), so
  the UI keeps the newest view by the order the session changed in: answers from the blocking pool land in any order.
  Tests compare views without it (`session_test::content`).
- **Tools take a scope.** `tool(session_id, tool, scope)` runs over the readable tracks at `scope` (paths as
  `TrackRow::path` gives them), in session order, or over every readable track when `scope` is `None`; the rest keep
  their tags. A path the session doesn't have refuses the run (`UnknownTrack`); an unreadable track in the scope is left
  out (it is never written anyway); `Some([])` changes nothing. Each tool sees a `ToolContext` (each file's parent
  folder and stem), so `autoNumber` can start a disc per folder and `fromFileName` reads the names. The context also
  carries the session's listed track count (readable or not) (`session_files`): the numbering tools drop the track
  number only for a one-file BOOK, so a one-track scope of a bigger book is numbered; with a partial scope TrackTotal
  counts the files numbered (as Mp3tag does). A bad file-name pattern is `BadPattern` before anything is touched;
  `preview_names` shows what a pattern reads per file (`fields: None` when the name doesn't match), matching outside the
  lock and changing nothing.
- **Every stored edit is normalized for its format** (`store` → `cmdr_tags::normalize_for`): an M4A holds track and disc
  numbers as integers, so a padded `01` is kept as `1`, and the window shows what the write will leave. A name with no
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

## Invalid dates

`TagEditorView::bad_dates` (`view.rs`, `count_bad_dates`) counts the readable files whose edited Date, first value, is
one the writer refuses. The test is `cmdr_tags::date_is_writable(format, value)`, the same function the write uses, so
the editor flags exactly what Write would refuse (today only an MP3's ID3v2.3 date is checked; other formats always
pass). The Album pane shows the Date row red with a hint when the count is above zero. Writing the raw value stays
refused: the write answers with the typed `BadDate` per file. Why: real files carried a month 20 (`2010-20-09`) and
every write failed with nothing to point at (regression anchor, 2026-10-27).

## Write and undo

`apply(app, session_id, ApplyRequest { kind, allowFrameLoss, skip, allowRemovals })` (`apply.rs`):

1. `pending` copies out the readable files whose edited tags differ from the disk's, in current order, minus `skip`; a
   read-only session refuses (`ReadOnly`).
2. Each file's fingerprint is checked (stat outside every lock); a moved one is `changedOnDisk`, never written.
3. `UndoStore::record(tags-<unix millis>-<session>, paths)` BEFORE any write; a refusal is `TagEditorError::Undo` and
   nothing is written. Millis never repeat within the process, so two applies can't share (and replace) a record.
4. Each file is re-checked right before its write (the TOCTOU window shrinks to one stat), then checked for what the
   kind removes (`removals_of`, read now): a file that would lose a field (or may: an unreadable check, fail closed)
   and isn't in `allowRemovals` (the paths whose removals the change list showed and the user confirmed) is
   `wouldRemove` and not written. This is the guard for every caller: ⌘⏎ sends none, the Write dialog only the files
   it listed with a removal, so a removal is only ever written from a list that showed it. Then `write_tags` with
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
restart. The undo forgets the record when nothing failed (kept for a retry otherwise), re-reads the restored files into
every open tag session that holds them (the undo can come from a later sheet than the one that wrote; each file is read
once, outside the tag lock), and re-reads them in the media cache. It works after the session closed: the record holds
paths. The asking sheet's session (`undo_tag_apply`'s `session_id`) takes the files as read, its unwritten edits to them
dropped: its user confirmed that. Any other session keeps its edits: `session::rebase` moves each field, the cover, and
each extra it never edited onto the restored disk, and its disk copy and fingerprint become the new ones. Several Tags
windows can be open at once, each holding its own Multi-Rename session and so its own tag session over possibly the same
files (held sessions don't count against `MAX_SESSIONS` and are released when their window closes), so "another session"
with unwritten edits is a normal case and `rebase` is what keeps those edits.

The store is one `UndoStore` at `<app data>/tag-undo` behind a `Mutex` held for a whole apply or undo (one writer);
no app data folder is `TagEditorError::NoAppDataDir`. Tests call `apply_with` / `undo_with` with their own store, and
`apply_with_before_write` to change a file between the record and its write.

Undo after "Write anyway" (verified on lofty via `undo_after_write_anyway_restores_the_other_tags`, 2026-10-27): the
snapshot doesn't carry the dropped ID3v2.4-only frame (ID3 extras are `TXXX` items only, `read::id3_items`), so the
restore (`allow_frame_loss: false`) writes every other old tag back cleanly; the dropped frame stays gone.

**Audiobookshelf side files** (`side_files.rs`, `ApplyRequest.sideFiles`, the `multiRename.tagEditor.absSideFiles`
setting, off by default). Audiobookshelf takes the narrator only from Composer or `reader.txt`'s first line, never from
Performer (its `server/utils/prober.js` and `scanner/AudioFileScanner.js` map `tagComposer` to narrators;
`scanner/BookScanner.js` reads `reader.txt` and `desc.txt` from the book folder; read 2026-10-28). The user's canon keeps
the narrator in Performer, so a write also writes `reader.txt` (the tracks' Performer values, distinct, `, `-joined, only
when every track names the same ones, in any order) and `desc.txt` (the Description every track shares) into
`cover::tracks_folder`, from the session's tracks as edited (`session::book_of`). Decision: what the tracks don't agree
on writes nothing, and the list says why (`SideFileChange::Withheld`), rather than guessing a majority. The list is
`get_side_file_changes` (create / replace with the old text / withheld; one that already says the same, first line for
`reader.txt`, is not listed and never touched). Safety: a replacement is written only when `allowSideFiles` names its
path with the old and the new text the list showed (else `sideFilesHeld`), so ⌘⏎ never replaces one, and only when
the apply keeps an undo (`undoUnavailable` holds it: a user's file is never replaced for good); a create is a
same-folder temp file hard-linked to the name, so a file that appeared meanwhile is never replaced; a replacement
re-reads the old bytes right before its rename and keeps the old permissions; a symlink or non-file at the name is
never read, followed, or replaced; 64 KiB cap both ways; only an editable (local) session gets any. Decision: side
files ride along with a tag write, never alone (written only when the apply wrote a track), and say what the files on
disk say right after it (`BookTags::OnDisk`, read after `mark_written`), so a skipped, held, failed, or changed-on-disk
track's edits never reach them; the list (`BookTags::Edited`) shows the intent. Undo: the old bytes (or "none") are kept in memory per
apply id (as many applies as the store keeps); `undo_with` restores them, or removes a created file, only while the file
still holds exactly what was written (else `changed`), and they count in the `RestoreReport` and in
`UndoableApply.written`.

**Non-UTF-8 paths.** Paths cross IPC as lossy strings (`TrackRow::path`). Such a file is listed and written like any
other, but `TagEdit::SetOne` can't name it (`UnknownTrack`, its `SetAll` edits still apply), and `skip` compares the
lossy form, so a skip may cover two files whose names differ only in invalid bytes (the safe direction).

**Known gap.** A media read already in flight when the write lands may store the old facts; the next listing change
(the write moves the file's mtime) queues it again.

## The last write, and quitting

- **The last write per folder** (`last_apply.rs`): `apply` records each report into the folder's last write under the
  undo store (lock order: store, then this leaf). `continues` (a "Write anyway" after a press that wrote) adds to it;
  any other apply that wrote replaces it; one that wrote nothing leaves it. An undo takes its apply out, or keeps it
  with the files it couldn't restore. Kept in memory for the app's lifetime, keyed by `(volume_id, dir)`: a Tags window
  opened later over the same folder, with any selection, offers ⌘Z for it (`last_tag_apply`). It isn't persisted: the
  undo store's records outlive a restart, but which of them was "the last write" doesn't. Undo still refuses a file
  changed since (the sealed fingerprint), whoever asks.
- **Quitting** (`at_quit.rs`): `apply_tags` and `undo_tag_apply` count themselves (`writing()`, inside the blocking
  work) and `unwritten_sessions` counts tag sessions holding edits. The quit gate (`quit/mod.rs`, `holds_quit`) asks
  when either is non-zero, and its teardown waits for a running write up to `TAG_WRITE_WAIT` (10 s) before exiting:
  each file is written whole anyway, but a cut between files leaves a book half written. When only unwritten edits
  hold the quit there is no countdown (`counts_down`): the prompt waits for an answer, as a document app with unsaved
  changes does, and a logout reports that Cmdr interrupted it. File operations or a running write keep the 15 s
  countdown (a write ends on its own). The prompt lives in main, so a held quit raises and focuses main
  (`QuitHost::raise_prompt`), or ⌘Q in a Tags window could put it behind that window.
- **The map is capped** at the store's 50 applies (the folder written longest ago goes first), and an undo that finds
  its record gone (pruned: `Io(NotFound)`) takes the apply out, so ⌘Z isn't offered for a record the store dropped.
- **A second open over the same files** raises the window already showing them: `hold` answers `already_open`
  (`held_twin`: another held session with the same volume, folder, and names in any order).

## Module map (full)

- `session.rs` the store (`SESSIONS`), `open` / `edit` / `tool` / `changes` / `close`; `error.rs` `TagEditorError`.
- `sort.rs` the track order (`TrackSort`, `sort_tracks`): reorders only the Multi-Rename session's names via `set_order`, never a file or an edit; a session opens in disc/track order (file name when no track has a number), once.
- `view.rs` the pure `TagEditorView`: shared rows, track rows, cover, extras, dirty, diacritics.
- `cover.rs` reading a cover (a folder picture by plain name, a dropped file, pasted bytes: regular file, 10 MiB, type
  from the magic bytes, never the name) and the `data:` URL; `cover_choices.rs` every distinct cover with its count,
  the in-memory "use for all", and a drop read under a deadline; `cover_image.rs` decodes (pasted TIFF → PNG/JPEG,
  cached tile thumbnails) under `contain_panics` + `image::Limits`. IPC in `commands/tag_editor.rs`.
- `apply.rs` the write (`apply`, `TagApplyReport`); `side_files.rs` Audiobookshelf's `reader.txt` / `desc.txt` beside
  the book, written and undone with the tags; `undo.rs` the app's `UndoStore` and `undo`; `last_apply.rs` each
  folder's last write (⌘Z), kept for the app's lifetime; `at_quit.rs` what a quit would cut short, for `quit/`;
  `activity.rs` log lines (counts and durations, never paths or values).
- `text_tools.rs` find/replace and format-from-mask over tag fields: in the app, not `cmdr-tags`, because they reuse
  Multi-Rename's engine (`Transform`, `Replace`, `Mask`).
- `window.rs` the Tags window's hold on its Multi-Rename session (`hold` counts tracks, names the folder; `end` on
  close and on the window's `Destroyed`). A held session is never evicted (`multi_rename::session::hold`).
  `close_orphans` (run by `hold_tags_session`, so on every open) destroys, through the child-window hide-and-wait,
  each `tags-*` window whose session is no longer held: one closed but still there would sit hidden for the life of
  the process, holding its web content process. `may_close_unasked` (IPC `tags_window_may_close`) tells the window's
  error boundary whether closing would lose anything: unwritten edits in the session (`session::has_unwritten`) or a
  tag write running (`at_quit::is_writing`). It also
  names what a main-menu key means in a focused Tags window (`menu_selection_of` for ⌘A / ⌘⇧A and ⌘X / ⌘C / ⌘V, `menu_key_command_of`
  for F2–F8, ⇧F2, ⇧F4, ⇧F8; never Tab), which `menu_route.rs` emits there instead of dropping. `native_edit_id_of` names the Edit item whose native action
  `tags_native_edit` hands back for a text box or the cover section.
- `playback.rs` `cmdr-media://` audio tokens per session (dropped on release and in `session::close`, not only at the
  viewer's choke point) and "open in foobar2000" (`com.foobar2000.mac`, else the default app; launch on the main thread,
  never awaited).

## Must-knows (full)

- **Edits are keyed by PATH.** The view walks Multi-Rename's CURRENT names (`multi_rename::session::files_of`) and looks
  each file up, so a reorder never moves an edit. `TagEdit::SetOne` names a track by path; an unknown or unreadable
  path refuses the batch (`UnknownTrack`). A file whose fingerprint moved since the read is never written.
- **Tools run on a scope** (track paths, `None` = all) in session order; an unknown path refuses (`UnknownTrack`).
  `store` normalizes every edit for its format (M4A track `01` → `1`).
- **The no-track rule is about the BOOK**: only a one-track session loses its track number, counting every audio track
  the session lists, readable or not (`ToolContext::session_files`); one selected track of a bigger book is numbered.
- **Only the `root` volume is editable**, as `multi_rename/run.rs` decides locality. Other volumes: `editable: false`,
  `RemoteVolume`; `edit` / `tool` / `apply` refuse with `ReadOnly`. Files are listed only if `std::fs` reaches the
  volume.
- **Every lofty call goes through `contain_panics`**; a panic is a `readError` or `failed` row, never a crash.
  Unreadable rows are never written and count in no summary.
- **A drop's read is stall-watched and the edit follows only its awaited answer** (`set_from_path_within`): a late
  read from a hung mount must never set a cover afterwards. Same split for a pasted TIFF's conversion.
- **❌ No file read under `SESSIONS`' lock**; ❌ never log paths or tag values.
- **Record before write**: `apply` records the old tags first, then seals the record to the files written with their
  post-write `Fingerprint`, so undo touches exactly those, and only while unchanged (never a file renamed onto the
  path). `WouldDrop` is never auto-accepted. Lock order: the store is outermost (`undo.rs`).
- `diacritics` only reports, it never strips; `bad_dates` flags a Date the writer would refuse (`date_is_writable`).
- `sort.rs`: a stored sort stands only while the tracks still are in the order it made (`sorted_tracks`); `reorder` records it after `set_order` succeeds and only if the tracks still match, so a failed or raced sort claims nothing and a Multi-Rename move clears it. `ordered` keeps a reopen from re-sorting a hand-moved order.
