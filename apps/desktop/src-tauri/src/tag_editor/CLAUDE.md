# Tag editor (the Tags window)

A tag session per Multi-Rename session: the supported audio files' tags, as on disk and as edited, in memory. Opt-in
(`multiRename.tagEditor`); the engine is `crates/cmdr-tags`.

## Module map

- `session.rs` the store (`SESSIONS`), `open` / `edit` / `tool` / `changes` / `close`; `error.rs` `TagEditorError`.
- `view.rs` the pure `TagEditorView`: shared rows, track rows, cover, extras, dirty, diacritics.
- `cover.rs` reading a cover (a folder picture by plain name, a dropped file, pasted bytes: regular file, 10 MiB, type
  from the magic bytes, never the name) and the `data:` URL; `cover_choices.rs` every distinct cover with its count,
  the in-memory "use for all", and a drop read under a deadline; `cover_image.rs` decodes (pasted TIFF → PNG/JPEG,
  cached tile thumbnails) under `contain_panics` + `image::Limits`. IPC in `commands/tag_editor.rs`.
- `apply.rs` the write (`apply`, `TagApplyReport`); `undo.rs` the app's `UndoStore` and `undo`; `activity.rs` the
  `tag_editor` log lines (counts and durations, never paths or values).
- `text_tools.rs` find/replace and format-from-mask over tag fields: in the app, not `cmdr-tags`, because they reuse
  Multi-Rename's engine (`Transform`, `Replace`, `Mask`).
- `window.rs` the Tags window's hold on its Multi-Rename session (`hold` counts tracks, names the folder; `end` on
  close and on the window's `Destroyed`). A held session is never evicted (`multi_rename::session::hold`). It also
  names what a main-menu key means in a focused Tags window (`menu_selection_of` for ⌘A / ⌘⇧A, `menu_key_command_of`
  for F2–F8, ⇧F2, ⇧F4, ⇧F8; never Tab), which `menu/menu_handlers.rs` emits there instead of dropping.
- `playback.rs` `cmdr-media://` audio tokens per session (dropped on release and in `session::close`, not only at the
  viewer's choke point) and "open in foobar2000" (`com.foobar2000.mac`, else the default app; launch on the main thread,
  never awaited).

## Must-knows

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

Details: `DETAILS.md`.
