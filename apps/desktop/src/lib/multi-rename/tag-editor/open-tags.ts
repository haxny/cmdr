/**
 * How the main window opens a Tags window. Every open makes a NEW Multi-Rename session and holds it for the window
 * (`holdTagsSession`), so the window owns it until it closes; the window gets no window-creation perms of its own.
 * Two roads: `file.editTags` from the focused pane's selection (`openTagsSession`: no media read), and the
 * Multi-Rename sheet's "Tags…" button, which copies the sheet's session (`reopenForTags`) rather than lending it,
 * so closing the sheet never ends the window's. Edit tags on files a Tags window already shows raises that window.
 * Off (`multiRename.tagEditor`) both do nothing, with no IPC.
 */

import { getSetting } from '#lib/settings/index.js'
import { getShowHiddenFiles } from '#lib/settings/reactive-settings.svelte.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { addToast } from '#lib/ui/toast/index.js'
import {
  closeMultiRename,
  holdTagsSession,
  openTagsSession,
  releaseTagsSession,
  reopenForTags,
  type MultiRenameError,
  type MultiRenameOpened,
  type TagsOpened,
} from '#lib/tauri-commands/index.js'
import { openTagsWindow } from './tags-window'

const log = getAppLogger('tagEditor')

/** What `file.editTags` needs from the explorer: the focused pane's selection, else its cursor row. */
export interface TagsSelectionSource {
  getFocusedPaneTagsTarget(): TagsSelection | null
}

export interface TagsSelection {
  listingId: string
  selectedIndices: number[] | null
  expectedSequence: number
  nothingSelected: boolean
}

/** Why `file.editTags` does nothing: the setting is off, or there is no selection and no cursor row (`..`, an empty pane, a pane with no listing). */
export type TagsOpenRefusal = 'off' | 'nothingSelected'

export function tagsOpenRefusal(settingOn: boolean, selection: TagsSelection | null): TagsOpenRefusal | null {
  if (!settingOn) return 'off'
  if (selection === null || selection.nothingSelected) return 'nothingSelected'
  return null
}

/** One open at a time: the menu accelerator and the keydown both fire on macOS. */
let opening = false

/** `file.editTags`: a Tags window over the focused pane's selection. */
export async function openTagsForSelection(explorer: TagsSelectionSource | undefined): Promise<void> {
  if (!getSetting('multiRename.tagEditor') || !explorer) return
  const selection = explorer.getFocusedPaneTagsTarget()
  if (tagsOpenRefusal(true, selection) !== null || selection === null) {
    addToast(tString('multiRename.tags.window.nothingSelected'), { level: 'info' })
    return
  }
  await openOnce(() =>
    openTagsSession(selection.listingId, getShowHiddenFiles(), selection.selectedIndices, selection.expectedSequence),
  )
}

/** What a refused open tells the user; the errors with no wording of their own say nothing, as before. */
function openFailureToast(error: MultiRenameError): { text: string; level: 'info' | 'warn' } | null {
  switch (error.type) {
    case 'selectionChanged':
      return { text: tString('multiRename.selectionChanged'), level: 'warn' }
    case 'tooManyTracks':
      return { text: tString('multiRename.tags.window.tooManyTracks', { limit: error.limit }), level: 'warn' }
    case 'timedOut':
      return { text: tString('multiRename.tags.window.openTimedOut'), level: 'warn' }
    case 'foldersNeedLocalDisk':
      return { text: tString('multiRename.tags.window.foldersNeedLocalDisk'), level: 'info' }
    default:
      return null
  }
}

/** The sheet's "Tags…": a Tags window over a copy of the sheet's session. */
export async function openTagsForSession(sheetSessionId: string): Promise<void> {
  if (!getSetting('multiRename.tagEditor')) return
  await openOnce(() => reopenForTags(sheetSessionId))
}

type Opened = { ok: true; value: TagsOpened | MultiRenameOpened } | { ok: false; error: MultiRenameError }

async function openOnce(open: () => Promise<Opened>): Promise<void> {
  if (opening) return
  opening = true
  try {
    const opened = await open()
    if (!opened.ok) {
      log.warn("couldn't open a session for the Tags window: {reason}", { reason: opened.error.type })
      const toast = openFailureToast(opened.error)
      if (toast !== null) addToast(toast.text, { level: toast.level })
      return
    }
    const unreadable = 'unreadable' in opened.value ? opened.value.unreadable : 0
    if (unreadable > 0) {
      addToast(tString('multiRename.tags.window.someUnreadable', { count: unreadable }), { level: 'info' })
    }
    await holdAndShow(opened.value.sessionId)
  } finally {
    opening = false
  }
}

/** Holds the new session and opens its window; a session with no track, or a window that won't open, goes again. */
async function holdAndShow(sessionId: string): Promise<void> {
  const held = await holdTagsSession(sessionId)
  if (!held.ok) {
    log.warn("couldn't hold the Tags window's session: {reason}", { reason: held.error.type })
    await giveBack(sessionId)
    return
  }
  // The same files are in a Tags window already: raise it (`openTagsWindow` does for a window it finds) rather than
  // open a second one over them.
  const alreadyOpen = held.value.alreadyOpen
  if (alreadyOpen !== null) {
    await giveBack(sessionId)
    await openTagsWindow(alreadyOpen, held.value.folderName, () => {})
    return
  }
  if (held.value.tracks === 0) {
    await giveBack(sessionId)
    addToast(tString('multiRename.tags.window.noTracks'), { level: 'info' })
    return
  }
  const failed = (): void => {
    void giveBack(sessionId)
  }
  try {
    await openTagsWindow(sessionId, held.value.folderName, failed)
  } catch (e) {
    log.warn("couldn't open the Tags window: {reason}", { reason: e })
    await giveBack(sessionId)
  }
}

async function giveBack(sessionId: string): Promise<void> {
  await releaseTagsSession(sessionId)
  await closeMultiRename(sessionId)
}
