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
- A failed read of one file is a row with a `readError`, never a `TagEditorError`. Cover-file errors (`BadCoverName`, `CoverTooLarge`, `CoverUnreadable`) come from `cover.rs`: a name must be one plain file name that resolves directly inside the session's folder (no separators, `..`, or symlink out), at most 10 MiB; `TaskFailed` is a blocking task that didn't finish.
- **`TagEdit` names a track by path** and is translated to the engine's row-based `Edit`; a path that is not a readable
  track refuses the whole batch with `UnknownTrack` (nothing half-applies).
- **`Cover.data` is `Arc<[u8]>`** so the on-disk and edited tags and every file given one cover share the bytes.

## Write and undo

`apply(app, session_id, ApplyRequest { kind, allowFrameLoss, skip })` (`apply.rs`):

1. `pending` copies out the readable files whose edited tags differ from the disk's, in current order, minus `skip`; a
   read-only session refuses (`ReadOnly`).
2. Each file's fingerprint is checked (stat outside every lock); a moved one is `changedOnDisk`, never written.
3. `UndoStore::record(tags-<unix millis>-<session>, paths)` BEFORE any write; a refusal is `TagEditorError::Undo` and
   nothing is written. Millis never repeat within the process, so two applies can't share (and replace) a record.
4. Each file is re-checked right before its write (the TOCTOU window shrinks to one stat), then `write_tags` with
   `WriteOptions { kind, track_total: None, disc_total: None, allow_frame_loss }`. `WouldDrop` → `wouldDrop`, any other
   error → `failed`, a lofty panic (`contain_panics`) → `failed` with `TagError::Write`.
5. `UndoStore::retain` narrows the record to the files written (`forget` when none), so an undo never rewrites a file
   the apply skipped, failed on, or found changed. If `retain` refuses, the record is forgotten instead and the report
   has no `applyId` and `undoUnavailable: true`: the full record could revert someone else's later change.
6. `mark_written` reads each written file again (outside the lock) and makes that the disk copy and fingerprint, and the
   edited copy too when it still equals what was written (a "Write anyway" may have dropped items). Then
   `multi_rename::media::SessionMedia::reread` for the written names in every session over the folder (`media_over`), so
   `[=tag.*]` previews show the new values.

`undo(app, apply_id)` (`undo.rs`) restores (`allow_frame_loss: false`), forgets the record when nothing failed (kept
for a retry otherwise), re-reads the restored files into the tag session named in the id if it's still open, and
re-reads them in the media cache. It works after the session closed: the record holds paths.

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
