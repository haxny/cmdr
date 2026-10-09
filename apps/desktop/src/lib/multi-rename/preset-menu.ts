/**
 * The presets menu (F2): load Default, a built-in, or a saved preset; save the
 * fields as a new preset; overwrite or delete the one they came from. Pure, so
 * its rows are testable without the sheet.
 */

import type { MultiRenamePreset } from '$lib/tauri-commands'
import type { MenuItem, MenuSection } from '$lib/ui/menu-types'
import { tString } from '$lib/intl/messages.svelte'
import { BUILT_IN_PRESETS } from './spec'

/** What a pick asks the sheet to do. */
export type PresetMenuPick = { kind: 'load'; id: string } | { kind: 'saveAs' } | { kind: 'save' } | { kind: 'delete' }

export interface PresetMenuState {
  saved: MultiRenamePreset[]
  /** The preset the fields came from (`default`, a built-in id, or a saved id), or `''`. */
  selectedId: string
  /** That preset when it's a saved one: Save and Delete act on it. */
  loaded: MultiRenamePreset | null
}

function loadRow(id: string, label: string, selectedId: string): MenuItem<PresetMenuPick> {
  return {
    value: `load:${id}`,
    label,
    check: id === selectedId ? { kind: 'current' } : undefined,
    data: { kind: 'load', id },
  }
}

export function presetMenuSections(state: PresetMenuState): MenuSection<PresetMenuPick>[] {
  const builtIn = [
    loadRow('default', tString('multiRename.preset.default'), state.selectedId),
    ...BUILT_IN_PRESETS.map((p) => loadRow(p.id, tString(p.nameKey), state.selectedId)),
  ]
  const saved = state.saved.map((p) => loadRow(p.id, p.name, state.selectedId))
  const manage: MenuItem<PresetMenuPick>[] = [
    { value: 'saveAs', label: tString('multiRename.presetsMenu.saveAs'), data: { kind: 'saveAs' } },
  ]
  if (state.loaded) {
    manage.push(
      {
        value: 'save',
        label: tString('multiRename.presetsMenu.save', { name: state.loaded.name }),
        data: { kind: 'save' },
      },
      {
        value: 'delete',
        label: tString('multiRename.presetsMenu.delete', { name: state.loaded.name }),
        data: { kind: 'delete' },
      },
    )
  }
  return [
    { id: 'builtIn', heading: tString('multiRename.presetsMenu.builtIn'), items: builtIn },
    {
      id: 'saved',
      heading: tString('multiRename.presetsMenu.saved'),
      items: saved,
      emptyLabel: tString('multiRename.presetsMenu.noSaved'),
    },
    { id: 'manage', items: manage },
  ]
}
