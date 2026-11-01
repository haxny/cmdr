/**
 * The Multi-rename sheet's preset keys: F2 opens the Presets menu (Total Commander's
 * key for it) and ⌘S saves the current fields as a preset. Both are registry commands
 * (`multiRename.openPresets`, `multiRename.savePreset`), so the sheet asks the registry
 * rather than testing key flags.
 */

import { eventMatchesCommand } from '#lib/shortcuts/index.js'
import type { KeyRoad } from '#lib/commands/menu-claims.js'

/** What a keypress in the sheet means for its presets, if anything. */
export type PresetKey = 'openMenu' | 'save'

export function presetKeyOf(event: KeyboardEvent): PresetKey | null {
  // A key that finishes an IME composition belongs to the field, never to the sheet.
  if (event.isComposing) return null
  if (eventMatchesCommand(event, 'multiRename.openPresets')) return 'openMenu'
  if (eventMatchesCommand(event, 'multiRename.savePreset')) return 'save'
  return null
}

/** The two roads one F2 can arrive by: the sheet's keydown, and File > Rename's accelerator. */
export type { KeyRoad } from '#lib/commands/menu-claims.js'
export { createKeyRoadEcho } from '#lib/commands/menu-claims.js'

/**
 * What a `bind:this` ref to `PresetsControl` exposes: the sheet routes F2 and ⌘S to it. A typed
 * ref, since the type-aware linter can't see a component's exports.
 */
export interface PresetsControlApi {
  /** F2 from either road: toggles the Presets menu once per press. */
  pressOpenKey: (road: KeyRoad) => void
  /** ⌘S: the name popover, to save the fields as a preset. */
  openSave: () => void
}
