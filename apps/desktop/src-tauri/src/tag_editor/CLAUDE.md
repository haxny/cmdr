# Tag editor (Tags tab of Multi-Rename)

A tag session per Multi-Rename session: the supported audio files' tags, as on disk and as edited, in memory. Opt-in
(`multiRename.tagEditor`); the engine is `crates/cmdr-tags`.

## Module map

- `session.rs` the store (`SESSIONS`), `open` / `edit` / `tool` / `changes` / `close`, and `TagEditorError`.
- `view.rs` the pure `TagEditorView`: shared rows, track rows, cover, extras, dirty, diacritics.

## Must-knows

- **Edits are keyed by PATH.** The view walks Multi-Rename's CURRENT names (`multi_rename::session::files_of`) and looks
  each file up, so a reorder never moves an edit. `Edit::SetOne.row` is a row of `tracks` as shown, unreadable rows
  included; `translate` maps it to the readable index and drops edits on unreadable rows.
- **Tools get the WHOLE session's readable tags**, never a sub-selection (`renumber` removes the track of a one-file
  session).
- **Only the `root` volume is editable**, as `multi_rename/run.rs` decides locality. Other volumes: `editable: false`,
  `RemoteVolume`; `edit` / `tool` refuse with `ReadOnly`. Files are listed only if `std::fs` reaches the volume.
- **Every lofty call goes through `contain_panics`**; a panic is a row with `readError`, never a crash. Unreadable rows
  are never written and count in no summary.
- **❌ No file read under `SESSIONS`' lock**; ❌ never log paths or tag values.
- Edits are memory only. Nothing here writes a file; `diacritics` only reports, it never strips.

Details: `DETAILS.md`.
