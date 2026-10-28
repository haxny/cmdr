# Tag editor details

Setting `multiRename.tagEditor` (boolean, default off). On, the tags open in a Tags window of their own: File > Edit
tags… (`file.editTags`, ⌘⇧E) over the focused pane's selection, or the Multi-Rename sheet's "Tags…" button
(`TagsButton`) over the sheet's files. Files in `tag-editor/`:

- **Window**: `open-tags.ts` (main window only: opens a NEW Multi-Rename session, from the selection with
  `openTagsSession` or as a copy of the sheet's with `reopenForTags`, holds it with `holdTagsSession`, then opens the
  window; no track, a failed hold, or a window that won't open gives the session back; a selection the listing moved
  past toasts as ⌃M does), `tags-window.ts` (the opener: label `tags-<sessionId>`, `/tags?session=…&folder=…`, raises
  the window a session already has), `TagsWindow.svelte` (the body: `TagsContent` plus the close; Escape is `tags.close`
  in scope `Tags window`) and the route `routes/tags/+page.svelte` (the shell, and the red button through
  `onCloseRequested`, answered by `tags-close-request.ts`: null-safe, never throws, lets the close go when in doubt).
  Closing releases the hold, then `closeTagEditor`, `closeMultiRename`, `closeSelfWindow()`, which DESTROYS the window
  after the hide and wait, so the page's own close listener can't veto it (a veto once left the window hidden and alive
  for good). Opening a Tags window takes down any `tags-*` window whose session is no longer held
  (`tag_editor/window.rs` `close_orphans`), and a Dock click brings back a hidden or minimized one whose session is held
  (`app_lifecycle.rs`). The Window menu lists every open window (`menu/macos_appkit.rs`). **Why a copy from the sheet**:
  the sheet closes its session on Cancel, Rename, and Undo; a shared one would die under the window. **Why the hold**:
  Multi-Rename evicts idle (30 min) and least-recently-used (cap 4) sessions, and a window left open over lunch must
  keep its edits. The backend also ends a `tags-*` window's session on `Destroyed`
  (`src-tauri/src/tag_editor/window.rs`), so a window that dies without its own close leaks nothing. Capability:
  `src-tauri/capabilities/tags.json` (its description says why each grant). **Same files twice**: Edit tags on the files
  a Tags window already shows (same volume, folder and names, in any order) raises that window and gives the new session
  back (`TagsSessionInfo.alreadyOpen`, from `held_twin`). Other files of the same folder open their own window; a write
  there is still checked per file by fingerprint. **Closing during a write**: while a write or an undo runs
  (`tags.writing`), a close request (Escape, ⌘W, the red button) neither asks "Discard?" nor ends anything: the Write
  dialog says "Writing tags… wait until it's done", then shows the per-file outcome, and the next close goes through the
  guard as usual. Discarding edits already on their way to disk would be a lie, and ending the session mid-write would
  lose the outcome. **Quitting** asks about unwritten Tags windows and waits for a running write: the quit gate
  (`src-tauri/src/quit/`, `src-tauri/src/tag_editor/at_quit.rs`). File > Edit tags… is greyed and the palette row hidden
  while the setting is off (`edit-tags-menu-gate.ts`, palette condition `tagEditorOn`); with nothing selected the
  command says "Select the audio files to tag first." The window's session reads no media (`openTagsSession` /
  `reopenForTags` seed it empty), and held sessions never count against Multi-Rename's cap, so open Tags windows never
  push out a sheet's live session.

