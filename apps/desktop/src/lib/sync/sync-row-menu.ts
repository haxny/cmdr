/**
 * The grid's row menu (right-click, ⌃-click, ⇧F10): every row key with its key shown, grouped TC's way: on the
 * selected rows, then on the row under the cursor. Picking a row runs the same `SyncKeyAction` as its key.
 */
import { tString } from '#lib/intl/messages.svelte.js'
import type { MenuItem, MenuSection } from '#lib/ui/menu-types.js'
import type { SyncKeyAction } from './sync-keys'
import { actionLabel, type SyncItemRow } from './row-label'

interface Entry {
  value: string
  label: string
  shortcut: string
  action: SyncKeyAction
  disabled?: boolean
}

function toItem(entry: Entry): MenuItem<SyncKeyAction> {
  return {
    value: entry.value,
    label: entry.label,
    shortcut: entry.shortcut,
    disabled: entry.disabled,
    data: entry.action,
  }
}

/** The menu for the row under the cursor (`null` on a folder header or an empty grid). */
export function rowMenuSections(row: SyncItemRow | null): MenuSection<SyncKeyAction>[] {
  const fileOn = (name: string | null, kind: string | null) => name !== null && kind !== 'folder'
  const leftFile = row !== null && fileOn(row.leftName, row.leftKind)
  const rightFile = row !== null && fileOn(row.rightName, row.rightKind)
  const selected: Entry[] = [
    {
      value: 'defaultDirection',
      label: tString('sync.menu.defaultDirection'),
      shortcut: '⌘D',
      action: { kind: 'bulk', bulk: 'defaultDirection' },
    },
    {
      value: 'copyRight',
      label: actionLabel('copyRight'),
      shortcut: '⌘L',
      action: { kind: 'bulk', bulk: 'copyRight' },
    },
    { value: 'copyLeft', label: actionLabel('copyLeft'), shortcut: '⌘R', action: { kind: 'bulk', bulk: 'copyLeft' } },
    {
      value: 'reverse',
      label: tString('sync.menu.reverse'),
      shortcut: '⌘I',
      action: { kind: 'bulk', bulk: 'reverse' },
    },
    { value: 'skip', label: actionLabel('skip'), shortcut: '⌘M', action: { kind: 'bulk', bulk: 'skip' } },
    {
      value: 'toggleSkip',
      label: tString('sync.menu.toggleSkip'),
      shortcut: 'Space',
      action: { kind: 'bulk', bulk: 'toggleSkip' },
    },
    {
      value: 'removeTarget',
      label: actionLabel('removeRight'),
      shortcut: '⌘⌫',
      action: { kind: 'bulk', bulk: 'removeTarget' },
    },
    { value: 'copyRows', label: tString('sync.menu.copyRows'), shortcut: '⌘C', action: { kind: 'copyRows' } },
  ]
  const onRow: Entry[] = [
    {
      value: 'compareContent',
      label: tString('sync.menu.compareContent'),
      shortcut: 'Enter',
      action: { kind: 'compareContent' },
      disabled: !(leftFile && rightFile),
    },
    {
      value: 'viewLeft',
      label: tString('sync.menu.viewLeft'),
      shortcut: 'F3',
      action: { kind: 'view', side: 'left' },
      disabled: !leftFile,
    },
    {
      value: 'viewRight',
      label: tString('sync.menu.viewRight'),
      shortcut: '⇧F3',
      action: { kind: 'view', side: 'right' },
      disabled: !rightFile,
    },
    {
      value: 'editLeft',
      label: tString('sync.menu.editLeft'),
      shortcut: 'F4',
      action: { kind: 'edit', side: 'left' },
      disabled: !leftFile,
    },
    {
      value: 'editRight',
      label: tString('sync.menu.editRight'),
      shortcut: '⇧F4',
      action: { kind: 'edit', side: 'right' },
      disabled: !rightFile,
    },
    {
      value: 'quickLook',
      label: tString('sync.menu.quickLook'),
      shortcut: '⌥Space',
      action: { kind: 'quickLook' },
      disabled: !(leftFile || rightFile),
    },
  ]
  return [
    { id: 'selected', heading: tString('sync.menu.selected'), items: selected.map(toItem) },
    { id: 'row', heading: tString('sync.menu.row'), items: onRow.map(toItem) },
  ]
}
