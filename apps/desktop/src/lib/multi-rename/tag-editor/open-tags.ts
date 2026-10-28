/**
 * How the main window opens a Tags window. Every open makes a NEW Multi-Rename session and holds it for the window
 * (`holdTagsSession`), so the window owns it until it closes; the window gets no window-creation perms of its own.
 * Two roads: `file.editTags` from the focused pane's selection (`openTagsSession`: no media read), and the
 * Multi-Rename sheet's "Tags…" button, which copies the sheet's session (`reopenForTags`) rather than lending it,
 * so closing the sheet never ends the window's.
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
} from '#lib/tauri-commands/index.js'
import { openTagsWindow } from './tags-window'

const log = getAppLogger('tagEditor')

/** What `file.editTags` needs from the explorer: the focused pane's selection, as Multi-Rename takes it. */
export interface TagsSelectionSource {
  getFocusedPaneRenameTarget(): TagsSelection | null
}

export interface TagsSelection {
  listingId: string
  selectedIndices: number[] | null
  expectedSequence: number
  nothingSelected: boolean
}

/** Why `file.editTags` does nothing: the setting is off, or nothing is selected (a pane with no listing included). */
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
  const selection = explorer.getFocusedPaneRenameTarget()
  if (tagsOpenRefusal(true, selection) !== null || selection === null) {
    addToast(tString('multiRename.tags.window.nothingSelected'), { level: 'info' })
    return
  }
  await openOnce(() =>
    openTagsSession(selection.listingId, getShowHiddenFiles(), selection.selectedIndices, selection.expectedSequence),
  )
}

/** The sheet's "Tags…": a Tags window over a copy of the sheet's session. */
export async function openTagsForSession(sheetSessionId: string): Promise<void> {
  if (!getSetting('multiRename.tagEditor')) return
  await openOnce(() => reopenForTags(sheetSessionId))
}

type Opened = { ok: true; value: MultiRenameOpened } | { ok: false; error: MultiRenameError }

async function openOnce(open: () => Promise<Opened>): Promise<void> {
  if (opening) return
  opening = true
  try {
    const opened = await open()
    if (!opened.ok) {
      log.warn("couldn't open a session for the Tags window: {reason}", { reason: opened.error.type })
      if (opened.error.type === 'selectionChanged') addToast(tString('multiRename.selectionChanged'), { level: 'warn' })
      return
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