- **State**: `tag-editor-state.svelte.ts` (the view from the backend's `TagEditorView`, edits sent by path, `busy`,
  `settled()` so a cell that just lost focus is in before a close/apply, Undo, `writing` and `editing`);
  `last-tag-apply.ts` (the last write's applies, which "Undo tag changes" reverses). **The last write is the backend's**
  (`src-tauri/src/tag_editor/last_apply.rs`), per folder for the app's lifetime, not persisted: a Tags window opened
  later over the same folder offers ⌘Z for it, and the window only mirrors it (asked on open and after every write and
  undo). Why: Undo outlived the Multi-Rename sheet before the window existed, and a write's Undo dying with its window
  broke "undo first". `discard-guard.ts` asks only about edits: a read stuck on a stalled share closes without a
  question.
- **Parts**: `AlbumPane` (one row per shared field, Name | Value; a "multiple values" row is untouched until changed;
  the cursor row's Values… and Clear under the list; `CoverPanel` as the last group, inside the list's scroll),
  `TracksPane` (☐ ▶ # Title Length File; the # column is hidden for a one-file book; # Title Length File are
  `TrackSortHeader` buttons that call `onSort`, the active one flipping direction and any other starting ascending, from
  `view.sort`; the header pads by the scroller's scroll bar width, and a double-click edits # or Title),
  `TagValueEditor` (the box F2 lays over a row), `TagsFunctionKeyBar` (the foot), `TagToolDialogs` (each tool's dialog),
  `WriteDialog` (`ApplyPanel` + `ApplyOutcome`: what will change, the per-file result, Undo; `WouldDropDialog`: items a
  format would lose, Write anyway or Skip).
- **Covers** (`CoverPanel`, `CoverViewer`, `cover-choices.ts`, `cover-drop.ts`): every distinct cover as a tile with how
  many files carry it and its size, the files without one counted (`listTagCovers`, asked again on each new view; a new
  view also clears the section's notice). Tiles show `getCoverThumbnail` (asked once more when none came, then "No
  preview"; a small PNG the backend makes once per cover key); a click opens the cover large in `CoverViewer` (a
  `ModalDialog` owned by `TagsContent`, so `hasDialog()` keeps the window keys out), which fetches the full picture;
  "Use for all files" there is `useCoverForAll`.
- **Paste**: ⌘V is the native Edit > Paste (`paste:` to the webview); WebKit enables it on a focus that isn't a text box
  only when the page `preventDefault`s `beforepaste`, and its `paste` targets the selection, not the focus, so both are
  listened to on the document and taken only while focus is in the section (which is focusable and takes focus on a
  click). No registry key is added, and ⌘V in a text box stays its own. The Paste picture button is the mouse road
  (`navigator.clipboard.read()`; WebKit shows its own Paste callout) and the fallback if ⌘V proves dead. The section
  refuses over 10 MiB before sending, except a TIFF (up to 40 MiB), which the backend converts to PNG or JPEG.
- **IPC cost of a paste**: the bytes cross as a JSON number array: the repo's IPC is JSON throughout
  (`src-tauri/src/commands/DETAILS.md`; `viewer_get_bytes` does the same). At the 10 MiB cap that is about 80 MB of
  transient JS array and a 40 MB body, for a one-shot user action. A raw `tauri::ipc::Request` body would be the
  codebase's first and bypass specta, so it isn't used.
- **Drop**: Tauri's `dragDropEnabled` turns file drags into the webview's `onDragDropEvent` (never an HTML5 `drop`),
  hit-tested as Ask Cmdr does, against the section's rect cut to the Album scroller (`data-cover-clip`), so a drop on
  what shows where the section is scrolled out of view (the rows' strip, the status line, the bar) is never the cover's.
  One path goes to `setCoverFromPath` (the backend waits as long as the read moves, and gives up on a stalled drive),
  two or more ask for one. While a dialog is open the section takes no paste or drop.
- **Decision**: the section sits inside the Album list's scroll after the rows, so its tiles can grow without pushing or
  resizing the panes. Not yet checked in a built app: that ⌘V reaches a focused section; which image type a copied
  picture arrives as (Preview ⌘C, Safari Copy Image, Finder ⌘C on a .jpg, a real .tiff); and that drop positions match
  `getBoundingClientRect` in the Tags window.
- **Layout and keys**: panes | a one-line status | the function-key bar, top to bottom; only the panes flex. The status
  line is always there (empty or not) and its notices cut with a tooltip, so a notice never moves the panes; every tool
  and the write open in a `ModalDialog` rendered beside the content, never in its flow. Decision/Why: the tools bar
  above the panes and the Apply panel under them pushed the table around and hid the write's answer in a small box (the
  user's report, 2026-10-27). The bar (`tags-function-keys.ts` holds its slots) mirrors the main window's
  `FunctionKeyBar` look but not its code, which is wired to the file commands: F2 Edit, F3 Play, F4 Replace, F5 Format,
  F6 From name, F7 Number, F8 Clear, then ⌘⏎ Write; Shift shows ⇧F2 Edit #, ⇧F3 Open in player, ⇧F4 Case, ⇧F5 Clean up,
  ⇧F8 Remove extras, and ⌘⇧S Review in Write's place. Its chips are the commands' live bindings
  (`getFirstShortcutReactive`), its buttons never take focus, and a disabled slot is a command that can't act (read-only
  leaves Play and Open in player). The window's keydown (`TagsWindow`) answers the same `Tags window` commands through
  `TagsContent.runCommand`, skipping a key a box, a pane or a dialog already claimed, every key while a dialog is open,
  every key but ⌘⏎ / ⌘S / ⌘⇧S (which the value box commits first) while a value is typed (⌘Z stays the box's text undo),
  and Enter on a focused control. Clean up (⇧F5, `tags.cleanUp`) is not in the plan's key list: it holds the one-press
  tools the old bar had (Remove diacritics, Renumber, Strip track prefix, Album artist = Artist, Narrator from text,
  Clear title = album). Clear (F8) empties one field, starting on the cursor's, on the selection (`setOne` per readable
  selected track) or every file (`setAll`). Each tool takes the field the user was on when it opened, since focus then
  moves into the dialog. The text tools stay open to run again; Case, Clean up, Remove extras and Clear close once run.
  ⌘⏎ and ⌘S (`tags.write`) write at once: `TagEditorState.writeNow` is the Write dialog's own write without its list (it
  waits for the edits out, lists as the remembered kind, writes nothing when nothing would change, then the same apply
  request). ❗ When the kind would remove a field from any file (`removes`, an MP3's conductor for Audiobook) it writes
  nothing and opens the list instead (`reviewFirst`, worded in the status line): a removal is only ever written from the
  list that names it. The backend enforces it (`ApplyRequest.allowRemovals` → `wouldRemove`, backend `DETAILS.md` §
  Write and undo): ⌘⏎ allows none, the dialog's write only the files it listed with a removal, and a file held back at
  the write opens the list again, continuing the press. With `multiRename.tagEditor.absSideFiles` on, the list also
  names Audiobookshelf's `reader.txt` / `desc.txt` under the files (`side-files.ts`; `TagEditorState.sideFileChanges`,
  `[]` with the setting off, so off changes nothing): ⌘⏎ creates a missing one but opens the list for one that says
  something else (`reviewFirst.replacing`), and the dialog's write sends back only the replacements it showed with their
  old text (`allowSideFiles`; backend `DETAILS.md` § Write and undo). An edit landing between ⌘⏎'s list and its write
  writes nothing and lists again (`changed`). The player stops first, a second press while it runs does nothing,
  would-drop files ask in a `WouldDropDialog` over the panes ("Write anyway" is `answerWriteAnyway`, shared with the
  dialog: an edit landed meanwhile opens the list, continuing the press so Undo keeps both applies), and the status line
  says how many files were written and how many weren't, until the next edit or 10 s. Refusals reuse the dialog's words
  (`refusalOf`). ⌘⇧S (`tags.review`) opens `WriteDialog` listing the changes (`ApplyPanel.start()`), where ⌘⏎ or Write
  tags writes; it can't close while a write is out. ⌘Z with a write to undo opens it on the undo question
  (`ApplyPanel.undo()`; a "no" closes it), and without one says so in the status line. ⌘W reaches the window as the
  native Close tab item, which closes a focused child window through its close request, so it meets the same discard
  guard as Esc. A text tool runs again on each press while its dialog stays open (a second Format re-applies the mask to
  the value the first one made), as the in-flow panels did. A command that can't act (tags still being read, read-only,
  no extras, a dialog open) does nothing: the reading notice or the status line already says why. Closing the Write
  dialog on a write that left files unwritten keeps "Some files weren't written" in the status line.
- **Keys the native menu takes first** (macOS): this window installs the main menu (greyed), whose File / Select items
  register F2–F8, ⇧F2, ⇧F4 and ⇧F8. Those accelerators fire ahead of the page and the file-scoped guard would drop them,
  so `menu_handlers.rs` sends each to the focused `tags-*` window as `TagsMenuKey`
  (`tag_editor::window::menu_key_command_of`: the item's Tags command, plus the item's own command). `menuKeyCommand`
  acts only while that item's first key is still one of the Tags command's keys (a rebound file command never runs a tag
  tool), under the keydown's rules (no dialog open, no value being typed). If the page sees the key too, the press acts
  once: `TagsContent`'s `createTagsKeyEcho` (per command, `createKeyRoadEcho`'s 300 ms) drops the other road's twin; the
  panes ask it for F2, ⇧F2 and F3 (never Enter or ⌥Space, which no menu item holds). Main focused: no Tags window is,
  and the items run as before. ❌ View > Switch pane's Tab is never routed. Decision/Why: the page sees Tab despite that
  accelerator (main's dialogs and inline rename move focus with it, and main's keydown resolver leaves Tab unclaimed in
  a dialog for exactly that, `routes/(main)/DETAILS.md` § The dialog gate); a routed Tab would switch panes from a
  dialog or a field where Tab must move focus. The panes answer Tab themselves; elsewhere it is the browser's and the
  dialog focus trap's. Edit > Undo (⌘Z) is a predefined item, so it can't be routed: its key reaches the page's keydown
  (as ⌘G does in main, both roads fire for a ⌘ chord), which runs `tags.undo` (the last pending edit at once when
  `view.canUndoEdit`, else the last write after asking; ⇧⌘Z is `tags.redo`) outside a text box and leaves ⌘Z to the
  box's text undo inside one; the native `undo:` may then also run, with no text edit to undo. ⌘S, ⇧F3, ⇧F5, ⌥Space and
  Esc have no menu item and reach the page directly; ⌘W is Close tab. Pinned: `menu_bar_test`'s
  `every_main_menu_key_a_tags_window_command_holds_reaches_the_tags_window`. ❗ Not yet checked by hand in a built app:
  F2–F8, ⇧F2–⇧F5, ⇧F8, Tab (panes, a tool dialog, the value box), ⌘S, ⌘Z (in and out of a box), ⌘W, Esc (Plan 2c Task 4b
  fix rounds 1–2, 2026-10-28).
