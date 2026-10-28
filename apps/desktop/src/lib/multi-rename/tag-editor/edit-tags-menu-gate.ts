/**
 * Keeps File > "Edit tags…" (`file.editTags`) greyed while the opt-in `multiRename.tagEditor` setting is off, and
 * live while it's on: pushed at main-window startup and on every change of the setting. The item is built disabled,
 * so a user who never turns the setting on never sees it live. ⚠️ Chrome only: a disabled item's accelerator can
 * still fire, so `open-tags.ts` checks the setting itself. Main window only, like `open-terminal/menu-gate`.
 */

import { getSetting, onSpecificSettingChange } from '#lib/settings/index.js'
import { setEditTagsEnabled } from '#lib/tauri-commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('tagEditor')

function push(enabled: boolean): void {
  void setEditTagsEnabled(enabled).catch((err: unknown) => {
    log.warn("Couldn't update the File menu's Edit tags item: {error}", { error: err })
  })
}

/** Starts the sync and returns its stop function. */
export function startEditTagsMenuGate(): () => void {
  push(getSetting('multiRename.tagEditor'))
  return onSpecificSettingChange('multiRename.tagEditor', push)
}
