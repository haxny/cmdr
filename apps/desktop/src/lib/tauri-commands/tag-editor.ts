// The Tags tab of Multi-Rename: a tag session per Multi-Rename session, edited in memory, then written. The work
// is the backend's (`src-tauri/src/tag_editor/`); these are pass-throughs.

import {
  commands,
  type ApplyRequest,
  type ContentKind,
  type CoverCandidate,
  type FileChangeView,
  type NamePattern,
  type NamePreviewRow,
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
  NamePattern,
  NamePreviewRow,
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

/**
 * Runs a tool over the tracks at `scope` (paths as `TrackRow.path` gives them, in session order), or over all the
 * session's files when `scope` is `null`. A path the session doesn't have (`unknownTrack`) refuses the run.
 */
export async function runTagTool(
  sessionId: string,
  tool: TagTool,
  scope: string[] | null = null,
): Promise<TagEditorResult<TagEditorView>> {
  return result(await commands.runTagTool(sessionId, tool, scope))
}

/**
 * What `pattern` reads from each file name of `scope` (or of every file when `null`), without changing anything: a
 * row's `fields` is `null` when its name doesn't match. A bad pattern comes back as `badPattern`.
 */
export async function previewTagsFromNames(
  sessionId: string,
  pattern: NamePattern,
  scope: string[] | null = null,
): Promise<TagEditorResult<NamePreviewRow[]>> {
  return result(await commands.previewTagsFromNames(sessionId, pattern, scope))
}

/** What a write as `kind` would change, per file, in current order, with what the kind removes. */
export async function getTagChanges(sessionId: string, kind: ContentKind): Promise<TagEditorResult<FileChangeView[]>> {
  return result(await commands.getTagChanges(sessionId, kind))
}

/** Writes the changed files. The report lists what was written, what would drop items, what failed, and what changed on disk. */
export async function applyTags(sessionId: string, request: ApplyRequest): Promise<TagEditorResult<TagApplyReport>> {
  return result(await commands.applyTags(sessionId, request))
}

/**
 * Writes the old tags of an apply back (`TagApplyReport.applyId`). `sessionId` is the asking sheet: its tag session
 * takes the restored files as read, any other open one keeps its unwritten edits to them.
 */
export async function undoTagApply(applyId: string, sessionId: string | null): Promise<TagEditorResult<RestoreReport>> {
  return result(await commands.undoTagApply(applyId, sessionId))
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

/** The cover every file shares, with its bytes (for a thumbnail), or `null` when there is none or the files differ. */
/** The shared cover as a `data:` URL for the thumbnail, or `null` when there is none or it isn't a known image. */
export async function getCoverImage(sessionId: string): Promise<TagEditorResult<string | null>> {
  return result(await commands.getCoverImage(sessionId))
}