- **Invalid date flag**: `AlbumPane` takes the view's `badDates` and marks the Date row `invalid` (red row, the
  `multiRename.tags.date.invalid` hint under the list, `aria-describedby` on the row and its editor) while it is above
  zero. It is on open and follows every edit, so correcting the Date clears it. Why: an existing date such as
  `2010-20-09` made every write fail; the user now sees the cause before writing. The write's own per-file refusal is
  `multiRename.tags.write.error.badDate`. Backend side: `tag_editor/DETAILS.md` § Invalid dates.
- **A value box with text in it settles before any other command** (`runCommand` → each pane's `settleEditor` →
  `TagValueEditor.commit()`): sendable text is sent, as a click away does; refused text («multiple values» marker left
  in) keeps the box open with its hint and the command doesn't run. A click away from refused text takes focus back,
  except into a dialog (its focus trap would fight it, a loop the round-1 review found).
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
  click away commits, ⌥Enter adds a line in Comment and Description. A text the row can't send (`sharedEditBlock`: the
  «multiple values» marker or the «…and N more» tail still in it) keeps the box open with a hint saying what to delete,
  on Enter, ⌘S and a click away alike (the box takes focus back, so the typing is never dropped without a word; Esc
  gives it up). The hint follows the typing, goes once the text can be sent, and is scrolled into view when it appears.
  Case, Replace and Clear take the Album cursor row's field while that pane is active, Title while the Tracks pane is,
  none once focus leaves both. ⌘A / ⌘⇧A: Select all and Deselect all are registered accelerators that fire before the
  page sees the key, so `menu_handlers.rs` sends `tags-select-all` / `tags-deselect-all` to the focused `tags-*` window
  (`focused_tags_window`); `TagsWindow` selects the text of a text box with focus (never a checkbox), else every track,
  and clears the track selection.
