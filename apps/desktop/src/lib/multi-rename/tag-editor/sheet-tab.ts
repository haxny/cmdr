import type { CommandId } from '#lib/commands/index.js'

export type SheetTab = 'rename' | 'tags'

/** The registry command that picks each tab (⌘1 and ⌘2). */
export const TAB_COMMANDS: readonly { tab: SheetTab; command: CommandId }[] = [
  { tab: 'rename', command: 'multiRename.tabRename' },
  { tab: 'tags', command: 'multiRename.tabTags' },
]
