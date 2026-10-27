# Tags tab of Multi-Rename (fork, opt-in)

The Tags tab lives in `tag-editor/` (its own pieces, backend: `src-tauri/src/tag_editor/CLAUDE.md`), behind the
`multiRename.tagEditor` setting, **off by default**: off, the sheet is upstream's (no tab strip, no extra IPC).
`MultiRenameDialog.svelte` carries one import (`tag-editor/index.ts`). Only `RenamePanel` / `SheetTabs` / `TagsTab`, the
discard guard and the state factory cross that line.

- **⌘1 / ⌘2** pick Rename / Tags: registry commands `multiRename.tabRename` / `tabTags`, an allowed conflict with
  `view.fullMode` / `view.briefMode` (`shortcuts/allowed-conflicts.test-helper.ts`).
- **Unwritten edits are never lost silently**: closing, Rename, and switching sessions go through `discard-guard.ts`
  ("Discard tag changes?").
- **The tag canon is shown, not enforced**: diacritics are allowed only in Comment and Description (a warning with a
  one-click fix, never a silent strip); a single-file book hides the # column and Renumber; the write kind defaults to
  Audiobook. Details: `tag-editor/` header comments and the backend `DETAILS.md`.
- **Writing is safe**: the undo record is saved first, a file changed on disk since the read is listed as
  `changedOnDisk` and not written, "Write anyway" (data loss) is an explicit choice, and Undo asks first and leaves a
  file that changed after the write (another write, a rename onto its name) alone, counted apart.

Details: `DETAILS.md`; backend: `src-tauri/src/tag_editor/CLAUDE.md`.