- **⌘X / ⌘C / ⌘V on a field**: Edit > Cut, Copy and Paste go to the focused `tags-*` window as `tags-clipboard`
  (`TagsContent.clipboard`, `tags-clipboard.ts`). A focused text box, or focus in the cover section, gets the same
  action back as the native `cut:` / `copy:` / `paste:` (`tagsNativeEdit`), so the box's own clipboard and its typing
  undo, and the cover's picture paste (`beforepaste` / `paste`, below), work as before; under a dialog nothing else
  happens. Otherwise Copy puts the cursor field's text on the clipboard (Album: the row, a «multiple values» row as its
  distinct values joined by "; " without the marker; Tracks: the active cell, # or Title; an empty field copies ""), Cut
  copies then empties it as Delete does (not when the copy didn't go through), and Paste sets the clipboard's plain text
  (`field-clipboard.ts`: one trailing line break dropped; a one-line field takes the first line, a multi-value field
  joins the lines with "; "; Comment and Description keep them) on the Album row in Clear's scope, or on the Tracks
  cursor row, or on every selected row when several are and the cursor is on one. Each is a pending edit (⌘Z). The
  clipboard goes through the backend (`copyToClipboard`, `readClipboardText`), ❌ not `navigator.clipboard`: the menu's
  action is no user gesture in the page, so WebKit would refuse the read or show its Paste callout. A read that fails, a
  clipboard with no text (or only blanks), and text over the field's limit (`pasteRefusal`: 64 KiB for Comment and
  Description, 4 KiB for the rest) say so in the status line and change nothing.
