/**
 * The Multi-Rename sheet's settings: the default (Total Commander's `<Default>`:
 * no change), the presets Cmdr seeds the list with, comparing two specs, and placeholder insertion. Pure, so the
 * sheet's state module and its tests share it.
 */

import type { MultiRenameSpec } from '#lib/tauri-commands/index.js'

/** TC's `<Default>`: every name stays as it is. */
export const DEFAULT_SPEC: MultiRenameSpec = {
  nameMask: '[N]',
  extensionMask: '[E]',
  search: '',
  replace: '',
  caseSensitive: false,
  firstOnly: false,
  includeExtension: false,
  regex: false,
  substitute: false,
  case: 'unchanged',
  removeDiacritics: false,
  greekToLatin: false,
  normalizeUnicode: false,
  maskSyntax: 'cmdr',
}

/**
 * A preset Cmdr starts the list with. Its name is a message key, read in the UI language when the
 * backend seeds the list (`presets.rs`'s `seed_in`); from then on it's an ordinary preset, the user's to
 * rename, update, move, or delete.
 */
export interface PresetSeedSource {
  /** The Cmdr version that introduced the seed (`seed_in`): a list takes a seed once, when its version is newer. */
  version: number
  nameKey:
    | 'multiRename.preset.dateAndTime'
    | 'multiRename.preset.removeDiacritics'
    | 'multiRename.preset.greekToLatin'
    | 'multiRename.preset.normalizeUnicode'
    | 'multiRename.preset.foobarAuthorYearAlbumTrack'
    | 'multiRename.preset.foobarArtistAlbum'
    | 'multiRename.preset.foobarAuthorYearAlbumTrackTitle'
    | 'multiRename.preset.foobarReadByDiscTrackTitle'
    | 'multiRename.preset.foobarReadByDiscTrack'
    | 'multiRename.preset.foobarReadByPublisherDiscTrack'
    | 'multiRename.preset.foobarSftRadioShow'
  spec: MultiRenameSpec
}

/**
 * A foobar2000 pattern as a preset: the pattern is the name mask, the extension stays, and Remove
 * diacritics is on (the user's tag canon names files without diacritics; `DEFAULT_SPEC` has it off).
 */
function foobarSpec(nameMask: string): MultiRenameSpec {
  return { ...DEFAULT_SPEC, nameMask, maskSyntax: 'foobar', removeDiacritics: true }
}

/** The seeds, top to bottom; version 2 ones follow the first four so they land after them in a new list. */
export const PRESET_SEEDS: PresetSeedSource[] = [
  { version: 1, nameKey: 'multiRename.preset.dateAndTime', spec: { ...DEFAULT_SPEC, nameMask: '[Y-M-D h.m.s]' } },
  { version: 1, nameKey: 'multiRename.preset.removeDiacritics', spec: { ...DEFAULT_SPEC, removeDiacritics: true } },
  { version: 1, nameKey: 'multiRename.preset.greekToLatin', spec: { ...DEFAULT_SPEC, greekToLatin: true } },
  { version: 1, nameKey: 'multiRename.preset.normalizeUnicode', spec: { ...DEFAULT_SPEC, normalizeUnicode: true } },
  {
    version: 2,
    nameKey: 'multiRename.preset.foobarAuthorYearAlbumTrack',
    spec: foobarSpec("[%album artist% - ]'('%year%')' %album% - %track%"),
  },
  { version: 2, nameKey: 'multiRename.preset.foobarArtistAlbum', spec: foobarSpec('[%artist% -] %album%') },
  {
    version: 2,
    nameKey: 'multiRename.preset.foobarAuthorYearAlbumTrackTitle',
    spec: foobarSpec("[%album artist% - ]'('%year%')' %album% - %track% %title%"),
  },
  {
    version: 2,
    nameKey: 'multiRename.preset.foobarReadByDiscTrackTitle',
    spec: foobarSpec(
      "[%album artist% - ]'('%year%')' %album% '('cte %performer%')' - %disc%$num(%tracknumber%,2) %title%",
    ),
  },
  {
    version: 2,
    nameKey: 'multiRename.preset.foobarReadByDiscTrack',
    spec: foobarSpec("[%album artist% - ]'('%year%')' %album% '('cte %performer%')' - %disc%$num(%tracknumber%,2)"),
  },
  {
    version: 2,
    nameKey: 'multiRename.preset.foobarReadByPublisherDiscTrack',
    spec: foobarSpec(
      "[%album artist% - ]'('%year%')' %album% '('cte %performer%, %publisher%')' - %disc%$num(%tracknumber%,2)",
    ),
  },
  {
    version: 2,
    nameKey: 'multiRename.preset.foobarSftRadioShow',
    spec: foobarSpec("SFT %date% '('[$num(%discnumber%,1)$num(%tracknumber%,2)]')' %title% '('%artist%')'"),
  },
]

/**
 * Whether two specs set the same thing. Field by field, so a spec that came back from
 * the backend (its own field order) still equals the one the sheet holds.
 */
export function specsEqual(a: MultiRenameSpec, b: MultiRenameSpec): boolean {
  return (Object.keys(DEFAULT_SPEC) as (keyof MultiRenameSpec)[]).every((field) =>
    // A spec saved before the syntax existed is a Cmdr one.
    field === 'maskSyntax' ? (a[field] ?? 'cmdr') === (b[field] ?? 'cmdr') : a[field] === b[field],
  )
}

/** Inserts `placeholder` into `mask` at the caret (or at the end). Returns the new mask and caret. */
export function insertAtCaret(
  mask: string,
  placeholder: string,
  caret: number | null,
): { mask: string; caret: number } {
  const at = caret === null ? mask.length : Math.max(0, Math.min(caret, mask.length))
  return { mask: mask.slice(0, at) + placeholder + mask.slice(at), caret: at + placeholder.length }
}
