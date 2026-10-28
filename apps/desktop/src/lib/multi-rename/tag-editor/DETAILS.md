# Tag editor details

Setting `multiRename.tagEditor` (boolean, default off). On, the tags open in a Tags window of their own: File > Edit
tags… (`file.editTags`, ⌘⇧E) over the focused pane's selection, or the Multi-Rename sheet's "Tags…" button
(`TagsButton`) over the sheet's files. Files in `tag-editor/`:

- **Window**: `open-tags.ts` (main window only: opens a NEW Multi-Rename session, from the selection with
  `openMultiRename` or as a copy of the sheet's with `reopenForTags`, holds it with `holdTagsSession`, then opens the
  window; no track, a failed hold, or a window that won't open gives the session back; a selection the listing moved
  past toasts as ⌃M does), `tags-window.ts` (the opener: label `tags-<sessionId>`, `/tags?session=…&folder=…`, raises
  the window a session already has), `TagsWindow.svelte` (the body: `TagsTab` plus the close; Escape is `tags.close` in
  scope `Tags window`) and the route `routes/tags/+page.svelte` (the shell, and the red button through
  `onCloseRequested`). Closing releases the hold, then `closeTagEditor`, `closeMultiRename`, `closeSelfWindow()`. **Why
  a copy from the sheet**: the sheet closes its session on Cancel, Rename, and Undo; a shared one would die under the
  window. **Why the hold**: Multi-Rename evicts idle (30 min) and least-recently-used (cap 4) sessions, and a window
  left open over lunch must keep its edits. The backend also ends a `tags-*` window's session on `Destroyed`
  (`src-tauri/src/tag_editor/window.rs`), so a window that dies without its own close leaks nothing. Capability:
  `src-tauri/capabilities/tags.json` (its description says why each grant). **Two windows over the same files**: each
  open is a new session, so Edit tags twice on one selection opens two windows, each with its own tag session. That's
  safe (a write checks every file's fingerprint, so the second window's stale write is refused per file) but can
  confuse; raising a window already open over the same folder and names would need main to track them. File > Edit tags…
  is greyed and the palette row hidden while the setting is off (`edit-tags-menu-gate.ts`, palette condition
  `tagEditorOn`); with nothing selected the command says "Select the audio files to tag first." The window's session
  reads no media (`openTagsSession` / `reopenForTags` seed it empty), and held sessions never count against
  Multi-Rename's cap, so open Tags windows never push out a sheet's live session.