- **Why edits are keyed by path**: the session's order can change under the view; every edit stays with its file.
  `open()` fetches the view again (`openTagEditor(id)`, never a restart) when asked, so the list follows the order; the
  unwritten edits live in the backend session and come back with it. A view answer older than one already shown is
  dropped.
- **`settled()` is bounded** (`SETTLE_LIMIT_MS`, 5 s): a call that never answers (a stalled network mount) can't hold
  the window. It answers `false` then, and the discard guard asks "Discard tag changes?" anyway, so Cancel and Esc
  always end in a question or a close.
- **A write press always answers** (`ApplyPanel`, in `WriteDialog`): it waits up to `WRITE_WAIT_LIMIT_MS` (30 s,
  `write-wait.ts`) for the calls out, showing "Finishing the last change…" after `WAITING_SHOWN_AFTER_MS`; a wait that
  gives up, a list that can't be made, and a refused write are worded next to the buttons (`notice`, the `refused`
  step), once: `changes` and `apply` leave the state's shared `error` (the window's top line, for edits) alone. A press
  runs once (`oncePressed`), so a double click or ⌘⏎ before a step shows starts no second write. A write where no file
  was written says "No file was written." first, each file with why. Regression anchor (2026-10-27): on a real 22-file
  audiobook every file was refused for a bad date and the write looked like nothing happened.
- **Apply / undo**: the old tags are recorded before any write; failures are per file and the rest are written; Undo
  restores exactly the written files after asking, and leaves one that changed since (its fingerprint moved) alone.
  Other open sessions keep their unwritten edits, rebased per field.
- **Tag canon**: no diacritics except Comment and Description; ContentKind is Audiobook on first run, then the last
  pick, remembered in `app-status.json` (`write-kind.ts`; Ruling 22, decided without the user: per user, not per
  folder); `getTagChanges` takes the kind and lists what it removes beyond the fields (`removes`, an MP3's `TPE3` under
  Audiobook), so the list matches the write; M4A genre is written only as freeform GENRE (the engine does it); a
  single-file book has no track number.
