# Tag editor details

## Flow

`open(session_id, restart)` reads every supported file (`cmdr_tags::format_of`) of the Multi-Rename session, outside the
store's lock. Without `restart` an existing tag session comes back with its edits; with it, everything is read again.
Every other call runs under the lock on in-memory tags and first drops entries that left the layout.

Guaranteed: the file set is the one at `open` (Multi-Rename's names are fixed per session, a reorder only changes the
order); each file has a `Fingerprint` (size, mtime, inode) taken before its read, and `unchanged_on_disk` tells a writer
whether it still holds; closing or evicting the rename session closes the tag session (hooks in
`multi_rename/session.rs`), and a `NoSession` answer removes a leftover one. Not done: nothing re-reads or follows a
file that something else renamed.

## Decisions

- **Codes, not text.** `ReadOnlyReason` and `ReadErrorCode` serialize as camelCase strings (`remoteVolume`,
  `unreadable`, `unsupported`, `io`, `parse`); the UI words them. Why: the app's copy changes, the codes don't.
- **`specta` feature on `cmdr-tags`.** The engine's `TagField`, `Edit`, `SharedValue`, `CoverEdit`, `Cover`, `TagTool`,
  and `CaseTool` derive `specta::Type` behind the feature, which the app enables, so commands name them without mirror
  types.
- **Cover size** comes from the image header (`image`), under `contain_panics`; `None` when unreadable.
- **`TagEditorError::Read`** is reserved for the commands; a failed read of one file is a row, not an error.
- **`TagEdit` names a track by path** and is translated to the engine's row-based `Edit`; a path that is not a readable
  track refuses the whole batch with `UnknownTrack` (nothing half-applies).
- **`Cover.data` is `Arc<[u8]>`** so the on-disk and edited tags and every file given one cover share the bytes.
