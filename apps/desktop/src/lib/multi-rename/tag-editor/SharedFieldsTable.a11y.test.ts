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
  { field: 'artist', value: { same: ['Karel Capek'] } },
  { field: 'album', value: 'multiple' },
  { field: 'genre', value: { same: ['audiokniha', 'detektivka'] } },
  { field: 'comment', value: 'empty' },
  { field: 'description', value: { same: ['Dlouhy popis'] } },
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