- **Player** (`track-player.svelte.ts`, `TracksPane`'s ▶ column, F3 and ⌥Space (`tags.play`), `TagsContent`): one
  `HTMLAudioElement` and one `cmdr-media://` URL at a time. `toggle(path)` mints a URL
  (`tagTrackAudioUrl(sessionId, path)`), plays it, and releases it when playback ends: another track, the same track
  again, `stop()`, `ended`, a decode error, or a URL that arrives after a stop (released unused). `TagsContent` stops it
  on destroy, which covers the window closing. A failure (`failed`) shows `multiRename.tags.play.failed`. The audio
  tokens are per tag session on the Rust side (`playback.rs`), and `closeTagEditor` drops what is left. The CSP's
  `media-src 'self' cmdr-media:` lets `<audio>` load them. "Open in foobar2000" (⇧F3) calls `openTagsInPlayer` with the
  selected tracks (or every track), through `runScoped`; `playerUnavailable` and other failures are worded in the status
  line.
- **Scope**: `track-selection.svelte.ts` holds the Tracks pane's selection as the file panes do (`createSelectionState`,
  indices over the view's order; `sync` carries it across a reload by path) and answers `all` (nothing selected), `some`
  (the selected paths, in session order) or `gone` (everything selected vanished in a reload: the run is refused with a
  notice, then the next takes all). Every tool and "Open in foobar2000" take the scope from `TagsContent.runScoped`,
  after `settled()`. The Album pane follows the selection too: `TagsContent` sends it to `setViewScope` 120 ms after it
  last changed (one call for a run of ⇧↓; sorting doesn't change it, being keyed by path), the backend sums up only
  those tracks in `shared` and says how many in `scopeCount` (the header's "5 of 42 tracks"; `null` for all), and every
  Album edit road (typing, a picked value, Clear, Delete, paste, cut) goes through `editAlbum`, which turns each
  `setAll` into a `setOne` per selected readable track. The track list, cover, dirty marks and bad dates stay over every
  track. A scope is no edit (no undo step). The pane never shows one scope while an edit goes to another: `editAlbum`
  first `flush`es a selection change still waiting (the view shows the new scope before the edit is sent), the scope
  sender waits for `settled()`, and `replaceView` keeps the newest view by the backend's `viewSeq` (stamped under the
  session lock), not by ask order, so a late answer never hides a later scope or edit. A newest view whose `scopeCount`
  doesn't match the selection sends the scope again, once. The editor's and Clear's accessible names say "for 5 of 42
  tracks" while scoped (`editLabelScoped`, `clearRowLabelScoped`).
- **Totals and normalization**: TrackTotal and DiscTotal are tag fields (the numbering wizard sets them; a write keeps
  them). The backend's `store` runs every edit through `cmdr_tags::normalize_for`, so the window shows what the format
  will hold (M4A integers). The one-file-BOOK rule (no track number) is decided on every audio track the session lists
  (`session_files`, readable or not), not on the scope.
- **Text tools live in the app**: find/replace and format a field run in `src-tauri/src/tag_editor/text_tools.rs`,
  reusing Multi-Rename's `Transform`/`Replace`/`Mask`, so the wildcard, case and regex rules match Multi-Rename's; the
  pure tag tools (file name → tags, `cmdr_tags::TagTool::FromFileName`, among them) stay in `cmdr-tags`.
- **Strings**: `multiRename.tags.*` in `messages/en/multiRename.json`; other locales hold untranslated skeletons (Czech
  comes with the Czech locale port). `sync-locale-keys.ts` copies a plural message's English branches only, so the extra
  CLDR categories i18n-plural asks for (`many` in es, fr, pt; `few` and `many` in ru, for
  `multiRename.tags.clearField.some`) are English copies added by hand: deliberate, not stale.
