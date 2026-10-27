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
- **Player** (`track-player.svelte.ts`, `PlayButton.svelte`, `TrackList`'s play column, `TagsTab`): one
  `HTMLAudioElement` and one `cmdr-media://` URL at a time. `toggle(path)` mints a URL
  (`tagTrackAudioUrl(sessionId, path)`), plays it, and releases it when playback ends: another track, the same track
  again, `stop()`, `ended`, a decode error, or a URL that arrives after a stop (released unused). `TagsTab` stops it on
  destroy, which covers the Rename tab being shown and the sheet closing. A failure (`failed`) shows
  `multiRename.tags.play.failed`. The audio tokens are per tag session on the Rust side (`playback.rs`), and
  `closeTagEditor` drops what is left. The CSP's `media-src 'self' cmdr-media:` lets `<audio>` load them. "Open in
  foobar2000" calls `openTagsInPlayer` with the checked tracks (or every track), through `runScoped`;
  `playerUnavailable` and other failures are worded.
- **Scope**: `track-selection.svelte.ts` answers `all` (nothing checked), `some` (the checked paths, in session order)
  or `gone` (everything checked vanished in a reload: the run is refused with a notice, then the next takes all). Every
  tool and "Open in foobar2000" take the scope from `TagsTab.runScoped`, after `settled()`.
- **Totals and normalization**: TrackTotal and DiscTotal are tag fields (the numbering wizard sets them; a write keeps
  them). The backend's `store` runs every edit through `cmdr_tags::normalize_for`, so the tab shows what the format will
  hold (M4A integers). The one-file-BOOK rule (no track number) is decided on the session's readable track count
  (`session_files`), not on the scope.
- **Text tools live in the app**: find/replace, format a field and file name to tags run in
  `src-tauri/src/tag_editor/text_tools.rs`, reusing Multi-Rename's `Transform`/`Replace`/`Mask`, so the wildcard, case
  and regex rules match the Rename tab; the pure tag tools stay in `cmdr-tags`.
- **Strings**: `multiRename.tags.*` in `messages/en/multiRename.json`; other locales hold untranslated skeletons (Czech
  comes with the Czech locale port).
