# Tag editor details

## Flow

`open(session_id)` reads every supported file (`cmdr_tags::format_of`) of the Multi-Rename session, outside the store's
lock, and replaces the tag session. Every other call first reads the current tracks the store doesn't know yet (a name
the session gained), then runs under the lock on in-memory tags.

## Decisions

- **Codes, not text.** `ReadOnlyReason` and `ReadErrorCode` serialize as camelCase strings (`remoteVolume`,
  `unreadable`, `unsupported`, `io`, `parse`); the UI words them. Why: the app's copy changes, the codes don't.
- **`specta` feature on `cmdr-tags`.** The engine's `TagField`, `Edit`, `SharedValue`, `CoverEdit`, `Cover`, `TagTool`,
  and `CaseTool` derive `specta::Type` behind the feature, which the app enables, so commands name them without mirror
  types.
- **Cover size** comes from the image header (`image`), under `contain_panics`; `None` when unreadable.
- **`TagEditorError::Read`** is reserved for the commands; a failed read of one file is a row, not an error.
- A file a Multi-Rename apply renamed is a new path: its tags are read fresh and edits under the old name stay with it.
