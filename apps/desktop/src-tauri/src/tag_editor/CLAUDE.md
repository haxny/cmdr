# Tag editor (Tags tab of Multi-Rename)

A tag session per Multi-Rename session: the supported audio files' tags, as on disk and as edited, in memory. Opt-in
(`multiRename.tagEditor`); the engine is `crates/cmdr-tags`.

## Module map

- `session.rs` the store (`SESSIONS`), `open` / `edit` / `tool` / `changes` / `close`, and `TagEditorError`.
- `view.rs` the pure `TagEditorView`: shared rows, track rows, cover, extras, dirty, diacritics.
- `apply.rs` the write (`apply`, `TagApplyReport`); `undo.rs` the app's `UndoStore` and `undo`.

## Must-knows

- **Edits are keyed by PATH.** The view walks Multi-Rename's CURRENT names (`multi_rename::session::files_of`) and looks
  each file up, so a reorder never moves an edit. `TagEdit::SetOne` names a track by path; an unknown or unreadable
  path refuses the batch (`UnknownTrack`). A file whose fingerprint moved since the read is never written.
- **Tools get the WHOLE session's readable tags**, never a sub-selection (`renumber` removes the track of a one-file
  session).
- **Only the `root` volume is editable**, as `multi_rename/run.rs` decides locality. Other volumes: `editable: false`,
  `RemoteVolume`; `edit` / `tool` / `apply` refuse with `ReadOnly`. Files are listed only if `std::fs` reaches the
  volume.
- **Every lofty call goes through `contain_panics`**; a panic is a `readError` or `failed` row, never a crash.
  Unreadable rows are never written and count in no summary.
- **❌ No file read under `SESSIONS`' lock**; ❌ never log paths or tag values.
- **Record before write**: `apply` records the old tags first, then narrows the record to the files written, so undo
  touches exactly those. `WouldDrop` is never auto-accepted. Lock order: undo store → rename → tag.
- `diacritics` only reports, it never strips.

Details: `DETAILS.md`.
