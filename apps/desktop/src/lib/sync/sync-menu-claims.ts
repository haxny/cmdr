/**
 * The dialog's keys that are also native menu accelerators. AppKit may run the menu item before (or instead of)
 * handing the page the key, so while the dialog is open it claims those commands (`#lib/commands/menu-claims`) and
 * runs its own action for them; the dialog drops the second half of a keyboard + menu double fire
 * (`createKeyRoadEcho`). A row key declines when focus isn't on the grid, as its keydown does, so ⌘C and ⌘A still
 * edit a text field.
 *
 * ⌘M (TC's skip) is the Window menu's Minimize, a predefined item AppKit runs itself with no way to claim it, so
 * skip moved to ⌘N.
 */
import type { CommandId } from '#lib/commands/command-ids.js'
import type { SyncKeyAction } from './sync-keys'

export interface SyncMenuKey {
  command: CommandId
  action: SyncKeyAction
  /** A row key: only while the grid has focus (else the claim declines). */
  gridOnly: boolean
}

export const SYNC_MENU_KEYS: readonly SyncMenuKey[] = [
  { command: 'file.duplicate', action: { kind: 'bulk', bulk: 'defaultDirection' }, gridOnly: true }, // ⌘D
  { command: 'file.getInfo', action: { kind: 'bulk', bulk: 'reverse' }, gridOnly: true }, // ⌘I
  { command: 'edit.copy', action: { kind: 'copyRows' }, gridOnly: true }, // ⌘C
  { command: 'selection.selectAll', action: { kind: 'selectAll' }, gridOnly: true }, // ⌘A
  { command: 'file.view', action: { kind: 'view', side: 'left' }, gridOnly: true }, // F3
  { command: 'file.edit', action: { kind: 'edit', side: 'left' }, gridOnly: true }, // F4
  { command: 'file.newFile', action: { kind: 'edit', side: 'right' }, gridOnly: true }, // ⇧F4
  { command: 'pane.swap', action: { kind: 'swap' }, gridOnly: false }, // ⌘U
  { command: 'file.rename', action: { kind: 'profiles' }, gridOnly: false }, // F2
  { command: 'file.editTags', action: { kind: 'filter', filter: 'equal' }, gridOnly: false }, // ⌘⇧E
]
