# Tags tab of Multi-Rename (fork, opt-in)

The Tags tab lives in `tag-editor/` (its own pieces, backend: `src-tauri/src/tag_editor/CLAUDE.md`), behind the
`multiRename.tagEditor` setting, **off by default**: off, the sheet is upstream's (no tab strip, no extra IPC).
`MultiRenameDialog.svelte` carries one import (`tag-editor/index.ts`). Only `RenamePanel` / `SheetTabs` / `TagsTab`, the
discard guard and the state factory cross that line.

- **⌘⇧[ / ⌘⇧]** pick Rename / Tags: registry commands `multiRename.tabRename` / `tabTags`. ❌ Never a combo another
  command holds in an overlapping scope: the Keyboard shortcuts settings would show a conflict to every user, setting
  off too (⌘1/⌘2 hit `view.*Mode`, ⌃Tab `tab.next`).
- **⌥Space** is `multiRename.playTrack`: plays or stops the track whose row holds focus (a cell or the checkbox). Plain
  Space types a space in a cell, so the key is ⌥Space; ⌘⌥P is `multiRename.normalizeUnicode`'s. It acts only inside the
  track list, and claims the key when it acts (macOS types a non-breaking space for ⌥Space, and `key-capture.ts` reads
  that NBSP as `Space`). Playback stops on unmount (the Rename tab shows, or the sheet closes), on another play, on
  `ended`; each stop releases the URL.
- **Scope**: the tag tools work on the checked tracks (`all` / `some` / `gone`, `track-selection.svelte.ts`) through
  `TagsTab`'s `runScoped`; a vanished selection never widens to every track. "Open in foobar2000" follows the same
  scope.
- **Totals are fields** (TrackTotal, DiscTotal), and `store` normalizes every edit for its format (an M4A's `01` shows
  as `1`). The text tools (find/replace, format, from file name) are driven from this folder but run in the app's Rust
  (`tag_editor/text_tools.rs`) because they reuse Multi-Rename's own engine, not `cmdr-tags`.
- **Unwritten edits are never lost silently**: closing the sheet (Cancel, Esc), Rename, and Undo rename go through
  `discard-guard.ts` ("Discard tag changes?"). One sheet is open at a time, so there is no session to switch to.
- **The tag canon is shown, not enforced**: diacritics are allowed only in Comment and Description (a warning with a
  one-click fix, never a silent strip); a single-file book hides the # column and Renumber; the write kind is Audiobook
  on first run, then the user's last pick (`write-kind.ts`), and the change list names what it removes (an MP3's
  conductor). Details: `tag-editor/` header comments and the backend `DETAILS.md`.
- **Writing is safe**: the undo record is saved first, a file changed on disk since the read is listed as
  `changedOnDisk` and not written, "Write anyway" (data loss) is an explicit choice, and Undo asks first and leaves a
  file that changed after the write (another write, a rename onto its name) alone, counted apart.

Details: `DETAILS.md`; backend: `src-tauri/src/tag_editor/CLAUDE.md`.
