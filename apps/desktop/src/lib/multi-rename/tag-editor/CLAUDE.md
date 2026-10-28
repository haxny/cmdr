# Tag editor (fork, opt-in): the Tags window

Behind the `multiRename.tagEditor` setting, **off by default**: off, nothing here is reachable and the Multi-Rename
sheet is upstream's (its one foothold, `index.ts`'s `TagsButton`, isn't mounted; no tag IPC). Backend:
`src-tauri/src/tag_editor/CLAUDE.md`.

- **A window per session**, `tags-<sessionId>` on `/tags`, opened only by the main window (`open-tags.ts`): File > Edit
  tags… (`file.editTags`, ⌘⇧E) from the selection, or the sheet's "Tags…". Each open makes a NEW Multi-Rename session
  and HOLDS it (`holdTagsSession`), so eviction never takes it. ❌ Never hand the window the sheet's own session: the
  sheet closing would end it (`reopenForTags` copies it).
- **Closing** goes through the discard guard (Escape = `tags.close`, the red button via `onCloseRequested`), then
  releases, `closeTagEditor`, `closeMultiRename`, `closeSelfWindow()`. ❌ Never `getCurrentWindow().close()` from a key
  handler. The backend ends the session on the window's `Destroyed` too.
- **Two panes like the file panes**, Album | Tracks (`AlbumPane`, `TracksPane`): the global `app-file-list.css` row
  classes, `getRowHeight()`, the list font; fixed columns with one flexing, so nothing scrolls sideways. Keys are
  `Tags window` registry commands answered by the focused pane's listbox (Tab, F2/Enter, ⇧F2, F3/⌥Space), plus the file
  panes' own selection commands (`classifySelectionKey`) and `list-cursor.ts`. ❌ Rows never take focus (no `tabindex`;
  mousedown keeps it on the list), or the keys die after a click. ⌘A / ⌘⇧A arrive from the native menu
  (`onTagsSelectAll` / `onTagsDeselectAll`), never as a keydown.
- **The function-key bar and the dialogs**: `TagsFunctionKeyBar` at the foot runs the tools (F4–F8, ⇧F3–⇧F8) and ⌘S
  Write; each opens a `ModalDialog` beside the content (`TagToolDialogs`, `WriteDialog`). ❌ Nothing goes in the
  window's flow above or below the panes but the one fixed status line: the panes never move. DETAILS § Layout and keys.
- **F3 / ⌥Space** is `tags.play`: plays or stops the Tracks pane's cursor row; plain Space selects, as on files. ⌥Space
  is claimed (macOS types a non-breaking space for it). Playback stops on unmount, on another play, on `ended`, and when
  a write starts (`ApplyPanel`'s `onWriting`); each stop releases the URL.
- **Scope**: the tag tools work on the Tracks pane's selection, ONE selection model (`track-selection.svelte.ts` over
  the file panes' `createSelectionState`; the ☐ column draws it): `all` / `some` / `gone` through `TagsTab`'s
  `runScoped`; a vanished selection never widens to every track. "Open in foobar2000" follows the same scope.
- **Totals are fields** (TrackTotal, DiscTotal), and `store` normalizes every edit for its format (an M4A's `01` shows
  as `1`). Find/replace and format run in the app's Rust (`tag_editor/text_tools.rs`) because they reuse Multi-Rename's
  own engine, not `cmdr-tags`; file name → tags stays the engine's `TagTool::FromFileName`.
- **Unwritten edits are never lost silently**: closing the window goes through `discard-guard.ts` ("Discard tag
  changes?").
- **The tag canon is shown, not enforced**: diacritics are allowed only in Comment and Description (a warning with a
  one-click fix, never a silent strip); a single-file book hides the # column and Renumber; the write kind is Audiobook
  on first run, then the user's last pick (`write-kind.ts`), and the change list names what it removes (an MP3's
  conductor). Details: `tag-editor/` header comments and the backend `DETAILS.md`.
- **Writing is safe**: the undo record is saved first, a file changed on disk since the read is listed as
  `changedOnDisk` and not written, "Write anyway" (data loss) is an explicit choice, and Undo asks first and leaves a
  file that changed after the write (another write, a rename onto its name) alone, counted apart.

Details: `DETAILS.md`; backend: `src-tauri/src/tag_editor/CLAUDE.md`.
