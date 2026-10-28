/**
 * The last tag write, which "Undo tag changes" (⌘Z) reverses: every apply of one "Write tags" press (the first and,
 * after "Write anyway", the one for the files that would lose data). The backend keeps it per folder for the app's
 * lifetime (`tag_editor/last_apply.rs`), so a Tags window opened later over the same folder still offers it; a window
 * only mirrors it (`tag-editor-state.svelte.ts`).
 */

import type { LastTagApply } from '#lib/tauri-commands/index.js'

export type { LastTagApply }

/** How many files Undo would put back. */
export function undoableCount(write: LastTagApply): number {
  return write.applies.reduce((sum, apply) => sum + apply.written, 0)
}
