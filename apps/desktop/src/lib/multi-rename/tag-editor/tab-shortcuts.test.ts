/**
 * The Tags tab's two commands are in the registry for every user, setting on or off, so Settings › Keyboard shortcuts
 * shows them. Their keys must not add a conflict there: upstream ships none on macOS, and `getConflictCount()` drives
 * the badge, the "Conflicts" filter and each row's warning icon.
 */

import { describe, it, expect, vi } from 'vitest'

// The registry reads `isMacOS()` at module load, so the UA is stubbed before the imports (as `registry-conflicts.test.ts`).
vi.hoisted(() => {
  vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' })
})

import { findConflictsForShortcut, getConflictCount, getConflictingCommandIds } from '#lib/shortcuts/index.js'
import { getDefaultShortcuts } from '#lib/shortcuts/shortcuts-store.js'
import { TAB_COMMANDS } from './sheet-tab'

describe('the Tags tab commands in Settings › Keyboard shortcuts', () => {
  it('add no conflict: the count stays at upstream’s zero, setting on or off', () => {
    expect(getConflictCount()).toBe(0)
    for (const { command } of TAB_COMMANDS) expect(getConflictingCommandIds().has(command)).toBe(false)
  })

  it('are ⌘⇧[ / ⌘⇧], and each combo names only its own command', () => {
    expect(TAB_COMMANDS.map(({ command }) => getDefaultShortcuts(command))).toEqual([['⌘⇧['], ['⌘⇧]']])
    for (const { command } of TAB_COMMANDS) {
      const [shortcut] = getDefaultShortcuts(command)
      expect(findConflictsForShortcut(shortcut, 'Main window/Multi-rename').map((c) => c.id)).toEqual([command])
    }
  })
})
