/**
 * Edit > Cut, Copy or Paste (⌘X, ⌘C, ⌘V) in the Tags window, from the native menu (`TagsWindow` → `TagsContent`). A
 * focused text box, and the cover section (its picture paste, `CoverPanel`), get the native action back, so they keep
 * their own. Under a dialog nothing else happens. Otherwise: Copy puts the cursor field's text on the clipboard, Cut
 * copies it and then empties the field as Delete does, Paste sets the clipboard's plain text (unless it's blank or over
 * the field's limit, `pasteRefusal`). On a Tracks row Cut and Paste, like Delete, set every selected row when the
 * cursor row is one of them (`cursorTracks`). Each edit is a pending edit, so ⌘Z takes it back. What can't be done is worded
 * through `say`, for the status line.
 */
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { TagField } from '#lib/ipc/bindings.js'
import { getAppLogger } from '#lib/logging/logger.js'
import {
  copyToClipboard,
  readClipboardText,
  tagsNativeEdit,
  type TagEditorView,
  type TagsClipboardAction,
} from '#lib/tauri-commands/index.js'
import { trackCellValue } from './shared-field-input'
import { pastedValues, pasteRefusal, sharedCopyText } from './field-clipboard'
import type { TagEditorState } from './tag-editor-state.svelte'
import { textBoxOf } from './tags-function-keys'
import type { TagsPaneId } from './tags-panes.svelte'
import type { ToolScope } from './track-selection.svelte'
import type { TrackCell } from './typed-key'

const log = getAppLogger('tagEditor')

/** What ⌘X / ⌘C / ⌘V act on: the Album pane's cursor row (`path` null), or a Tracks row's cell. `text` is its copy. */
interface ClipboardTarget {
  field: TagField
  text: string
  path: string | null
  index: number
}

export interface TagsClipboardDeps {
  tags: TagEditorState
  /** The active pane and both cursors. */
  panes: { readonly active: TagsPaneId | null; readonly albumCursor: number; readonly tracksCursor: number }
  /** The Tracks pane's active cell (#, Disc or Title). */
  activeTrackField: () => TrackCell
  selection: {
    readonly count: number
    has: (index: number) => boolean
    scope: () => ToolScope
  }
  /** Sets `field` on the selected tracks, or every file when none is selected (Clear's scope). */
  setFieldScoped: (field: TagField, values: string[]) => Promise<void>
  /** Empties the target's field, as Delete does (on a Tracks row, `cursorTracks`'s rows). */
  clearTarget: (target: { field: TagField; path: string | null }) => Promise<void>
  /** A dialog is open over the panes. */
  hasDialog: () => boolean
  /** Words a refusal or a failure in the status line; `null` clears the last one. */
  say: (key: MessageKey | null) => void
}

/** The paths of the tracks whose tags were read: an edit can name only these. */
export function readablePathsOf(view: TagEditorView | null): Set<string> {
  return new Set(view?.tracks.filter((track) => track.readError === null).map((track) => track.path))
}

/**
 * The tracks an edit on the Tracks pane's cursor row sets (⌘V, ⌘X, Delete): that row, or, when several tracks are
 * selected and the row is one of them, every selected one (as a spreadsheet fills a selection). Readable tracks only.
 */
export function cursorTracks(
  selection: TagsClipboardDeps['selection'],
  view: TagEditorView | null,
  target: { path: string; index: number },
): string[] {
  const readable = readablePathsOf(view)
  const scope = selection.scope()
  if (selection.count > 1 && selection.has(target.index) && scope.kind === 'some') {
    return scope.paths.filter((path) => readable.has(path))
  }
  return readable.has(target.path) ? [target.path] : []
}

/** The window's ⌘X / ⌘C / ⌘V handler. */
export function createTagsClipboard(deps: TagsClipboardDeps): (action: TagsClipboardAction) => Promise<void> {
  const { tags, panes, selection } = deps

  function targetOf(): ClipboardTarget | null {
    const view = tags.view
    if (view === null) return null
    if (panes.active === 'album') {
      const index = Math.max(0, Math.min(panes.albumCursor, view.shared.length - 1))
      const row = view.shared.at(index)
      return row === undefined ? null : { field: row.field, text: sharedCopyText(row), path: null, index }
    }
    if (panes.active !== 'tracks') return null
    const index = Math.max(0, Math.min(panes.tracksCursor, view.tracks.length - 1))
    const track = view.tracks.at(index)
    if (track === undefined) return null
    const field = deps.activeTrackField()
    return {
      field,
      text: trackCellValue(track, field),
      path: track.path,
      index,
    }
  }

  async function copyText(text: string): Promise<boolean> {
    try {
      await copyToClipboard(text)
      return true
    } catch (e) {
      log.warn("couldn't copy a field to the clipboard: {reason}", { reason: e instanceof Error ? e.name : typeof e })
      deps.say('multiRename.tags.clipboard.writeFailed')
      return false
    }
  }

  /** The clipboard's plain text, or `null` after wording why there is none to paste. */
  async function clipboardText(): Promise<string | null> {
    let text: string | null
    try {
      text = await readClipboardText()
    } catch (e) {
      log.warn("couldn't read the clipboard: {reason}", { reason: e instanceof Error ? e.name : typeof e })
      deps.say('multiRename.tags.clipboard.readFailed')
      return null
    }
    if (text === null || text.trim() === '') {
      deps.say('multiRename.tags.clipboard.noText')
      return null
    }
    return text
  }

  async function paste(target: ClipboardTarget): Promise<void> {
    const text = await clipboardText()
    if (text === null) return
    const refused = pasteRefusal(target.field, text)
    if (refused !== null) {
      deps.say(refused)
      return
    }
    const values = pastedValues(target.field, text)
    const { field, path } = target
    if (path === null) {
      await deps.setFieldScoped(field, values)
      return
    }
    const paths = cursorTracks(selection, tags.view, { path, index: target.index })
    if (paths.length > 0) await tags.edit(paths.map((one) => ({ kind: 'setOne', path: one, field, values })))
  }

  return async (action) => {
    const focus = document.activeElement
    if (textBoxOf(focus) !== null || (focus !== null && focus.closest('.cover-group') !== null)) {
      await tagsNativeEdit(action)
      return
    }
    if (deps.hasDialog()) return
    const target = targetOf()
    if (target === null) return
    const editable = tags.view?.editable === true
    deps.say(null)
    if (action === 'paste') {
      if (editable) await paste(target)
      return
    }
    if (!(await copyText(target.text)) || action === 'copy' || !editable || target.text === '') return
    if (target.path !== null && !readablePathsOf(tags.view).has(target.path)) return
    await deps.clearTarget({ field: target.field, path: target.path })
  }
}
