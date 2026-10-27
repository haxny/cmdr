/**
 * Clashes in the DEFAULT shortcuts we've decided to keep, each with the reason it's safe. Shared by
 * `registry-conflicts.test.ts` (exact) and `conflict-detector-registry.test.ts` (by command set, so an
 * off-macOS spelling such as `Ctrl+1` is covered too). Adding a line is a deliberate product decision, not a
 * way to quiet a test: every entry means one of the two commands loses the combo in some state.
 */
export const allowedConflicts: { shortcut: string; commandIds: string[]; why: string }[] = [
  {
    shortcut: '⌘1',
    commandIds: ['multiRename.tabRename', 'view.fullMode'],
    why: 'The tab commands are fixed-key, not Tier 1, so the main window dispatches ⌘1 to Full view; the Multi-rename sheet claims ⌘1 for its Rename tab while it is open with the Tag editor setting on.',
  },
  {
    shortcut: '⌘2',
    commandIds: ['multiRename.tabTags', 'view.briefMode'],
    why: 'The tab commands are fixed-key, not Tier 1, so the main window dispatches ⌘2 to Brief view; the Multi-rename sheet claims ⌘2 for its Tags tab while it is open with the Tag editor setting on.',
  },
]

/** True when `commandIds` are exactly the commands of an allowed clash, whatever the combo's spelling. */
export function isAllowedConflictSet(commandIds: readonly string[]): boolean {
  const key = [...commandIds].sort().join(',')
  return allowedConflicts.some((entry) => [...entry.commandIds].sort().join(',') === key)
}
