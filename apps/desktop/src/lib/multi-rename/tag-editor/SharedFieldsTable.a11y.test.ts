/** Tier 3 a11y tests for `SharedFieldsTable.svelte`: editable, with a «multiple values» row, and locked. */
import { afterEach, describe, it } from 'vitest'
import type { SharedRow } from '#lib/ipc/bindings.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import SharedFieldsTable from './SharedFieldsTable.svelte'
import { mountInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

const ROWS: SharedRow[] = [
  { field: 'artist', value: { same: ['Karel Capek'] }, choices: [], choicesMore: 0 },
  {
    field: 'album',
    value: 'multiple',
    choices: [
      { values: ['Book one'], files: 2 },
      { values: ['Book two'], files: 1 },
    ],
    choicesMore: 0,
  },
  { field: 'genre', value: { same: ['audiokniha', 'detektivka'] }, choices: [], choicesMore: 0 },
  { field: 'comment', value: 'empty', choices: [], choicesMore: 0 },
  { field: 'description', value: { same: ['Dlouhy popis'] }, choices: [], choicesMore: 0 },
]

async function mountTable(editable: boolean, badDates = 0): Promise<HTMLElement> {
  const rows: SharedRow[] =
    badDates > 0 ? [...ROWS, { field: 'date', value: { same: ['2010-20-09'] }, choices: [], choicesMore: 0 }] : ROWS
  const mounted = await mountInto(SharedFieldsTable, { rows, editable, badDates, onEdit: () => {} })
  cleanup = mounted.cleanup
  return mounted.root
}

describe('SharedFieldsTable a11y', () => {
  it('editable has no violations', async () => {
    await expectNoA11yViolations(await mountTable(true))
  })

  it('read-only has no violations', async () => {
    await expectNoA11yViolations(await mountTable(false))
  })

  it('a flagged Date row has no violations', async () => {
    await expectNoA11yViolations(await mountTable(true, 3))
  })
})
