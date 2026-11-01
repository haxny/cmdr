/**
 * What a grid row says in words: its accessible name, and the category, action, and rung texts its tooltips use.
 * Colour and arrows only repeat these, so no row is ever told apart by colour alone (spec 9).
 */
import { tString } from '#lib/intl/messages.svelte.js'
import { formatUnitList } from '#lib/intl/list-format.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { Action, Category, Rung, ViewRowWire } from '#lib/tauri-commands/sync.js'

/** A plan row of the grid (the other kind is a folder header). */
export type SyncItemRow = Extract<ViewRowWire, { type: 'item' }>
/** A folder header row: "folder 3 of 12". */
export type SyncFolderRow = Extract<ViewRowWire, { type: 'folder' }>

const CATEGORY_LABEL: Record<Category, MessageKey> = {
  leftOnly: 'sync.category.label.leftOnly',
  rightOnly: 'sync.category.label.rightOnly',
  newerLeft: 'sync.category.label.newerLeft',
  newerRight: 'sync.category.label.newerRight',
  different: 'sync.category.label.different',
  equal: 'sync.category.label.equal',
  unknown: 'sync.category.label.unknown',
  typeClash: 'sync.category.label.typeClash',
  nameClash: 'sync.category.label.nameClash',
  unreadable: 'sync.category.label.unreadable',
}

const CATEGORY_SPOKEN: Record<Category, MessageKey> = {
  leftOnly: 'sync.category.spoken.leftOnly',
  rightOnly: 'sync.category.spoken.rightOnly',
  newerLeft: 'sync.category.spoken.newerLeft',
  newerRight: 'sync.category.spoken.newerRight',
  different: 'sync.category.spoken.different',
  equal: 'sync.category.spoken.equal',
  unknown: 'sync.category.spoken.unknown',
  typeClash: 'sync.category.spoken.typeClash',
  nameClash: 'sync.category.spoken.nameClash',
  unreadable: 'sync.category.spoken.unreadable',
}

const ACTION_LABEL: Record<Action, MessageKey> = {
  copyRight: 'sync.action.label.copyRight',
  copyLeft: 'sync.action.label.copyLeft',
  removeRight: 'sync.action.label.removeRight',
  removeLeft: 'sync.action.label.removeLeft',
  equal: 'sync.action.label.equal',
  skip: 'sync.action.label.skip',
  undecided: 'sync.action.label.undecided',
}

const ACTION_SPOKEN: Record<Action, MessageKey> = {
  copyRight: 'sync.action.spoken.copyRight',
  copyLeft: 'sync.action.spoken.copyLeft',
  removeRight: 'sync.action.spoken.removeRight',
  removeLeft: 'sync.action.spoken.removeLeft',
  equal: 'sync.action.spoken.equal',
  skip: 'sync.action.spoken.skip',
  undecided: 'sync.action.spoken.undecided',
}

const RUNG_LABEL: Record<Rung, MessageKey> = {
  presence: 'sync.grid.rung.presence',
  sizeAndTime: 'sync.grid.rung.sizeAndTime',
  sizeOnly: 'sync.grid.rung.sizeOnly',
  content: 'sync.grid.rung.content',
}

/** TC's arrows: the shape that says what the run does, beside its colour. */
export const ACTION_GLYPH: Record<Action, string> = {
  copyRight: '→',
  copyLeft: '←',
  removeRight: '×→',
  removeLeft: '←×',
  equal: '=',
  skip: '·',
  undecided: '?',
}

/** The rung badge: one letter, its words in the tooltip and for screen readers. */
export const RUNG_BADGE: Record<Rung, string> = {
  presence: '',
  sizeAndTime: 'T',
  sizeOnly: 'S',
  content: 'C',
}

export function categoryLabel(category: Category): string {
  return tString(CATEGORY_LABEL[category])
}

export function actionLabel(action: Action): string {
  return tString(ACTION_LABEL[action])
}

export function rungLabel(rung: Rung): string {
  return tString(RUNG_LABEL[rung])
}

/** The name the row shows: the left spelling, both when the sides spell it differently. */
export function rowName(row: SyncItemRow): string {
  const { leftName, rightName } = row
  if (leftName !== null && rightName !== null && leftName !== rightName) return `${leftName} / ${rightName}`
  return leftName ?? rightName ?? ''
}

/** Whether the person changed this row's action from the one the mode chose. */
export function isChangedByYou(row: SyncItemRow): boolean {
  return row.action !== row.defaultAction
}

/** "report.pdf, newer on the left, will be copied to the right, changed by you". */
export function rowAccessibleName(row: SyncItemRow): string {
  const parts = [rowName(row)]
  if (row.leftKind === 'folder' || row.rightKind === 'folder') parts.push(tString('sync.grid.spoken.folder'))
  parts.push(tString(CATEGORY_SPOKEN[row.category]), tString(ACTION_SPOKEN[row.action]))
  if (isChangedByYou(row)) parts.push(tString('sync.grid.spoken.changedByYou'))
  if (row.olderOverwritesNewer) parts.push(tString('sync.grid.spoken.olderReplacesNewer'))
  if (row.content === 'pending') parts.push(tString('sync.grid.spoken.comparingContent'))
  return formatUnitList(parts)
}
