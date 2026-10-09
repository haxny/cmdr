# Multi-Rename sheet

The ⌃M sheet over `src-tauri/src/multi_rename/` (the engine and its rules: that module's `CLAUDE.md` / `DETAILS.md`).

- `MultiRenameDialog.svelte` the sheet; opened from `routes/(main)/+page.svelte` with the focused pane's target
  (`getFocusedPaneRenameTarget`), closed back to the pane.
- `multi-rename-state.svelte.ts` the spec (opened on the last settings), the debounced preview (a generation counter
  drops stale answers), presets, Results' edits, Start.
- `spec.ts` the default spec, built-in presets, counts, placeholder insertion. Pure. `preset-menu.ts` F2's rows.
- `last-run.svelte.ts` the session's last run, for Undo (⌥⌫).

## Must-knows

- **Names come from the backend.** The sheet sends the listing id, backend row numbers, the spec, and at Start the ready
  rows it SHOWED, which the backend only checks against its own (`previewOutOfDate` re-previews).
- **Start waits for the preview of the last edit** (`pending`), so it never runs a spec nobody saw; a failed Start
  (`applyError`) doesn't block a retry.
- **TC's keys, each matched on its whole combo**: Enter in a text field renames, F2 opens the presets menu, ⌥Enter
  Results, ⌥⌫ Undo (taking word-delete from the fields), Esc closes. Enter in the preset name saves; none fires
  mid-composition.
- **Results' edits travel with the preview AND the apply**; dropping them from either would rename names nobody saw.
- **A spec error keeps the last good preview** on screen under the message; any other error clears it.
- Built-in preset names are message keys (translated); saved ones are the user's text.

More: `DETAILS.md`.