- **State**: `tag-editor-state.svelte.ts` (the view from the backend's `TagEditorView`, edits sent by path, `busy`,
  `settled()` so a cell that just lost focus is in before a close/apply, Undo); `last-tag-apply.svelte.ts` (the last
  write's applies, which "Undo tag changes" reverses; lives as long as its window, not persisted); `discard-guard.ts`.
- **Parts**: `AlbumPane` (one row per shared field, Name | Value; a "multiple values" row is untouched until changed;
  the cursor row's Values… and Clear under the list; `CoverPanel` as the last group), `TracksPane` (☐ ▶ # Title Length
  File; the # column is hidden for a one-file book), `TagValueEditor` (the box F2 lays over a row), `TagToolsBar`
  (clean-up tools over the whole session, Case on the active pane's field), `ApplyPanel` + `ApplyOutcome` (what will
  change, the per-file result, Undo), `WouldDropDialog` (items a format would lose; Write anyway or Skip).
- **Panes** (`tags-panes.svelte.ts`: the active pane, each pane's cursor, Tab's switch). Decision: the rows use the file
  panes' classes directly (`.full-list-container` › `.file-entry` with `.is-striped` / `.is-selected` /
  `.is-under-cursor`, `.is-focused` on the container) rather than `ColumnList`. Why: `ColumnList` draws its own cursor
  fill (`--color-accent-subtle`), font (`--font-size-md`) and padding, and takes one static `rowClass` for every row, so
  per-row selection and stripes can't reach `app-file-list.css`; overriding it all would be a second look to keep in
  step. `FullList`'s box rules (header, gutter, row padding, descender room) live once, in `tag-pane.css`, imported by
  both panes. Rows carry no `tabindex` and a row's mousedown is prevented and refocuses the list, as in `FullList`: a
  focusable row would take focus on a click, and the pane answers keys only from its listbox. Keys go to the focused
  pane's listbox only (`e.target === list`), so Tab is the browser's anywhere else (tools, cover, Write). F2/Enter
  (`tags.edit`) and ⇧F2 (`tags.editNumber`) open `TagValueEditor` over the cell, placed from the cell's box in the
  scroller (`pane-cursor.ts`'s `editorBox`), re-placed when the scroller resizes, and scrolled into view on open so a
  multiline box at the pane's foot isn't cut off. Enter commits, Esc cancels (claimed, so it never closes the window), a
  click away commits, ⌥Enter adds a line in Comment and Description. Enter on a text the row can't send
  (`sharedEditBlock`: the «multiple values» marker or the «…and N more» tail still in it) keeps the box open with a hint
  saying what to delete. Case takes the Album cursor row's field while that pane is active, Title while the Tracks pane
  is, none once focus leaves both (the tools bar keeps the pane, though the pane stops drawing its cursor as focused).
  ⌘A / ⌘⇧A: Select all and Deselect all are registered accelerators that fire before the page sees the key, so
  `menu_handlers.rs` sends `tags-select-all` / `tags-deselect-all` to the focused `tags-*` window
  (`focused_tags_window`); `TagsWindow` selects the text of a text box with focus (never a checkbox), else every track,
  and clears the track selection.
- **Why edits are keyed by path**: the session's order can change under the view; every edit stays with its file.
  `open()` fetches the view again (`openTagEditor(id)`, never a restart) when asked, so the list follows the order; the
  unwritten edits live in the backend session and come back with it. A view answer older than one already shown is
  dropped.
- **`settled()` is bounded** (`SETTLE_LIMIT_MS`, 5 s): a call that never answers (a stalled network mount) can't hold
  the window. It answers `false` then, and the discard guard asks "Discard tag changes?" anyway, so Cancel and Esc
  always end in a question or a close.
- **A write press always answers** (`ApplyPanel`): it waits up to `WRITE_WAIT_LIMIT_MS` (30 s, `write-wait.ts`) for the
  calls out, showing "Finishing the last change…" after `WAITING_SHOWN_AFTER_MS`; a wait that gives up, a list that
  can't be made, and a refused write are worded next to the buttons (`notice`, the `refused` step), once: `changes` and
  `apply` leave the state's shared `error` (the tab's top line, for edits) alone. A press runs once (`oncePressed`), so
  a double click or ⌘⏎ before a step shows starts no second write. A write where no file was written says "No file was
  written." first, each file with why. Regression anchor (2026-10-27): on a real 22-file audiobook every file was
  refused for a bad date and the write looked like nothing happened.
- **Apply / undo**: the old tags are recorded before any write; failures are per file and the rest are written; Undo
  restores exactly the written files after asking, and leaves one that changed since (its fingerprint moved) alone.
  Other open sessions keep their unwritten edits, rebased per field.
- **Tag canon**: no diacritics except Comment and Description; ContentKind is Audiobook on first run, then the last
  pick, remembered in `app-status.json` (`write-kind.ts`; Ruling 22, decided without the user: per user, not per
  folder); `getTagChanges` takes the kind and lists what it removes beyond the fields (`removes`, an MP3's `TPE3` under
  Audiobook), so the list matches the write; M4A genre is written only as freeform GENRE (the engine does it); a
  single-file book has no track number.
- **Player** (`track-player.svelte.ts`, `TracksPane`'s ▶ column and ⌥Space, `TagsTab`): one `HTMLAudioElement` and one
  `cmdr-media://` URL at a time. `toggle(path)` mints a URL (`tagTrackAudioUrl(sessionId, path)`), plays it, and
  releases it when playback ends: another track, the same track again, `stop()`, `ended`, a decode error, or a URL that
  arrives after a stop (released unused). `TagsTab` stops it on destroy, which covers the window closing. A failure
  (`failed`) shows `multiRename.tags.play.failed`. The audio tokens are per tag session on the Rust side
  (`playback.rs`), and `closeTagEditor` drops what is left. The CSP's `media-src 'self' cmdr-media:` lets `<audio>` load
  them. "Open in foobar2000" calls `openTagsInPlayer` with the selected tracks (or every track), through `runScoped`;
  `playerUnavailable` and other failures are worded.
- **Scope**: `track-selection.svelte.ts` holds the Tracks pane's selection as the file panes do (`createSelectionState`,
  indices over the view's order; `sync` carries it across a reload by path) and answers `all` (nothing selected), `some`
  (the selected paths, in session order) or `gone` (everything selected vanished in a reload: the run is refused with a
  notice, then the next takes all). Every tool and "Open in foobar2000" take the scope from `TagsTab.runScoped`, after
  `settled()`. Selected tracks don't narrow the Album pane's shared fields.
- **Totals and normalization**: TrackTotal and DiscTotal are tag fields (the numbering wizard sets them; a write keeps
  them). The backend's `store` runs every edit through `cmdr_tags::normalize_for`, so the tab shows what the format will
  hold (M4A integers). The one-file-BOOK rule (no track number) is decided on the session's readable track count
  (`session_files`), not on the scope.
- **Text tools live in the app**: find/replace and format a field run in `src-tauri/src/tag_editor/text_tools.rs`,
  reusing Multi-Rename's `Transform`/`Replace`/`Mask`, so the wildcard, case and regex rules match Multi-Rename's; the
  pure tag tools (file name → tags, `cmdr_tags::TagTool::FromFileName`, among them) stay in `cmdr-tags`.
- **Strings**: `multiRename.tags.*` in `messages/en/multiRename.json`; other locales hold untranslated skeletons (Czech
  comes with the Czech locale port).
