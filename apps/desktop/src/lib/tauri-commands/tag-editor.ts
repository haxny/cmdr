// The Tags tab of Multi-Rename: a tag session per Multi-Rename session, edited in memory, then written. The work
// is the backend's (`src-tauri/src/tag_editor/`); these are pass-throughs.

import {
  commands,
  type ApplyRequest,
  type CoverCandidate,
  type FileChangeView,
  type RestoreReport,
  type TagApplyReport,
  type TagEdit_Deserialize,
  type TagEditorError,
  type TagEditorView,
  type TagTool,
} from '#lib/ipc/bindings.js'

export type {
  ApplyRequest,
  CoverCandidate,
  FileChangeView,
  RestoreReport,
  TagApplyReport,
  TagEdit_Deserialize as TagEdit,
  TagEditorError,
  TagEditorView,
  TagTool,
}

/** A typed answer the tab words itself: the value, or why there is none. */
export type TagEditorResult<T> = { ok: true; value: T } | { ok: false; error: TagEditorError }

function result<T>(res: { status: 'ok'; data: T } | { status: 'error'; error: TagEditorError }): TagEditorResult<T> {
  return res.status === 'ok' ? { ok: true, value: res.data } : { ok: false, error: res.error }
}

/**
 * Starts the tag session of a Multi-Rename session. Without `restart` an open session comes back with its
 * edits; with it every file is read again and the edits are dropped.
 */
export async function openTagEditor(sessionId: string, restart = false): Promise<TagEditorResult<TagEditorView>> {
  return result(await commands.openTagEditor(sessionId, restart))
}

/** Applies edits in memory. An unknown track (`unknownTrack`) refuses the whole batch. */
export async function editTags(
  sessionId: string,
  edits: TagEdit_Deserialize[],
): Promise<TagEditorResult<TagEditorView>> {
  return result(await commands.editTags(sessionId, edits))
}

/** Runs a clean-up tool over all the session's files. */
export async function runTagTool(sessionId: string, tool: TagTool): Promise<TagEditorResult<TagEditorView>> {
  return result(await commands.runTagTool(sessionId, tool))
}

/** What a write would change, per file, in current order. */
export async function getTagChanges(sessionId: string): Promise<TagEditorResult<FileChangeView[]>> {
  return result(await commands.getTagChanges(sessionId))
}

/** Writes the changed files. The report lists what was written, what would drop items, what failed, and what changed on disk. */
export async function applyTags(sessionId: string, request: ApplyRequest): Promise<TagEditorResult<TagApplyReport>> {
  return result(await commands.applyTags(sessionId, request))
}

/** Writes the old tags of an apply back (`TagApplyReport.applyId`). */
export async function undoTagApply(applyId: string): Promise<TagEditorResult<RestoreReport>> {
  return result(await commands.undoTagApply(applyId))
}

/** Ends the tag session, dropping its edits. */
export async function closeTagEditor(sessionId: string): Promise<void> {
  await commands.closeTagEditor(sessionId)
}

/** The jpg, png and webp images in the session's folder, by name, with their size. */
export async function listCoverCandidates(sessionId: string): Promise<TagEditorResult<CoverCandidate[]>> {
  return result(await commands.listCoverCandidates(sessionId))
}

/** Gives every file the image `fileName` of the session's folder as its cover (a plain name, at most 10 MiB). */
export async function setCoverFromFile(sessionId: string, fileName: string): Promise<TagEditorResult<TagEditorView>> {
  return result(await commands.setCoverFromFile(sessionId, fileName))
}
