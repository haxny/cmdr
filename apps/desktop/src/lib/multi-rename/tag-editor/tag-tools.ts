/**
 * Which clean-up tools apply to which selection, and the exact `TagTool` each button sends. Pure; the tools run in
 * the backend over the whole session's files (`cmdr_tags::tools`).
 */

import type { CaseTool, TagField, TagTool } from '#lib/ipc/bindings.js'
import { FIELD_LABELS, TAG_FIELDS } from './field-labels'

/** What Case changes with no field focused: the names a person reads. */
export const DEFAULT_CASE_FIELDS: readonly TagField[] = ['title', 'album', 'artist', 'albumArtist', 'performer']

/** Diacritics stay allowed in Comment and Description only (the user's tag canon). */
export const DIACRITICS_FIELDS: readonly TagField[] = TAG_FIELDS.filter(
  (field) => field !== 'comment' && field !== 'description',
)

/** The field a tag box edits, by its id (`SharedFieldsTable`, `TrackList`); `null` for any other element. */
export function focusedFieldOf(id: string): TagField | null {
  if (/^tag-track-title-\d+$/.test(id)) return 'title'
  if (/^tag-track-number-\d+$/.test(id)) return 'track'
  const shared = /^tag-field-(\w+)$/.exec(id)?.[1]
  return shared !== undefined && Object.hasOwn(FIELD_LABELS, shared) ? (shared as TagField) : null
}

/** The fields Case changes: the focused one, or with none the default set. */
export function caseFields(focused: TagField | null): readonly TagField[] {
  return focused === null ? DEFAULT_CASE_FIELDS : [focused]
}

export function caseTool(change: CaseTool, focused: TagField | null): TagTool {
  return { kind: 'case', case: change, fields: [...caseFields(focused)] }
}

/** A single-file book has no track number, so Renumber is only offered for two files or more. */
export function showsRenumber(trackCount: number): boolean {
  return trackCount > 1
}

/** The extras the user checked, or `null` while none is: nothing is removed without an explicit choice. */
export function removeExtrasTool(checked: readonly string[]): TagTool | null {
  if (checked.length === 0) return null
  return { kind: 'removeExtras', keys: [...new Set(checked)].sort() }
}

/**
 * Runs a backend call over the tracks the Tags tab's tools work on: waits for pending edits, then hands the call the
 * checked tracks' paths (`null` for every track). Resolves `undefined` without calling when the checked tracks are
 * gone (the tab says so). The panels under the tools bar call their tools through it.
 */
export type RunScoped = <T>(call: (scope: string[] | null) => Promise<T>) => Promise<T | undefined>
