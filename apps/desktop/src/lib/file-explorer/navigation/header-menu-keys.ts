/**
 * Which command opens which pane's volume switcher (⌥F1 / ⌥F2 by default), so a header
 * menu can tell this pane's chooser key from the other pane's. Both header menus answer
 * these themselves: central dispatch is suppressed while one is open.
 */
import type { CommandId } from '$lib/commands'
import type { PaneId } from '$lib/commands/types'

export function chooserCommandFor(pane: PaneId): CommandId {
  return pane === 'left' ? 'pane.leftVolumeChooser' : 'pane.rightVolumeChooser'
}

export function otherChooserCommandFor(pane: PaneId): CommandId {
  return chooserCommandFor(pane === 'left' ? 'right' : 'left')
}
