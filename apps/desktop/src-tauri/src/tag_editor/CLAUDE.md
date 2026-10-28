# Tag editor (the Tags window)

A tag session per Multi-Rename session: the supported audio files' tags, as on disk and as edited, in memory. Opt-in
(`multiRename.tagEditor`); the engine is `crates/cmdr-tags`.

## Module map

- `session.rs` the store (`SESSIONS`), `open` / `edit` / `tool` / `changes` / `close`; `error.rs` `TagEditorError`.
- `sort.rs` the track order (`TrackSort`): reorders only the rename session's names via `set_order`, never a file or an
  edit; opens in disc/track order (file name when no track has a number), once.
- `view.rs` the pure `TagEditorView`; `cover*.rs` covers; `apply.rs`, `undo.rs`, `last_apply.rs`, `at_quit.rs` the
  write, ⌘Z and the quit gate; `text_tools.rs` find/replace and format-from-mask; `window.rs` the window's hold on its
  session and menu-key routing; `playback.rs` audio tokens and foobar2000; `activity.rs` log lines.
  IPC is in `commands/tag_editor.rs`. Each file's story is in `DETAILS.md`.

## Must-knows

- **Edits are keyed by PATH.** The view walks the rename session's CURRENT names, so a reorder never moves an edit. An
  unknown or unreadable path refuses the batch (`UnknownTrack`). A file whose fingerprint moved since the read is never
  written.
- **Tools run on a scope** (selected tracks, else all), in session order; an unknown path refuses.
- **The no-track rule is about the BOOK**: only a one-track book loses its track number, counting all its audio tracks.
- **Only the `root` volume is editable**; other volumes are `editable: false` and `edit` / `tool` / `apply` refuse with
  `ReadOnly`.
- **Every lofty call goes through `contain_panics`**; a panic is a `readError` or `failed` row, never a crash.
  Unreadable rows are never written.
- **A drop's read is stall-watched** and the edit follows only its awaited answer: a late read from a hung mount must
  never set a cover afterwards.
- **❌ No file read under `SESSIONS`' lock**; ❌ never log paths or tag values.
- **Record before write**: `apply` records the old tags first, then seals the record with post-write fingerprints, so
  undo touches exactly those files, and only while unchanged. `WouldDrop` is never auto-accepted. The store is the
  outermost lock (`undo.rs`).
- **The stored sort is only true while the order is**: a Multi-Rename move clears it (see `DETAILS.md`).

Details: `DETAILS.md` (full module map and every must-know in long form).
