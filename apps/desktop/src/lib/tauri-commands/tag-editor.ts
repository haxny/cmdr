// The Tags window: a tag session per Multi-Rename session, edited in memory, then written. The work
// is the backend's (`src-tauri/src/tag_editor/`); these are pass-throughs.

import type { UnlistenFn } from '@tauri-apps/api/event'
import {
  commands,
  events,
  type ApplyRequest,
  type ContentKind,
  type CoverCandidate,
  type CoverChoice,
  type FileChangeView,
  type LastTagApply,
  type NamePattern,
  type NamePreviewRow,
  type RestoreReport,
  type TagApplyReport,
  type TagEdit_Deserialize,
  type TagEditorError,
  type TagEditorView,
  type TagTextTool,
  type TagTool,
  type TagsMenuKey,
  type TagsSessionInfo,
  type MultiRenameError,
  type MultiRenameOpened,
} from '#lib/ipc/bindings.js'

export type {
  ApplyRequest,
  LastTagApply,
  CoverCandidate,
  CoverChoice,
  FileChangeView,
  NamePattern,
  NamePreviewRow,
  RestoreReport,
  TagApplyReport,
  TagEdit_Deserialize as TagEdit,
  TagEditorError,
  TagEditorView,
  TagTextTool,
  TagTool,
  TagsMenuKey,
  TagsSessionInfo,
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
 * Runs find/replace (`replace`) or rebuilds one field from a Multi-Rename mask (`format`, e.g. `[N3-]` cuts the first
 * two characters) over the tracks at `scope`, or over all the session's files when `scope` is `null`. A bad search
 * comes back as `badRegex`, a bad mask as `badMask`, an unknown path as `unknownTrack`; nothing changes then.
 */
export async function runTagTextTool(
  sessionId: string,
  tool: TagTextTool,
  scope: string[] | null = null,
): Promise<TagEditorResult<TagEditorView>> {
  return result(await commands.runTagTextTool(sessionId, tool, scope))
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
/**
 * Writes the edits. `continues`: a "Write anyway" after a press that wrote, so Undo reverses both; otherwise a write
 * that writes something becomes the folder's last write (`lastTagApply`).
 */
export async function applyTags(
  sessionId: string,
  request: ApplyRequest,
  continues = false,
): Promise<TagEditorResult<TagApplyReport>> {
  return result(await commands.applyTags(sessionId, request, continues))
}

/**
 * The last tag write of the session's folder, which ⌘Z undoes, or `null`. The backend keeps it for the app's lifetime,
 * so a Tags window opened later over the same folder still offers it.
 */
export async function lastTagApply(sessionId: string): Promise<TagEditorResult<LastTagApply | null>> {
  return result(await commands.lastTagApply(sessionId))
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

/**
 * A cover as a `data:` URL: the one `key` names (a `CoverChoice.key`; `'none'` has no image), or for `null` the cover
 * every file shares. `null` comes back when there is none, the files differ, or it isn't a known image; a key no file
 * carries any more is `unknownCover`.
 */
export async function getCoverImage(
  sessionId: string,
  key: string | null = null,
): Promise<TagEditorResult<string | null>> {
  return result(await commands.getCoverImage(sessionId, key))
}

/**
 * A small PNG `data:` URL of the cover `key` names, for a tile (about twice its size, cached by key in the backend);
 * `null` for `'none'` or a cover that doesn't decode. The large view takes the full image (`getCoverImage`).
 */
export async function getCoverThumbnail(sessionId: string, key: string): Promise<TagEditorResult<string | null>> {
  return result(await commands.getCoverThumbnail(sessionId, key))
}

/** Every distinct cover of the readable files with how many carry it, most common first, the files without one last. */
export async function listTagCovers(sessionId: string): Promise<TagEditorResult<CoverChoice[]>> {
  return result(await commands.listTagCovers(sessionId))
}

/** Gives every file the cover `key` names, in memory (listed for Write, nothing written). */
export async function useCoverForAll(sessionId: string, key: string): Promise<TagEditorResult<TagEditorView>> {
  return result(await commands.useCoverForAll(sessionId, key))
}

/**
 * Gives every file a pasted picture as its cover, in memory. The bytes cross IPC as base64 (a number array would be
 * several times their size); the backend refuses an oversized one before decoding it (`coverTooLarge`), then sniffs
 * the bytes (`pastedNotAnImage`) and caps a cover at 10 MiB (a TIFF is converted).
 */
export async function setCoverFromBytes(sessionId: string, data: Uint8Array): Promise<TagEditorResult<TagEditorView>> {
  return result(await commands.setCoverFromBytes(sessionId, toBase64(data)))
}

/** Gives every file a dropped picture file as its cover, in memory; a folder, FIFO, or symlink is `coverNotAFile`. */
export async function setCoverFromPath(sessionId: string, path: string): Promise<TagEditorResult<TagEditorView>> {
  return result(await commands.setCoverFromPath(sessionId, path))
}

/**
 * A `cmdr-media://` URL that plays the track at `path` (as `TrackRow.path` gives it) in an `<audio>` element. A
 * path that isn't a readable track of the session comes back as `unknownTrack`. Release it with
 * `releaseTagTrackAudio` when playback stops; closing the tag session releases every one.
 */
export async function tagTrackAudioUrl(sessionId: string, path: string): Promise<TagEditorResult<string>> {
  return result(await commands.tagTrackAudioUrl(sessionId, path))
}

/** Ends a URL `tagTrackAudioUrl` gave. Any other URL is ignored. */
export async function releaseTagTrackAudio(url: string): Promise<void> {
  await commands.releaseTagTrackAudio(url)
}

/**
 * Opens tracks of the session in foobar2000 when it's installed, otherwise each in its default app. Resolves once
 * the launch is asked for; `unknownTrack` for a path outside the session, `playerUnavailable` when nothing could
 * start.
 */
export async function openTagsInPlayer(sessionId: string, paths: string[]): Promise<TagEditorResult<null>> {
  return result(await commands.openTagsInPlayer(sessionId, paths))
}

/** A Multi-Rename answer for the Tags window's session calls. */
type SessionResult<T> = { ok: true; value: T } | { ok: false; error: MultiRenameError }

function sessionResult<T>(
  res: { status: 'ok'; data: T } | { status: 'error'; error: MultiRenameError },
): SessionResult<T> {
  return res.status === 'ok' ? { ok: true, value: res.data } : { ok: false, error: res.error }
}

/** Holds a Multi-Rename session for a Tags window (no eviction takes it), and says its track count and folder. */
export async function holdTagsSession(sessionId: string): Promise<SessionResult<TagsSessionInfo>> {
  return sessionResult(await commands.holdTagsSession(sessionId))
}

/** Lets eviction take the session again. */
export async function releaseTagsSession(sessionId: string): Promise<void> {
  await commands.releaseTagsSession(sessionId)
}

/** A new session over an open sheet's files, in their current order, for a Tags window of its own. */
export async function reopenForTags(sessionId: string): Promise<SessionResult<MultiRenameOpened>> {
  return sessionResult(await commands.reopenForTags(sessionId))
}

/**
 * Opens a Multi-Rename session over a pane's selection for a Tags window, as `openMultiRename` does, but with an
 * empty media cache: no EXIF or tag read runs for it. `selectionChanged` when the rows are stale.
 */
export async function openTagsSession(
  listingId: string,
  includeHidden: boolean,
  selectedIndices: number[] | null,
  expectedSequence: number,
): Promise<SessionResult<MultiRenameOpened>> {
  return sessionResult(await commands.openTagsSession(listingId, includeHidden, selectedIndices, expectedSequence))
}

/**
 * Edit > Select all (⌘A) while this Tags window is focused. The menu accelerator fires before the page's keydown
 * sees ⌘A, so this is how the window learns of it (`menu_handlers.rs` routes it here instead of the native
 * `selectAll:`). Returns an `UnlistenFn`.
 */
export async function onTagsSelectAll(callback: () => void): Promise<UnlistenFn> {
  return events.tagsSelectAll.listen(() => {
    callback()
  })
}

/**
 * A File / Select menu key (F2–F8, ⇧F2, ⇧F4, ⇧F8) pressed while this Tags window is focused. Those
 * accelerators fire before the page's keydown, so `menu_handlers.rs` sends the press here (`TagsMenuKey`: the Tags
 * command the item's key runs, and the item's own main-window command). Returns an `UnlistenFn`.
 */
export async function onTagsMenuKey(callback: (key: TagsMenuKey) => void): Promise<UnlistenFn> {
  return events.tagsMenuKey.listen((event) => {
    callback(event.payload)
  })
}

/** Select > Deselect all (⌘⇧A) while this Tags window is focused; routed as `onTagsSelectAll`. */
export async function onTagsDeselectAll(callback: () => void): Promise<UnlistenFn> {
  return events.tagsDeselectAll.listen(() => {
    callback()
  })
}

/** Bytes as base64, in chunks so a large picture never overflows the call stack. */
function toBase64(data: Uint8Array): string {
  const CHUNK = 0x8000
  let binary = ''
  for (let start = 0; start < data.length; start += CHUNK) {
    binary += String.fromCharCode(...data.subarray(start, start + CHUNK))
  }
  return btoa(binary)
}
