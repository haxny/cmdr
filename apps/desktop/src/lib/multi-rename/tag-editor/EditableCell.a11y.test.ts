/** Tier 3 a11y tests for `EditableCell.svelte`: a labelled line, a labelled textarea, a placeholder, and disabled. */
import { afterEach, describe, it } from 'vitest'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import EditableCell from './EditableCell.svelte'
import { mountInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountCell(props: {
  multiline?: boolean
  disabled?: boolean
  placeholder?: string
}): Promise<HTMLElement> {
  const mounted = await mountInto(EditableCell, {
    id: 'cell',
    value: 'text',
    ariaLabel: 'Title of 01.mp3',
    onCommit: () => {},
    ...props,
  })
  cleanup = mounted.cleanup
  return mounted.root
}

describe('EditableCell a11y', () => {
  it('a line has no violations', async () => {
    await expectNoA11yViolations(await mountCell({}))
  })

  it('a textarea has no violations', async () => {
    await expectNoA11yViolations(await mountCell({ multiline: true }))
  })

  it('with a placeholder has no violations', async () => {
    await expectNoA11yViolations(await mountCell({ placeholder: '‹multiple values›' }))
  })

  it('disabled has no violations', async () => {
    await expectNoA11yViolations(await mountCell({ disabled: true }))
  })
})
