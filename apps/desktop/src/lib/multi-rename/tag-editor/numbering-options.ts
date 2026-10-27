/**
 * The auto-numbering wizard's options: the defaults (the Mp3tag dialog's), begin-at validation, and the remembered
 * choices in `app-status.json` under `tagNumbering`. What's stored is validated field by field on load, so a
 * hand-edited or older value never breaks the wizard: each bad field falls back to its default.
 */
import { loadTagNumbering, saveTagNumbering } from '#lib/app-status-store.js'
import type { DiscOptions, NumberOptions, TagField } from '#lib/ipc/bindings.js'
import { TAG_FIELDS } from './field-labels'

export const DEFAULT_NUMBERING: NumberOptions = { trackStart: 1, trackTotal: true, trackPad: true, disc: null }

/** The disc section's choices when it is first switched on. */
export const DEFAULT_DISC: DiscOptions = {
  start: 1,
  total: true,
  pad: false,
  nextOnFolder: true,
  nextOnField: null,
  resetTrack: false,
  resetDiscOnField: null,
}

/** The field the "change of" selects offer first. */
export const DEFAULT_CHANGE_FIELD: TagField = 'album'

/** The most digits a begin-at box takes (the engine counts in 32 bits). */
const MAX_BEGIN_DIGITS = 9

/** A begin-at box's text as a number: whole, 0 or more. `null` for anything else. */
export function parseBegin(text: string): number | null {
  const trimmed = text.trim()
  return new RegExp(`^\\d{1,${String(MAX_BEGIN_DIGITS)}}$`).test(trimmed) ? Number(trimmed) : null
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

const asBegin = (value: unknown, fallback: number): number =>
  typeof value === 'number' && Number.isInteger(value) && value >= 0 && value < 10 ** MAX_BEGIN_DIGITS
    ? value
    : fallback

const asBool = (value: unknown, fallback: boolean): boolean => (typeof value === 'boolean' ? value : fallback)

const asField = (value: unknown): TagField | null => TAG_FIELDS.find((field) => field === value) ?? null

function normalizeDisc(value: unknown): DiscOptions | null {
  if (!isRecord(value)) return null
  return {
    start: asBegin(value.start, DEFAULT_DISC.start),
    total: asBool(value.total, DEFAULT_DISC.total),
    pad: asBool(value.pad, DEFAULT_DISC.pad),
    nextOnFolder: asBool(value.nextOnFolder, DEFAULT_DISC.nextOnFolder),
    nextOnField: asField(value.nextOnField),
    resetTrack: asBool(value.resetTrack, DEFAULT_DISC.resetTrack),
    resetDiscOnField: asField(value.resetDiscOnField),
  }
}

/** Whatever was stored, as valid options: each field that isn't right takes its default. */
export function normalizeNumbering(stored: unknown): NumberOptions {
  if (!isRecord(stored)) return DEFAULT_NUMBERING
  return {
    trackStart: asBegin(stored.trackStart, DEFAULT_NUMBERING.trackStart),
    trackTotal: asBool(stored.trackTotal, DEFAULT_NUMBERING.trackTotal),
    trackPad: asBool(stored.trackPad, DEFAULT_NUMBERING.trackPad),
    disc: normalizeDisc(stored.disc),
  }
}

/** The choices last made, or the defaults. */
export async function loadNumbering(): Promise<NumberOptions> {
  return normalizeNumbering(await loadTagNumbering())
}

export function saveNumbering(options: NumberOptions): Promise<void> {
  return saveTagNumbering(options)
}
