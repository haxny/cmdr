/**
 * The dialog's option defaults and which options change matching. The backend owns the same defaults
 * (`crates/cmdr-sync`: `SyncOptions::default`, `ShowFilters::default`, `DEFAULT_EXCLUDES`); these mirror them for a
 * pair of roots the dialog has never seen, and `getSyncLastOptions` replaces them once it answers.
 */
import type { ProfileOverrides, ShowFilters, SyncOptions } from '#lib/tauri-commands/index.js'

/** `cmdr_sync::DEFAULT_EXCLUDES`, the folder exclude list a new pair starts with. */
export const DEFAULT_EXCLUDES: readonly string[] = [
  '.DS_Store',
  '._*',
  '.Trashes/',
  '.Spotlight-V100/',
  '.fseventsd/',
  'Thumbs.db',
]

/** A new pair: Both ways (never removes), date and size, subfolders on. */
export function defaultSyncOptions(): SyncOptions {
  return {
    mode: 'bothWays',
    compareBy: 'dateAndSize',
    subfolders: true,
    onlySelected: false,
    ignoreDate: false,
    emptyFolders: false,
    masks: '',
    excludes: [...DEFAULT_EXCLUDES],
  }
}

/** TC's set: everything but the equal rows, and no warnings-only view. */
export const DEFAULT_SHOW_FILTERS: Readonly<ShowFilters> = {
  toRight: true,
  equal: false,
  different: true,
  toLeft: true,
  duplicates: true,
  singles: true,
  unknown: true,
  warningsOnly: false,
}

/** No profile: every value follows Settings. Profiles arrive with the profiles menu. */
export const NO_OVERRIDES: Readonly<ProfileOverrides> = {
  keep: null,
  timeToleranceSecs: null,
  ignoreHourShift: null,
  timeOffsetSecs: 0,
  guards: null,
}

/**
 * Every option decides the plan the backend built at compare, so each one marks the grid stale (a "Compare again"
 * banner) rather than re-scanning on its own. Show-filters are separate and never re-compare.
 */
export type SyncOptionKey = keyof SyncOptions

/** Whether a show-filter key hides rows when it's in this state (`warningsOnly` hides when on). */
export function filterHides(filters: ShowFilters, key: keyof ShowFilters): boolean {
  return key === 'warningsOnly' ? filters.warningsOnly : !filters[key]
}

/** The exclude list as the options strip edits it: one entry per `;`. */
export function excludesFromText(text: string): string[] {
  return text
    .split(';')
    .map((entry) => entry.trim())
    .filter((entry) => entry !== '')
}

export function excludesToText(excludes: readonly string[]): string {
  return excludes.join('; ')
}
