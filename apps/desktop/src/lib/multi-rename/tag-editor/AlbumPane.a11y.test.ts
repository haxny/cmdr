/** Tier 3 a11y tests for `AlbumPane.svelte`: fields with «multiple values», a red Date, an open editor, locked. */
import { afterEach, describe, it, vi } from 'vitest'
import type { SharedRow } from '#lib/ipc/bindings.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import AlbumPane from './AlbumPane.svelte'
import { mountInto, press } from './tag-cell.test-helper'

vi.mock('#lib/settings/reactive-settings.svelte.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getRowHeight: () => 20,
  getStripedRows: () => true,
}))

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
      { values: ['A'], files: 1 },
      { values: ['B'], files: 2 },
    ],
    choicesMore: 0,
  },
  { field: 'date', value: { same: ['2010-20-09'] }, choices: [], choicesMore: 0 },
  { field: 'comment', value: 'empty', choices: [], choicesMore: 0 },
]

async function mountPane(editable = true, cursor = 1): Promise<HTMLElement> {
  const mounted = await mountInto(AlbumPane, {
    rows: ROWS,
    editable,
    badDates: 1,
    focused: true,
    cursor,
    onCursorChange: () => {},
    onEdit: () => {},
    onClear: () => {},
    onSwitchPane: () => {},
  })
  cleanup = mounted.cleanup
  return mounted.root
}

describe('AlbumPane a11y', () => {
  it('with fields, multiple values, and a red Date has no violations', async () => {
    await expectNoA11yViolations(await mountPane())
  })

  it('with the editor open has no violations', async () => {
    const root = await mountPane(true, 3)
    const list = root.querySelector<HTMLElement>('[role="listbox"]')
    if (!list) throw new Error('no listbox')
    await press(list, 'F2')
    await expectNoA11yViolations(root)
  })

  it('read-only has no violations', async () => {
    await expectNoA11yViolations(await mountPane(false))
  })
})
