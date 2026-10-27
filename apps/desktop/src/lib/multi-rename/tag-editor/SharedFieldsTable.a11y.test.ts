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
  { field: 'artist', value: { same: ['Karel Capek'] }, choices: [] },
  {
    field: 'album',
    value: 'multiple',
    choices: [
      { values: ['Book one'], files: 2 },
      { values: ['Book two'], files: 1 },
    ],
  },
  { field: 'genre', value: { same: ['audiokniha', 'detektivka'] }, choices: [] },
  { field: 'comment', value: 'empty', choices: [] },
  { field: 'description', value: { same: ['Dlouhy popis'] }, choices: [] },
]

async function mountTable(editable: boolean): Promise<HTMLElement> {
  const mounted = await mountInto(SharedFieldsTable, { rows: ROWS, editable, onEdit: () => {} })
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
})
