# Multi-Rename sheet: details

- **Keyboard-first**: the name mask has focus on open; Tab walks the fields; Enter in a text field renames (TC's
  Start!), a button or menu keeps its own Enter; Esc closes. The placeholder buttons insert at the name mask's caret.
  The footer names each key: Cancel (esc), Undo (⌥⌫), Results (⌥⏎), Rename (⏎).
- **Last settings**: the sheet opens on what the last one closed with (`getMultiRenameLastSpec`), saved on destroy. An
  edit made before they arrive wins (`touched`). `completeSpec` fills fields an older save lacks.
- **Presets (F2)**: the house `Menu` under the Presets button: No change and the built-ins, the saved presets, then Save
  as new preset (an inline name field: Enter saves, Esc drops it), and Save / Delete for a loaded saved preset.
- **Results (⌥⏎)**: the backend writes the preview to a text file, `openFileInEditor` opens it (the user's editor
  setting), and the window's next `focus` reads it back into `edits` (a button reads it at once). A banner says the
  names come from the edited list, with a button to use the settings again. Why focus, not a file watcher: the user
  comes back to Cmdr after saving, and nothing runs while they don't.
- **Undo (⌥⌫)**: `rollbackOperation` on the session's last run (`last-run.svelte.ts`), as the operation log's Roll back
  does; the page toasts and closes the sheet. A typed refusal is worded with `rollbackRefusalNotice`. **Decision: the
  sheet closes after Rename and after Undo.** Why: the target is row numbers, which the renames shift; reopening ⌃M
  takes a fresh target and the remembered settings, so rename, check the pane, ⌃M, ⌥⌫ is the flow.
- **Preview**: reruns `PREVIEW_DELAY_MS` (120 ms) after the last edit. The table draws the first 1,000 rows; the rest
  still rename, and a line says how many aren't shown.
- **Rename** calls `applyMultiRename`; the page shows a toast and closes the sheet. The operation is in the queue, and
  Undo (here or in the operation log) reverses it.
- **Target**: the focused pane's selected rows in row order (backend numbers), or the whole folder when nothing is
  selected; a pane with no backend listing (servers, search results) opens nothing.
- **Gallery**: `not-triggerable`, since the preview is computed from a real listing.
