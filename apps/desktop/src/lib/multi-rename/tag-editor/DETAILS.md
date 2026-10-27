# Tags tab details

Setting `multiRename.tagEditor` (boolean, default off). On, `SheetTabs` appears (Rename | Tags); the Rename tab is
`RenamePanel`, the Tags tab `TagsTab`. Files in `tag-editor/`:

- **State**: `tag-editor-state.svelte.ts` (the view from the backend's `TagEditorView`, edits sent by path, `busy`,
  `settled()` so a cell that just lost focus is in before a close/apply, Undo); `last-tag-apply.svelte.ts` (the last
  write's applies, which "Undo tag changes" reverses; outlives the sheet, not persisted); `discard-guard.ts`.
- **Parts**: `SharedFieldsTable` (one row per field; a "multiple values" row is untouched until changed), `TrackList`
  (per-file Title and #; the # column is hidden for a one-file book), `CoverPanel` (+ thumbnail), `EditableCell`,
  `TagToolsBar` (clean-up tools over the whole session, Case on the focused field), `ApplyPanel` + `ApplyOutcome` (what
  will change, the per-file result, Undo), `WouldDropDialog` (items a format would lose; Write anyway or Skip).
- **Why edits are keyed by path**: the user may reorder rows in the Rename tab; every edit stays with its file. Each
  time the Tags tab shows, `open()` fetches the view again (`openTagEditor(id)`, never a restart), so the list follows
  the new order; the unwritten edits live in the backend session and come back with it. A view answer older than one
  already shown is dropped.
- **`settled()` is bounded** (`SETTLE_LIMIT_MS`, 5 s): a call that never answers (a stalled network mount) can't hold
  the sheet. It answers `false` then, and the discard guard asks "Discard tag changes?" anyway, so Cancel and Esc always
  end in a question or a close.
- **Apply / undo**: the old tags are recorded before any write; failures are per file and the rest are written; Undo
  restores exactly the written files after asking, and leaves one that changed since (its fingerprint moved) alone.
  Other open sessions keep their unwritten edits, rebased per field. One Multi-Rename sheet is open at a time.
- **Tag canon**: no diacritics except Comment and Description; ContentKind is Audiobook on first run, then the last
  pick, remembered in `app-status.json` (`write-kind.ts`; Ruling 22, decided without the user: per user, not per
  folder); `getTagChanges` takes the kind and lists what it removes beyond the fields (`removes`, an MP3's `TPE3` under
  Audiobook), so the list matches the write; M4A genre is written only as freeform GENRE (the engine does it); a
  single-file book has no track number.
- **Strings**: `multiRename.tags.*` in `messages/en/multiRename.json`; other locales hold untranslated skeletons (Czech
  comes with the Czech locale port).
